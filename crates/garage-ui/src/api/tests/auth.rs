
    fn capability_body(version: &Value, buckets: &str, keys: &str) -> Value {
        json!({
            "version": version,
            "listBucketsInfo": buckets,
            "listKeysInfo": keys,
        })
    }

    #[tokio::test]
    async fn capabilities_are_unknown_before_any_list_and_a_viewer_can_read_them() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_task = Arc::clone(&hits);
        let task = tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                hits_task.fetch_add(1, Ordering::SeqCst);
                drop(socket);
            }
        });
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, &format!("http://127.0.0.1:{port}")).await;
        let (admin, _) = login(&app).await;
        let viewer = viewer_session(&app, &admin).await;
        let missing = exchange(&app, request("GET", "/api/capabilities", None, None)).await;
        assert_error(
            &missing,
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "Login required",
        );
        let health = exchange(&app, request("GET", "/healthz", None, None)).await;
        assert_eq!(health.status, StatusCode::OK, "{}", health.body);
        let health_body: Value = serde_json::from_str(&health.body).unwrap();
        let response = exchange(
            &app,
            request("GET", "/api/capabilities", None, Some(&viewer)),
        )
        .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        assert_eq!(
            serde_json::from_str::<Value>(&response.body).unwrap(),
            capability_body(&health_body["version"], "unknown", "unknown")
        );
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        let bare = crate::util::temp_dir();
        let plain = state::test_app(&bare).await;
        let (plain_admin, _) = login(&plain).await;
        let plain_caps = exchange(
            &plain,
            request("GET", "/api/capabilities", None, Some(&plain_admin)),
        )
        .await;
        assert_eq!(
            serde_json::from_str::<Value>(&plain_caps.body).unwrap(),
            capability_body(&health_body["version"], "unknown", "unknown")
        );
        task.abort();
    }

    #[tokio::test]
    async fn capabilities_follow_info_404_and_info_200() {
        let absent = InfoSocket::start(404).await;
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, &absent.url).await;
        let (admin, _) = login(&app).await;
        let health = exchange(&app, request("GET", "/healthz", None, None)).await;
        let health_body: Value = serde_json::from_str(&health.body).unwrap();
        let before = exchange(
            &app,
            request("GET", "/api/capabilities", None, Some(&admin)),
        )
        .await;
        assert_eq!(
            serde_json::from_str::<Value>(&before.body).unwrap(),
            capability_body(&health_body["version"], "unknown", "unknown")
        );
        assert_eq!(absent.hits(), 0);
        let buckets = exchange(
            &app,
            request("GET", "/api/buckets?limit=1", None, Some(&admin)),
        )
        .await;
        assert_eq!(buckets.status, StatusCode::OK, "{}", buckets.body);
        let after_buckets = exchange(
            &app,
            request("GET", "/api/capabilities", None, Some(&admin)),
        )
        .await;
        let after_buckets_hits = absent.hits();
        assert_eq!(
            serde_json::from_str::<Value>(&after_buckets.body).unwrap(),
            capability_body(&health_body["version"], "absent", "unknown")
        );
        let again = exchange(
            &app,
            request("GET", "/api/capabilities", None, Some(&admin)),
        )
        .await;
        assert_eq!(again.body, after_buckets.body);
        assert_eq!(absent.hits(), after_buckets_hits);
        let keys = exchange(
            &app,
            request("GET", "/api/keys?limit=1", None, Some(&admin)),
        )
        .await;
        assert_eq!(keys.status, StatusCode::OK, "{}", keys.body);
        let after_keys = exchange(
            &app,
            request("GET", "/api/capabilities", None, Some(&admin)),
        )
        .await;
        assert_eq!(
            serde_json::from_str::<Value>(&after_keys.body).unwrap(),
            capability_body(&health_body["version"], "absent", "absent")
        );

        let present = InfoSocket::start(200).await;
        let other = crate::util::temp_dir();
        let app2 = app_for(&other, &present.url).await;
        let (admin2, _) = login(&app2).await;
        let listed = exchange(
            &app2,
            request("GET", "/api/buckets?limit=1", None, Some(&admin2)),
        )
        .await;
        assert_eq!(listed.status, StatusCode::OK, "{}", listed.body);
        let shown = exchange(
            &app2,
            request("GET", "/api/capabilities", None, Some(&admin2)),
        )
        .await;
        let shown_hits = present.hits();
        assert_eq!(
            serde_json::from_str::<Value>(&shown.body).unwrap(),
            capability_body(&health_body["version"], "present", "unknown")
        );
        let shown_again = exchange(
            &app2,
            request("GET", "/api/capabilities", None, Some(&admin2)),
        )
        .await;
        assert_eq!(shown_again.body, shown.body);
        assert_eq!(present.hits(), shown_hits);
        let listed_keys = exchange(
            &app2,
            request("GET", "/api/keys?limit=1", None, Some(&admin2)),
        )
        .await;
        assert_eq!(listed_keys.status, StatusCode::OK, "{}", listed_keys.body);
        let both = exchange(
            &app2,
            request("GET", "/api/capabilities", None, Some(&admin2)),
        )
        .await;
        assert_eq!(
            serde_json::from_str::<Value>(&both.body).unwrap(),
            capability_body(&health_body["version"], "present", "present")
        );
    }

    #[tokio::test]
    async fn connection_failure_leaves_capabilities_unknown() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_task = Arc::clone(&hits);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                hits_task.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0_u8; 1024];
                let _ = socket.read(&mut buf).await;
                let _ = socket.shutdown().await;
            }
        });
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, &format!("http://127.0.0.1:{port}")).await;
        let (admin, _) = login(&app).await;
        let failed = exchange(
            &app,
            request("GET", "/api/buckets?limit=1", None, Some(&admin)),
        )
        .await;
        assert_error(
            &failed,
            StatusCode::BAD_GATEWAY,
            "garage_unavailable",
            "Garage admin API is not reachable",
        );
        let tried = hits.load(Ordering::SeqCst);
        assert!(tried > 0, "the list did not reach the upstream");
        let health = exchange(&app, request("GET", "/healthz", None, None)).await;
        let health_body: Value = serde_json::from_str(&health.body).unwrap();
        let response = exchange(
            &app,
            request("GET", "/api/capabilities", None, Some(&admin)),
        )
        .await;
        assert_eq!(
            serde_json::from_str::<Value>(&response.body).unwrap(),
            capability_body(&health_body["version"], "unknown", "unknown")
        );
        assert_eq!(hits.load(Ordering::SeqCst), tried);
        task.abort();
    }

    #[tokio::test]
    async fn csrf_rejection_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let (admin, _) = login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let rejected = exchange(
            &app,
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header("cookie", cookie_header(&admin))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_error(
            &rejected,
            StatusCode::FORBIDDEN,
            "csrf_rejected",
            "CSRF token missing or mismatched",
        );
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.result, "denied");
        assert_eq!(row.reason, "csrf_rejected");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/logout");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 403);
        assert_eq!(row.username, "Ada");
        assert!(!audit_text(row).contains(&admin.csrf));
        let status = exchange(
            &app,
            request("GET", "/api/auth/status", None, Some(&admin)),
        )
        .await;
        assert_eq!(status.status, StatusCode::OK, "{}", status.body);
        assert_eq!(
            serde_json::from_str::<Value>(&status.body).unwrap()["authenticated"],
            true
        );
    }

    #[tokio::test]
    async fn csrf_rejection_records_the_last_forwarded_hop() {
        let dir = crate::util::temp_dir();
        let app = state::test_app_with(
            &dir,
            &[
                ("GARAGE_ADMIN_URL", "http://127.0.0.1:9"),
                ("GARAGE_ADMIN_TOKEN", TOKEN),
                ("TRUSTED_PROXY", "true"),
            ],
            test_quota(),
        )
        .await;
        let (admin, _) = login(&app).await;
        let rejected = exchange(
            &app,
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header("cookie", cookie_header(&admin))
                .header("x-forwarded-for", "198.51.100.10, 203.0.113.55, ")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_error(
            &rejected,
            StatusCode::FORBIDDEN,
            "csrf_rejected",
            "CSRF token missing or mismatched",
        );
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        let row = rows
            .iter()
            .find(|row| row.reason == "csrf_rejected")
            .expect("csrf rejection audit");
        assert_eq!(row.ip, "203.0.113.55");
        assert_eq!(row.username, "Ada");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/logout");
        assert_eq!(row.status, 403);
        assert!(!audit_text(row).contains("198.51.100.10"));
        assert!(!audit_text(row).contains(&admin.csrf));
    }

    #[tokio::test]
    async fn setup_invalid_username_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let username = "bad name";
        let response = exchange(
            &app,
            request(
                "POST",
                "/api/setup",
                Some(&json!({ "username": username, "password": PASSWORD }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{}", response.body);
        assert_eq!(code_of(&response.body), "invalid_username");
        assert!(!response.body.contains(PASSWORD));
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, username);
        assert_eq!(row.result, "error");
        assert_eq!(row.reason, "invalid_username");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/setup");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 400);
        assert!(!audit_text(row).contains(PASSWORD));
        let sink = app.log_sink.lock().expect("logs").join("\n");
        assert!(!sink.contains(PASSWORD));
    }

    #[tokio::test]
    async fn setup_invalid_password_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let submitted = "badpass";
        let response = exchange(
            &app,
            request(
                "POST",
                "/api/setup",
                Some(&json!({ "username": "Ada", "password": submitted }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{}", response.body);
        assert_eq!(code_of(&response.body), "password_invalid");
        assert!(!response.body.contains(submitted));
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "error");
        assert_eq!(row.reason, "password_invalid");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/setup");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 400);
        assert!(!audit_text(row).contains(submitted));
        let sink = app.log_sink.lock().expect("logs").join("\n");
        assert!(!sink.contains(submitted));
    }

    #[tokio::test]
    async fn setup_already_initialized_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let username = "Other";
        let response = exchange(
            &app,
            request(
                "POST",
                "/api/setup",
                Some(&json!({ "username": username, "password": PASSWORD }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(response.status, StatusCode::CONFLICT, "{}", response.body);
        assert_eq!(code_of(&response.body), "already_initialized");
        assert!(!response.body.contains(PASSWORD));
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, username);
        assert_eq!(row.result, "error");
        assert_eq!(row.reason, "already_initialized");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/setup");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 409);
        assert!(!audit_text(row).contains(PASSWORD));
        let sink = app.log_sink.lock().expect("logs").join("\n");
        assert!(!sink.contains(PASSWORD));
    }

    #[tokio::test]
    async fn change_password_invalid_new_password_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let (session, _) = login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let submitted = "badpass";
        let response = exchange(
            &app,
            request(
                "POST",
                "/api/auth/change-password",
                Some(
                    &json!({
                        "current_password": PASSWORD,
                        "new_password": submitted
                    })
                    .to_string(),
                ),
                Some(&session),
            ),
        )
        .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{}", response.body);
        assert_eq!(code_of(&response.body), "password_invalid");
        assert!(!response.body.contains(submitted));
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.result, "error");
        assert_eq!(row.reason, "password_invalid");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/change-password");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 400);
        assert!(!audit_text(row).contains(submitted));
        let sink = app.log_sink.lock().expect("logs").join("\n");
        assert!(!sink.contains(submitted));
    }

    #[tokio::test]
    async fn password_change_drops_the_other_session() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let (session_a, _) = login(&app).await;
        let logged_in = exchange(
            &app,
            request(
                "POST",
                "/api/auth/login",
                Some(&json!({ "username": "Ada", "password": PASSWORD }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(logged_in.status, StatusCode::OK, "{}", logged_in.body);
        let session_b = session_from(&logged_in.headers);
        let changed = exchange(
            &app,
            request(
                "POST",
                "/api/auth/change-password",
                Some(
                    &json!({
                        "current_password": PASSWORD,
                        "new_password": "updated horse"
                    })
                    .to_string(),
                ),
                Some(&session_a),
            ),
        )
        .await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
        let replacement = session_from(&changed.headers);
        assert_ne!(replacement.id, session_a.id);

        let other = exchange(&app, request("GET", "/api/config", None, Some(&session_b))).await;
        assert_eq!(other.status, StatusCode::UNAUTHORIZED);
        assert_eq!(code_of(&other.body), "unauthenticated");

        let stale = exchange(&app, request("GET", "/api/config", None, Some(&session_a))).await;
        assert_eq!(stale.status, StatusCode::UNAUTHORIZED);
        assert_eq!(code_of(&stale.body), "unauthenticated");

        let current =
            exchange(&app, request("GET", "/api/config", None, Some(&replacement))).await;
        assert_eq!(current.status, StatusCode::OK, "{}", current.body);
    }

    #[tokio::test]
    async fn setup_success_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let response = exchange(
            &app,
            request(
                "POST",
                "/api/setup",
                Some(&json!({ "username": "Ada", "password": PASSWORD }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "success");
        assert_eq!(row.reason, "setup");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/setup");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 200);
        assert!(!audit_text(row).contains(PASSWORD));
    }

    #[tokio::test]
    async fn setup_rate_limit_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        // Freeze the limiter clock before the first setup request, which already spends the
        // setup budget: a loaded machine must not refill it before the eleventh request.
        app.limits.clock().freeze();
        let created = exchange(
            &app,
            request(
                "POST",
                "/api/setup",
                Some(&json!({ "username": "Ada", "password": PASSWORD }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::OK, "{}", created.body);
        for _ in 0..9 {
            let conflict = exchange(
                &app,
                request(
                    "POST",
                    "/api/setup",
                    Some(&json!({ "username": "Ada", "password": PASSWORD }).to_string()),
                    None,
                ),
            )
            .await;
            assert_eq!(conflict.status, StatusCode::CONFLICT, "{}", conflict.body);
            assert_eq!(code_of(&conflict.body), "already_initialized");
        }
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let blocked = exchange(
            &app,
            request(
                "POST",
                "/api/setup",
                Some(&json!({ "username": "Ada", "password": PASSWORD }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS, "{}", blocked.body);
        assert_eq!(code_of(&blocked.body), "rate_limited");
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "rate_limited");
        assert_eq!(row.reason, "rate_limited");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/setup");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 429);
        assert!(!audit_text(row).contains(PASSWORD));
    }

    #[tokio::test]
    async fn login_failure_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let submitted = "wrong horse";
        let failed = exchange(
            &app,
            request(
                "POST",
                "/api/auth/login",
                Some(&json!({ "username": "Ada", "password": submitted }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
        assert_eq!(code_of(&failed.body), "invalid_credentials");
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "failure");
        assert_eq!(row.reason, "invalid_credentials");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/login");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 401);
        assert!(!audit_text(row).contains(submitted));
        assert!(!audit_text(row).contains(PASSWORD));
    }

    #[tokio::test]
    async fn login_with_an_illegal_username_skips_the_username_limiter_and_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let failed = exchange(
            &app,
            request(
                "POST",
                "/api/auth/login",
                Some(&json!({ "username": "x".repeat(65), "password": "wrong horse" }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
        assert_eq!(code_of(&failed.body), "invalid_credentials");
        assert_eq!(app.limits.login_len(), 0);
        // It still spends the client address's budget (one key for 127.0.0.1).
        assert_eq!(app.limits.login_ip_len(), 1);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        assert_eq!(rows[0].result, "failure");
        assert_eq!(rows[0].reason, "invalid_credentials");
        assert_eq!(rows[0].status, 401);
    }

    #[tokio::test]
    async fn login_rate_limit_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        // Freeze the limiter clock: slow bcrypt or a loaded machine must not refill the quota.
        app.limits.clock().freeze();
        let submitted = "wrong horse";
        for _ in 0..10 {
            let failed = exchange(
                &app,
                request(
                    "POST",
                    "/api/auth/login",
                    Some(&json!({ "username": "Ada", "password": submitted }).to_string()),
                    None,
                ),
            )
            .await;
            assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
            assert_eq!(code_of(&failed.body), "invalid_credentials");
        }
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let blocked = exchange(
            &app,
            request(
                "POST",
                "/api/auth/login",
                Some(&json!({ "username": "Ada", "password": submitted }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS, "{}", blocked.body);
        assert_eq!(code_of(&blocked.body), "rate_limited");
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "rate_limited");
        assert_eq!(row.reason, "rate_limited");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/login");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 429);
        assert!(!audit_text(row).contains(submitted));
        assert!(!audit_text(row).contains(PASSWORD));
    }

    #[tokio::test]
    async fn login_success_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let logged_in = exchange(
            &app,
            request(
                "POST",
                "/api/auth/login",
                Some(&json!({ "username": "Ada", "password": PASSWORD }).to_string()),
                None,
            ),
        )
        .await;
        assert_eq!(logged_in.status, StatusCode::OK, "{}", logged_in.body);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "success");
        assert_eq!(row.reason, "login");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/login");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 200);
        assert!(!audit_text(row).contains(PASSWORD));
    }

    #[tokio::test]
    async fn logout_success_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let (session, _) = login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let logged_out = exchange(
            &app,
            request("POST", "/api/auth/logout", None, Some(&session)),
        )
        .await;
        assert_eq!(logged_out.status, StatusCode::NO_CONTENT, "{}", logged_out.body);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "success");
        assert_eq!(row.reason, "logout");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/logout");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 204);
        assert!(!audit_text(row).contains(PASSWORD));
    }

    #[tokio::test]
    async fn change_password_invalid_credentials_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let (session, _) = login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let submitted = "wrong horse";
        let replacement = "updated horse";
        let failed = exchange(
            &app,
            request(
                "POST",
                "/api/auth/change-password",
                Some(
                    &json!({
                        "current_password": submitted,
                        "new_password": replacement
                    })
                    .to_string(),
                ),
                Some(&session),
            ),
        )
        .await;
        assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
        assert_eq!(code_of(&failed.body), "invalid_credentials");
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "failure");
        assert_eq!(row.reason, "invalid_credentials");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/change-password");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 401);
        assert!(!audit_text(row).contains(submitted));
        assert!(!audit_text(row).contains(replacement));
        assert!(!audit_text(row).contains(PASSWORD));
    }

    #[tokio::test]
    async fn change_password_rate_limit_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let (session, _) = login(&app).await;
        // Freeze the limiter clock: slow bcrypt or a loaded machine must not refill the quota.
        app.limits.clock().freeze();
        let submitted = "wrong horse";
        let replacement = "updated horse";
        for _ in 0..10 {
            let failed = exchange(
                &app,
                request(
                    "POST",
                    "/api/auth/change-password",
                    Some(
                        &json!({
                            "current_password": submitted,
                            "new_password": replacement
                        })
                        .to_string(),
                    ),
                    Some(&session),
                ),
            )
            .await;
            assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
            assert_eq!(code_of(&failed.body), "invalid_credentials");
        }
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let blocked = exchange(
            &app,
            request(
                "POST",
                "/api/auth/change-password",
                Some(
                    &json!({
                        "current_password": submitted,
                        "new_password": replacement
                    })
                    .to_string(),
                ),
                Some(&session),
            ),
        )
        .await;
        assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS, "{}", blocked.body);
        assert_eq!(code_of(&blocked.body), "rate_limited");
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "rate_limited");
        assert_eq!(row.reason, "rate_limited");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/change-password");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 429);
        assert!(!audit_text(row).contains(submitted));
        assert!(!audit_text(row).contains(replacement));
        assert!(!audit_text(row).contains(PASSWORD));
    }

    #[tokio::test]
    async fn change_password_success_is_audited() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let (session, _) = login(&app).await;
        let before = crate::db::recent_audit(&app.db).await.unwrap();
        let replacement = "updated horse";
        let changed = exchange(
            &app,
            request(
                "POST",
                "/api/auth/change-password",
                Some(
                    &json!({
                        "current_password": PASSWORD,
                        "new_password": replacement
                    })
                    .to_string(),
                ),
                Some(&session),
            ),
        )
        .await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows.len(), before.len() + 1, "{rows:?}");
        let row = &rows[0];
        assert_eq!(row.username, "Ada");
        assert_eq!(row.result, "success");
        assert_eq!(row.reason, "password_changed");
        assert_eq!(row.method, "POST");
        assert_eq!(row.path, "/api/auth/change-password");
        assert_eq!(row.target, "");
        assert_eq!(row.status, 200);
        assert!(!audit_text(row).contains(PASSWORD));
        assert!(!audit_text(row).contains(replacement));
    }

    #[tokio::test]
    async fn change_password_preserves_live_permissions_and_rejects_revoked_authentication() {
        for mutation in [
            "permissions",
            "role",
            "disabled",
            "password",
            "session",
            "demoted",
        ] {
            let dir = crate::util::temp_dir();
            let app = state::test_app(&dir).await;
            let (session, _) = login(&app).await;
            let mut headers = HeaderMap::new();
            headers.insert(
                "cookie",
                HeaderValue::from_str(&cookie_header(&session)).unwrap(),
            );
            if mutation == "permissions" {
                let user = crate::db::find_user_by_name(&app.db, "Ada")
                    .await
                    .unwrap()
                    .unwrap();
                crate::db::update_user(&app.db, user.id, "viewer", true, true, false, None, None)
                    .await
                    .unwrap();
            }
            let current = crate::session::authenticate(&app, &headers, "127.0.0.1")
                .await
                .unwrap();
            let id = current.user.id;
            let original_hash = current.user.password_hash.clone();
            match mutation {
                "permissions" | "role" => {
                    crate::db::update_user(&app.db, id, "viewer", false, false, false, None, None)
                        .await
                        .unwrap()
                }
                "disabled" => crate::db::update_user(&app.db, id, "admin", false, false, true, None, None)
                    .await
                    .unwrap(),
                "password" => {
                    let changed_hash = crate::password::hash_async("other password".into(), 4)
                        .await
                        .unwrap();
                    crate::db::update_user(
                        &app.db,
                        id,
                        "admin",
                        false,
                        false,
                        false,
                        None,
                        Some(&changed_hash),
                    )
                    .await
                    .unwrap();
                }
                "session" => crate::db::delete_user_sessions(&app.db, id).await.unwrap(),
                "demoted" => {
                    crate::db::update_user(&app.db, id, "viewer", false, false, false, None, None)
                        .await
                        .unwrap();
                    crate::db::delete_user_sessions(&app.db, id).await.unwrap();
                }
                _ => unreachable!(),
            }
            let before = crate::db::find_user_by_id(&app.db, id)
                .await
                .unwrap()
                .unwrap();
            let body = serde_json::from_value(
                json!({"current_password": PASSWORD, "new_password": "updated password"}),
            )
            .unwrap();
            let result = super::auth::change_password(
                axum::extract::State(app.clone()),
                super::Authed(current),
                axum::Json(body),
            )
            .await;
            let after = crate::db::find_user_by_id(&app.db, id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(after.role, before.role, "{mutation} restored the role");
            assert_eq!(
                after.can_download, before.can_download,
                "{mutation} restored download permission"
            );
            assert_eq!(
                after.can_share, before.can_share,
                "{mutation} restored share permission"
            );
            assert_eq!(
                after.disabled, before.disabled,
                "{mutation} restored the account"
            );
            if matches!(mutation, "permissions" | "role") {
                let response = result.unwrap();
                let data = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let user: Value = serde_json::from_slice(&data).unwrap();
                assert_eq!(user["role"], "viewer");
                assert_eq!(user["can_download"], false);
                assert_ne!(after.password_hash, original_hash);
                assert!(crate::password::verify(
                    "updated password",
                    &after.password_hash
                ));
            } else {
                assert_eq!(
                    result.unwrap_err().status_code(),
                    StatusCode::UNAUTHORIZED,
                    "{mutation}"
                );
                assert_eq!(
                    after.password_hash, before.password_hash,
                    "{mutation} overwrote the password"
                );
                if matches!(mutation, "session" | "demoted") {
                    assert!(
                        crate::db::load_session(&app.db, &session.id)
                            .await
                            .unwrap()
                            .is_none()
                    );
                }
            }
        }
    }

    // ---- M1 / M2: per-address login budget and bounded unauthenticated audit ----

    fn login_from(username: &str, password: &str, forwarded: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header("content-type", "application/json");
        if let Some(forwarded) = forwarded {
            builder = builder.header("x-forwarded-for", forwarded);
        }
        builder
            .body(Body::from(
                json!({ "username": username, "password": password }).to_string(),
            ))
            .unwrap()
    }

    async fn audit_count(app: &App) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM audit")
            .fetch_one(&app.db)
            .await
            .unwrap()
    }

    async fn proxied_app(dir: &std::path::Path) -> App {
        state::test_app_with(
            dir,
            &[
                ("GARAGE_ADMIN_URL", "http://127.0.0.1:9"),
                ("GARAGE_ADMIN_TOKEN", TOKEN),
                ("TRUSTED_PROXY", "true"),
            ],
            test_quota(),
        )
        .await
    }

    #[tokio::test]
    async fn login_attempts_from_one_address_are_capped_across_usernames() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        // Default LOGIN_IP_LIMIT_PER_MINUTE is 30: thirty different usernames are answered.
        for index in 0..30 {
            let failed = exchange(&app, login_from(&format!("user{index}"), "wrong horse", None)).await;
            assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "attempt {index}: {}", failed.body);
        }
        // The 31st attempt is refused whatever the username, before any bcrypt work.
        let blocked = exchange(&app, login_from("user30", "wrong horse", None)).await;
        assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS, "{}", blocked.body);
        assert_eq!(code_of(&blocked.body), "rate_limited");
        let real = exchange(&app, login_from("Ada", PASSWORD, None)).await;
        assert_eq!(real.status, StatusCode::TOO_MANY_REQUESTS, "{}", real.body);
    }

    #[tokio::test]
    async fn login_address_budget_follows_the_trusted_proxy_setting() {
        // Behind a trusted proxy each forwarded client has its own budget.
        let dir = crate::util::temp_dir();
        let app = proxied_app(&dir).await;
        login(&app).await;
        for index in 0..30 {
            let failed = exchange(
                &app,
                login_from(&format!("user{index}"), "wrong horse", Some("198.51.100.1, 203.0.113.9")),
            )
            .await;
            assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "attempt {index}: {}", failed.body);
        }
        let blocked = exchange(
            &app,
            login_from("user30", "wrong horse", Some("198.51.100.2, 203.0.113.9")),
        )
        .await;
        assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS, "{}", blocked.body);
        let other = exchange(&app, login_from("Ada", PASSWORD, Some("203.0.113.10"))).await;
        assert_eq!(other.status, StatusCode::OK, "{}", other.body);

        // Without TRUSTED_PROXY a client cannot mint new budgets by rotating the header.
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        for index in 0..30 {
            let forwarded = format!("203.0.113.{index}");
            let failed = exchange(
                &app,
                login_from(&format!("user{index}"), "wrong horse", Some(&forwarded)),
            )
            .await;
            assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "attempt {index}: {}", failed.body);
        }
        let blocked = exchange(&app, login_from("user30", "wrong horse", Some("203.0.113.200"))).await;
        assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS, "{}", blocked.body);
    }

    #[tokio::test]
    async fn illegal_usernames_spend_the_address_budget() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        for index in 0..30 {
            let name = format!("{}{index}", "x".repeat(65));
            let failed = exchange(&app, login_from(&name, "wrong horse", None)).await;
            assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "attempt {index}: {}", failed.body);
            assert_eq!(code_of(&failed.body), "invalid_credentials");
        }
        let blocked = exchange(&app, login_from("Ada", PASSWORD, None)).await;
        assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS, "{}", blocked.body);
        assert_eq!(code_of(&blocked.body), "rate_limited");
    }

    #[tokio::test]
    async fn unauthenticated_login_noise_writes_a_bounded_number_of_audit_rows() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        let before = audit_count(&app).await;
        let mut statuses = std::collections::BTreeMap::new();
        for index in 0..200 {
            let name = if index % 2 == 0 { format!("bad name {index}") } else { "x".repeat(65) };
            let response = exchange(&app, login_from(&name, "wrong horse", None)).await;
            *statuses.entry(response.status.as_u16()).or_insert(0) += 1;
        }
        let written = audit_count(&app).await - before;
        assert!(written <= 20, "200 unauthenticated requests wrote {written} audit rows ({statuses:?})");
        assert!(written >= 1, "the first attempts are still audited");
    }

    #[tokio::test]
    async fn repeated_429s_write_a_bounded_number_of_audit_rows() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        login(&app).await;
        // Without this, slow bcrypt in the first ten attempts lets the quota refill.
        app.limits.clock().freeze();
        for _ in 0..10 {
            let failed = exchange(&app, login_from("Ada", "wrong horse", None)).await;
            assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
        }
        let before = audit_count(&app).await;
        for _ in 0..100 {
            let blocked = exchange(&app, login_from("Ada", "wrong horse", None)).await;
            assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS, "{}", blocked.body);
        }
        let written = audit_count(&app).await - before;
        assert!(written <= 10, "100 rate-limited requests wrote {written} audit rows");
    }

    #[tokio::test]
    async fn login_noise_does_not_hide_audit_rows_for_real_attempts() {
        let dir = crate::util::temp_dir();
        let app = proxied_app(&dir).await;
        login(&app).await;
        let attacker = Some("203.0.113.66");
        for _ in 0..15 {
            let failed = exchange(&app, login_from(&"x".repeat(65), "wrong horse", attacker)).await;
            assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
        }
        // A well-formed username from the noisy address is still recorded in full.
        let failed = exchange(&app, login_from("Ada", "wrong horse", attacker)).await;
        assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(rows[0].username, "Ada");
        assert_eq!(rows[0].ip, "203.0.113.66");
        assert_eq!(rows[0].reason, "invalid_credentials");
        // Another client's failure and success are recorded as usual.
        let other = Some("198.51.100.7");
        let failed = exchange(&app, login_from("Ada", "wrong horse", other)).await;
        assert_eq!(failed.status, StatusCode::UNAUTHORIZED, "{}", failed.body);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!((rows[0].username.as_str(), rows[0].ip.as_str(), rows[0].reason.as_str()), ("Ada", "198.51.100.7", "invalid_credentials"));
        let ok = exchange(&app, login_from("Ada", PASSWORD, other)).await;
        assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!((rows[0].username.as_str(), rows[0].ip.as_str(), rows[0].result.as_str()), ("Ada", "198.51.100.7", "success"));
    }

    #[tokio::test]
    async fn suppressed_login_noise_is_summarised_with_its_count() {
        let dir = crate::util::temp_dir();
        let app = proxied_app(&dir).await;
        login(&app).await;
        for _ in 0..200 {
            let _ = exchange(&app, login_from(&"x".repeat(65), "wrong horse", Some("203.0.113.66"))).await;
        }
        for _ in 0..12 {
            let _ = exchange(&app, login_from(&"x".repeat(65), "wrong horse", Some("2001:db8:7:7::1"))).await;
        }
        let before = audit_count(&app).await;
        assert_eq!(crate::flush_suppressed_audit(&app.db, &app.limits).await, Ok(2));
        assert_eq!(audit_count(&app).await, before + 2);
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        let summary = |ip: &str| {
            rows.iter()
                .find(|row| row.reason == "audit_suppressed" && row.ip == ip)
                .unwrap_or_else(|| panic!("summary for {ip}: {rows:?}"))
        };
        let v4 = summary("203.0.113.66");
        assert_eq!(v4.target, "suppressed=190");
        assert_eq!((v4.result.as_str(), v4.method.as_str(), v4.path.as_str(), v4.status), ("rate_limited", "POST", "/api/auth/login", 429));
        assert_eq!(v4.username, "");
        assert_eq!(summary("2001:db8:7:7::/64").target, "suppressed=2");
        // Nothing is pending afterwards, so the next flush writes nothing.
        assert_eq!(crate::flush_suppressed_audit(&app.db, &app.limits).await, Ok(0));
    }

    fn setup_from(username: &str, forwarded: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/setup")
            .header("content-type", "application/json")
            .header("x-forwarded-for", forwarded)
            .body(Body::from(
                json!({ "username": username, "password": PASSWORD }).to_string(),
            ))
            .unwrap()
    }

    #[tokio::test]
    async fn setup_after_init_from_one_ipv6_64_writes_a_bounded_number_of_audit_rows() {
        let dir = crate::util::temp_dir();
        let app = proxied_app(&dir).await;
        login(&app).await;
        app.limits.clock().freeze();
        let before = audit_count(&app).await;
        let mut statuses = std::collections::BTreeMap::new();
        for index in 0..300u32 {
            // Every request comes from a new address inside 2001:db8:9:9::/64.
            let forwarded = format!("2001:db8:9:9:{:x}:{:x}::1", index >> 16, index & 0xffff);
            let response = exchange(&app, setup_from(&"x".repeat(128), &forwarded)).await;
            *statuses.entry(response.status.as_u16()).or_insert(0u32) += 1;
        }
        // The /64 shares one setup budget of ten; the rest is refused before the database.
        assert_eq!(statuses, std::collections::BTreeMap::from([(409, 10), (429, 290)]));
        let written = audit_count(&app).await - before;
        assert_eq!(written, 10, "300 setup requests from one /64 wrote {written} audit rows");
        assert_eq!(crate::flush_suppressed_audit(&app.db, &app.limits).await, Ok(1));
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        let summary = &rows[0];
        assert_eq!(
            (
                summary.ip.as_str(),
                summary.path.as_str(),
                summary.reason.as_str(),
                summary.target.as_str()
            ),
            ("2001:db8:9:9::/64", "/api/setup", "audit_suppressed", "suppressed=290")
        );
    }

    #[tokio::test]
    async fn setup_after_init_from_one_ipv4_address_writes_a_bounded_number_of_audit_rows() {
        let dir = crate::util::temp_dir();
        let app = proxied_app(&dir).await;
        login(&app).await;
        app.limits.clock().freeze();
        let before = audit_count(&app).await;
        let mut statuses = std::collections::BTreeMap::new();
        for _ in 0..300 {
            let response = exchange(&app, setup_from("Mallory", "203.0.113.5")).await;
            *statuses.entry(response.status.as_u16()).or_insert(0u32) += 1;
        }
        assert_eq!(statuses, std::collections::BTreeMap::from([(409, 10), (429, 290)]));
        // 409 and 429 rows share the address's ten noisy rows per minute.
        let written = audit_count(&app).await - before;
        assert_eq!(written, 10, "300 setup requests from one address wrote {written} audit rows");
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert!(
            rows.iter()
                .take(10)
                .all(|row| row.reason == "already_initialized" && row.ip == "203.0.113.5"),
            "{rows:?}"
        );
    }

    #[tokio::test]
    async fn a_failed_summary_write_keeps_the_unwritten_counts_for_the_next_flush() {
        let dir = crate::util::temp_dir();
        let app = proxied_app(&dir).await;
        login(&app).await;
        app.limits.clock().freeze();
        for _ in 0..25 {
            let _ = exchange(
                &app,
                login_from(&"x".repeat(65), "wrong horse", Some("203.0.113.66")),
            )
            .await;
        }
        for _ in 0..13 {
            let _ = exchange(
                &app,
                login_from(&"x".repeat(65), "wrong horse", Some("203.0.113.77")),
            )
            .await;
        }
        // Summaries are written in address order; make the second one fail.
        sqlx::query(
            "CREATE TRIGGER fail_summary BEFORE INSERT ON audit WHEN NEW.ip = '203.0.113.77' \
             BEGIN SELECT RAISE(ABORT, 'injected failure'); END",
        )
        .execute(&app.db)
        .await
        .unwrap();
        let summaries = |rows: &[crate::db::AuditRow]| {
            rows.iter()
                .filter(|row| row.reason == "audit_suppressed")
                .map(|row| (row.ip.clone(), row.target.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(crate::flush_suppressed_audit(&app.db, &app.limits).await, Err(1));
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(
            summaries(&rows),
            vec![("203.0.113.66".to_string(), "suppressed=15".to_string())]
        );

        sqlx::query("DROP TRIGGER fail_summary").execute(&app.db).await.unwrap();
        // Only the failed counter is retried, with its count unchanged.
        assert_eq!(crate::flush_suppressed_audit(&app.db, &app.limits).await, Ok(1));
        let rows = crate::db::recent_audit(&app.db).await.unwrap();
        assert_eq!(
            summaries(&rows),
            vec![
                ("203.0.113.77".to_string(), "suppressed=3".to_string()),
                ("203.0.113.66".to_string(), "suppressed=15".to_string()),
            ]
        );
        assert_eq!(crate::flush_suppressed_audit(&app.db, &app.limits).await, Ok(0));
    }
