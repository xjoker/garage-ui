    // Low-risk hardening: /metrics authorization (L2) and session storage (L3).

    fn sha256_hex(text: &str) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    async fn stored_session_ids(app: &App) -> Vec<String> {
        sqlx::query_scalar("SELECT id FROM sessions ORDER BY id")
            .fetch_all(&app.db)
            .await
            .unwrap()
    }

    async fn tenant_session(app: &App) -> Session {
        let hash = crate::password::hash_async("tenant horse".to_string(), 4)
            .await
            .unwrap();
        crate::db::insert_user(
            &app.db,
            "Tess",
            &hash,
            "viewer",
            true,
            false,
            Some("GKtenant"),
        )
        .await
        .unwrap();
        sign_in(app, "Tess", "tenant horse").await
    }

    #[tokio::test]
    async fn metrics_page_requires_the_metrics_token_or_view_metrics() {
        let dir = crate::util::temp_dir();
        let token = "metrics-token-ok16";
        let app =
            state::test_app_with(&dir, &[("GARAGE_METRICS_TOKEN", token)], test_quota()).await;
        let (admin, _) = login(&app).await;
        let viewer = viewer_session(&app, &admin).await;
        let tenant = tenant_session(&app).await;

        for (label, session) in [("admin", &admin), ("viewer", &viewer)] {
            let response = exchange(&app, request("GET", "/metrics", None, Some(session))).await;
            assert_eq!(response.status, StatusCode::OK, "{label}: {}", response.body);
            assert!(
                response.body.contains("garage_ui_http_requests_total"),
                "{label}: {}",
                response.body
            );
        }

        let refused = exchange(&app, request("GET", "/metrics", None, Some(&tenant))).await;
        assert_error(&refused, StatusCode::FORBIDDEN, "forbidden", "Not allowed");
        assert!(!refused.body.contains("garage_ui_http_requests_total"));
        let api = exchange(&app, request("GET", "/api/metrics", None, Some(&tenant))).await;
        assert_eq!(api.status, StatusCode::FORBIDDEN, "{}", api.body);

        let mut scrape = request("GET", "/metrics", None, Some(&tenant));
        scrape.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        let scraped = exchange(&app, scrape).await;
        assert_eq!(scraped.status, StatusCode::OK, "{}", scraped.body);

        let anonymous = exchange(&app, request("GET", "/metrics", None, None)).await;
        assert_error(
            &anonymous,
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "Login required",
        );

        // With a token configured, a wrong one grants nothing: the request is judged by its
        // session alone, so an anonymous caller gets 401 and a tenant still gets 403.
        let with_bearer = |session: Option<&Session>, bearer: &str| {
            let mut scrape = request("GET", "/metrics", None, session);
            scrape.headers_mut().insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {bearer}")).unwrap(),
            );
            scrape
        };
        let short = &token[..token.len() - 1];
        for bearer in ["wrong-metrics-token", short] {
            let wrong = exchange(&app, with_bearer(None, bearer)).await;
            assert_error(
                &wrong,
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
                "Login required",
            );
            assert!(!wrong.body.contains("garage_ui_http_requests_total"));
            let tenant_wrong = exchange(&app, with_bearer(Some(&tenant), bearer)).await;
            assert_error(&tenant_wrong, StatusCode::FORBIDDEN, "forbidden", "Not allowed");
            assert!(!tenant_wrong.body.contains("garage_ui_http_requests_total"));
        }
    }

    #[tokio::test]
    async fn sessions_table_stores_only_token_digests() {
        let dir = crate::util::temp_dir();
        let app = state::test_app(&dir).await;
        let (admin, _) = login(&app).await;
        let viewer = viewer_session(&app, &admin).await;

        let stored = stored_session_ids(&app).await;
        assert_eq!(stored.len(), 2, "{stored:?}");
        for session in [&admin, &viewer] {
            assert!(!stored.contains(&session.id), "cookie value stored in plain text");
            assert!(stored.contains(&sha256_hex(&session.id)), "{stored:?}");
        }
        let in_any_column: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE id = ?1 OR csrf_token = ?1 OR key_id = ?1",
        )
        .bind(&admin.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(in_any_column, 0);

        let status = exchange(&app, request("GET", "/api/auth/status", None, Some(&admin))).await;
        assert_eq!(
            serde_json::from_str::<Value>(&status.body).unwrap()["authenticated"],
            true,
            "{}",
            status.body
        );
        // The digest itself is not a usable cookie value.
        let forged = Session {
            id: sha256_hex(&admin.id),
            csrf: admin.csrf.clone(),
        };
        let forged = exchange(&app, request("GET", "/api/auth/status", None, Some(&forged))).await;
        assert_eq!(
            serde_json::from_str::<Value>(&forged.body).unwrap()["authenticated"],
            false,
            "{}",
            forged.body
        );

        let logout = exchange(&app, request("POST", "/api/auth/logout", None, Some(&admin))).await;
        assert!(logout.status.is_success(), "{}", logout.body);
        assert_eq!(stored_session_ids(&app).await, vec![sha256_hex(&viewer.id)]);
    }

    #[tokio::test]
    async fn stale_sessions_are_deleted_without_being_presented() {
        let dir = crate::util::temp_dir();
        let app = state::test_app(&dir).await;
        let (admin, _) = login(&app).await;
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'Ada'")
            .fetch_one(&app.db)
            .await
            .unwrap();
        let now = crate::util::now_secs();
        let idle = i64::try_from(app.settings.session_idle.as_secs()).unwrap();
        let key = app.key_id.as_str();
        for (token, key_id, seen, expires) in [
            ("absolute-expired", key, now - 10, now - 1),
            ("idle-expired", key, now - idle - 10, now + 3600),
            ("other-session-key", "retired-key-id", now, now + 3600),
            ("fresh", key, now, now + 3600),
        ] {
            crate::db::insert_session(&app.db, token, user_id, "csrf", key_id, seen, expires)
                .await
                .unwrap();
        }
        assert_eq!(stored_session_ids(&app).await.len(), 5);

        crate::start_session_cleanup(app.clone());
        let mut expected = vec![sha256_hex(&admin.id), sha256_hex("fresh")];
        expected.sort();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let stored = stored_session_ids(&app).await;
            if stored == expected {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "stale sessions were not deleted: {stored:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let status = exchange(&app, request("GET", "/api/auth/status", None, Some(&admin))).await;
        assert_eq!(
            serde_json::from_str::<Value>(&status.body).unwrap()["authenticated"],
            true
        );
    }

    #[tokio::test]
    async fn sessions_from_before_digests_are_signed_out_once() {
        let dir = crate::util::temp_dir();
        let path = dir.join("garage-ui.sqlite");
        let pool = crate::db::open(&path).await.unwrap();
        let user_id = crate::db::insert_user(&pool, "Ada", "stored-hash", "admin", false, false, None)
            .await
            .unwrap();
        // An older version stored the cookie value itself and left user_version at 0. Its
        // cookies are 32 random bytes in hex, the same length as a digest, so length cannot
        // tell the old rows apart.
        let legacy_cookie = crate::util::hex_encode(&crate::util::random_bytes(32).unwrap());
        assert_eq!(legacy_cookie.len(), sha256_hex("x").len());
        sqlx::query(
            "INSERT INTO sessions (id, user_id, csrf_token, key_id, created_at, last_seen_at, expires_at)
             VALUES (?, ?, 'csrf', 'key', 1, 1, 9999999999)",
        )
        .bind(&legacy_cookie)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("PRAGMA user_version = 0")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let pool = crate::db::open(&path).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0, "plain-text sessions survived the upgrade");
        let legacy_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id = ?")
            .bind(&legacy_cookie)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(legacy_left, 0, "the legacy cookie row survived the upgrade");
        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(users, 1);
        let now = crate::util::now_secs();
        crate::db::insert_session(&pool, "kept", user_id, "csrf", "key", now, now + 60)
            .await
            .unwrap();
        pool.close().await;

        let pool = crate::db::open(&path).await.unwrap();
        let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(kept, 1, "a later restart must not sign users out again");
        pool.close().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn database_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        fn mode(path: &std::path::Path) -> u32 {
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777
        }
        fn with_suffix(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
            let mut name = path.as_os_str().to_owned();
            name.push(suffix);
            std::path::PathBuf::from(name)
        }
        let dir = crate::util::temp_dir();

        // A new database, written once so that the WAL and shared-memory files exist.
        let fresh = dir.join("fresh.sqlite");
        let pool = crate::db::open(&fresh).await.unwrap();
        crate::db::insert_user(&pool, "Ada", "stored-hash", "admin", false, false, None)
            .await
            .unwrap();
        for suffix in ["", "-wal", "-shm"] {
            let file = with_suffix(&fresh, suffix);
            assert_eq!(mode(&file), 0o600, "{}", file.display());
        }
        pool.close().await;

        // Files an older version left world-readable after it was stopped without a final
        // checkpoint: the WAL still holds rows that are not in the database file. The files are
        // copied while the writer is open, which is what such a stop leaves on disk. The copy is
        // not torn: the insert above has committed, nothing else writes to `running`, and the WAL
        // is far below the 1000-page automatic checkpoint, so all three files are at rest. The
        // copied `-shm` is only an index of the WAL; SQLite rebuilds it when its header is invalid.
        let running = dir.join("running.sqlite");
        let writer = crate::db::open(&running).await.unwrap();
        crate::db::insert_user(&writer, "Ada", "stored-hash", "admin", false, false, None)
            .await
            .unwrap();
        let legacy = dir.join("legacy.sqlite");
        for suffix in ["", "-wal", "-shm"] {
            let file = with_suffix(&legacy, suffix);
            std::fs::copy(with_suffix(&running, suffix), &file).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        writer.close().await;
        let wal = with_suffix(&legacy, "-wal");
        assert!(std::fs::metadata(&wal).unwrap().len() > 0, "the legacy WAL is empty");
        let pool = crate::db::open(&legacy).await.unwrap();
        for suffix in ["", "-wal", "-shm"] {
            let file = with_suffix(&legacy, suffix);
            assert_eq!(mode(&file), 0o600, "{}", file.display());
        }
        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(users, 1, "the rows in the legacy WAL were not recovered");
        pool.close().await;
    }

    async fn sessions_of(pool: &sqlx::SqlitePool, user_id: i64) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn deleting_a_user_removes_its_sessions_even_without_foreign_keys() {
        let dir = crate::util::temp_dir();
        let path = dir.join("garage-ui.sqlite");
        crate::db::open(&path).await.unwrap().close().await;
        // The same database with foreign keys off, so ON DELETE CASCADE does nothing.
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .foreign_keys(false);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        crate::db::insert_user(&pool, "Ada", "stored-hash", "admin", false, false, None)
            .await
            .unwrap();
        let bea = crate::db::insert_user(&pool, "Bea", "stored-hash", "viewer", true, false, None)
            .await
            .unwrap();
        let now = crate::util::now_secs();
        crate::db::insert_session(&pool, "bea-session", bea, "csrf", "key", now, now + 3600)
            .await
            .unwrap();
        crate::db::insert_zip(&pool, "bea-zip", bea, "bucket", "[]")
            .await
            .unwrap();

        crate::db::delete_user(&pool, bea).await.unwrap();
        assert_eq!(sessions_of(&pool, bea).await, 0, "a deleted user kept its session");
        let zips: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM zip_tokens WHERE user_id = ?")
            .bind(bea)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(zips, 0, "a deleted user kept its archive token");

        // `users.id` is reused: the next user gets the deleted id and must start with nothing.
        let cat = crate::db::insert_user(&pool, "Cat", "stored-hash", "admin", false, false, None)
            .await
            .unwrap();
        assert_eq!(cat, bea, "this test relies on SQLite reusing the highest rowid");
        assert_eq!(sessions_of(&pool, cat).await, 0);
        pool.close().await;
    }

    #[tokio::test]
    async fn a_deleted_users_cookie_does_not_sign_in_the_next_user_with_its_id() {
        let dir = crate::util::temp_dir();
        let app = state::test_app(&dir).await;
        let (admin, _) = login(&app).await;
        let viewer = viewer_session(&app, &admin).await;
        let bea: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'Bea'")
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(sessions_of(&app.db, bea).await, 1);

        let deleted = exchange(
            &app,
            request("DELETE", &format!("/api/users/{bea}"), None, Some(&admin)),
        )
        .await;
        assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);
        assert_eq!(sessions_of(&app.db, bea).await, 0);

        let created = exchange(
            &app,
            request(
                "POST",
                "/api/users",
                Some(
                    &json!({ "username": "Cat", "password": "admin horse", "role": "admin" })
                        .to_string(),
                ),
                Some(&admin),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        let cat: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'Cat'")
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(cat, bea, "this test relies on SQLite reusing the highest rowid");
        let status = exchange(&app, request("GET", "/api/auth/status", None, Some(&viewer))).await;
        let status: Value = serde_json::from_str(&status.body).unwrap();
        assert_eq!(status["authenticated"], false, "{status}");
    }
