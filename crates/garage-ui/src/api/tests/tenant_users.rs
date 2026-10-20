    // The Garage key a tenant is bound to through the user API (docs/security.md 3.20, "Binding").
    const OTHER_KEY: &str = "GKother";
    // Keys that grant more than a viewer may hold: `write` on one bucket, or `owner` alone.
    const WRITE_KEY: &str = "GKwriter";
    const OWNER_KEY: &str = "GKowner";
    // GetKeyInfo replies that say nothing about the key: a server error, a 200 that is not
    // JSON, incomplete permissions, JSON of the wrong shape, a body cut short, or too large.
    const FAILING_KEYS: [&str; 7] = [
        "GKmissingBuckets",
        "GKmissingPermissions",
        "GKerror",
        "GKgarbled",
        "GKshapeless",
        "GKtruncated",
        "GKoversized",
    ];

    fn raw_http(status: &str, body: &str, length: usize) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}"
        )
        .into_bytes()
    }

    // GetKeyInfo for a key that holds `read` on b1 and b2, plus the given grants on b1.
    fn bound_key_info(id: &str, write: bool, owner: bool) -> Value {
        let grant = |bucket: &str, read: bool, write: bool, owner: bool| {
            json!({
                "id": bucket,
                "globalAliases": [format!("name-{bucket}")],
                "localAliases": [],
                "permissions": { "read": read, "write": write, "owner": owner },
            })
        };
        json!({
            "accessKeyId": id,
            "name": id,
            "expired": false,
            "permissions": { "createBucket": false },
            "secretAccessKey": TENANT_SECRET,
            "buckets": [
                grant("b2", true, false, false),
                grant("b1", !owner, write, owner),
                grant("b4", false, false, false),
            ],
        })
    }

    // GetKeyInfo knows four keys: two read-only, one with `write`, one with `owner`. Every other
    // key is a 404, like a deleted key.
    fn bind_reply() -> impl Fn(&Seen) -> Vec<u8> + Send + Sync + 'static {
        let known = tenant_admin_reply(false);
        move |call| {
            let target = request_target(&call.line);
            let path = target.split('?').next().unwrap_or("");
            let key = query_value(target, "id").unwrap_or("");
            if path == "/v2/GetKeyInfo" {
                let info = match key {
                    TENANT_KEY | OTHER_KEY => bound_key_info(key, false, false),
                    WRITE_KEY => bound_key_info(key, true, false),
                    OWNER_KEY => bound_key_info(key, false, true),
                    "GKmissingBuckets" => json!({}),
                    "GKmissingPermissions" => json!({ "buckets": [{}] }),
                    "GKempty" => json!({ "buckets": [] }),
                    "GKmissingWrite" => json!({ "buckets": [{ "permissions": { "owner": false } }] }),
                    "GKmissingOwner" => json!({ "buckets": [{ "permissions": { "write": false } }] }),
                    "GKerror" => {
                        return json_http(
                            "500 Internal Server Error",
                            &json!({ "code": "InternalError", "message": "boom" }),
                        );
                    }
                    "GKgarbled" => return raw_http("200 OK", "<html>proxy</html>", 18),
                    "GKshapeless" => {
                        return json_http("200 OK", &json!({ "buckets": "everything" }));
                    }
                    "GKtruncated" => return raw_http("200 OK", "{\"buckets\": [", 4096),
                    "GKoversized" => {
                        // Valid JSON granting nothing, padded past the limit.
                        let body = format!("{{\"buckets\": []{}}}", " ".repeat(1024 * 1024));
                        return raw_http("200 OK", &body, body.len());
                    }
                    _ => {
                        return json_http(
                            "404 Not Found",
                            &json!({ "code": "NoSuchAccessKey", "message": "gone" }),
                        );
                    }
                };
                return json_http("200 OK", &info);
            }
            known(call)
        }
    }

    struct BindEnv {
        app: App,
        admin: Session,
        garage: TenantServer,
        _s3: TenantServer,
    }

    async fn bind_env() -> BindEnv {
        let garage = TenantServer::start(bind_reply()).await;
        let s3 = TenantServer::start(tenant_s3_reply(false)).await;
        let dir = crate::util::temp_dir();
        let app = state::test_app_with(
            &dir,
            &[
                ("GARAGE_ADMIN_URL", garage.url.as_str()),
                ("GARAGE_ADMIN_TOKEN", TOKEN),
                ("S3_ENDPOINT_URL", s3.url.as_str()),
                ("S3_REGION", "garage"),
                ("S3_ACCESS_KEY_ID", "PROJECT"),
                ("S3_SECRET_ACCESS_KEY", "project-secret-value"),
                ("PUBLIC_S3_URL", "http://127.0.0.1:3901"),
            ],
            test_quota(),
        )
        .await;
        let (admin, _) = login(&app).await;
        BindEnv {
            app,
            admin,
            garage,
            _s3: s3,
        }
    }

    async fn create_account(env: &BindEnv, name: &str, extra: Value) -> Exchange {
        let mut body = json!({
            "username": name,
            "password": "tenant horse",
            "role": "viewer",
            "can_download": true,
        });
        for (field, value) in extra.as_object().unwrap() {
            body[field] = value.clone();
        }
        exchange(
            &env.app,
            request("POST", "/api/users", Some(&body.to_string()), Some(&env.admin)),
        )
        .await
    }

    async fn patch_account(env: &BindEnv, id: i64, body: Value) -> Exchange {
        exchange(
            &env.app,
            request(
                "PATCH",
                &format!("/api/users/{id}"),
                Some(&body.to_string()),
                Some(&env.admin),
            ),
        )
        .await
    }

    fn id_of(response: &Exchange) -> i64 {
        serde_json::from_str::<Value>(&response.body).unwrap()["id"]
            .as_i64()
            .unwrap()
    }

    fn key_of(response: &Exchange) -> Value {
        serde_json::from_str::<Value>(&response.body).unwrap()["access_key_id"].clone()
    }

    async fn stored_key(env: &BindEnv, id: i64) -> Option<String> {
        crate::db::find_user_by_id(&env.app.db, id)
            .await
            .unwrap()
            .unwrap()
            .access_key_id
    }

    async fn session_status(env: &BindEnv, session: &Session) -> StatusCode {
        exchange(&env.app, request("GET", "/api/buckets", None, Some(session)))
            .await
            .status
    }

    fn assert_bad_request(response: &Exchange, code: &str) {
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{}", response.body);
        assert_eq!(code_of(&response.body), code, "{}", response.body);
    }

    fn assert_no_secret(response: &Exchange) {
        assert!(!response.body.contains(TENANT_SECRET), "{}", response.body);
    }

    #[tokio::test]
    async fn a_viewer_created_with_a_key_is_listed_with_it_and_acts_as_a_tenant() {
        let env = bind_env().await;
        let created = create_account(&env, "Tess", json!({ "access_key_id": TENANT_KEY })).await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(key_of(&created), json!(TENANT_KEY));
        assert_no_secret(&created);
        let id = id_of(&created);
        assert_eq!(stored_key(&env, id).await.as_deref(), Some(TENANT_KEY));

        let plain = create_account(&env, "Plain", json!({})).await;
        assert_eq!(plain.status, StatusCode::CREATED, "{}", plain.body);
        assert_eq!(key_of(&plain), Value::Null);

        let listed = exchange(&env.app, request("GET", "/api/users", None, Some(&env.admin))).await;
        assert_eq!(listed.status, StatusCode::OK, "{}", listed.body);
        assert_no_secret(&listed);
        let items = user_items(&listed.body);
        let keyed = |name: &str| {
            items
                .iter()
                .find(|user| user["username"] == name)
                .map(|user| user["access_key_id"].clone())
        };
        assert_eq!(keyed("Tess"), Some(json!(TENANT_KEY)));
        assert_eq!(keyed("Plain"), Some(Value::Null));
        assert_eq!(keyed("Ada"), Some(Value::Null));

        let tenant = sign_in(&env.app, "Tess", "tenant horse").await;
        let buckets = exchange(&env.app, request("GET", "/api/buckets", None, Some(&tenant))).await;
        assert_eq!(buckets.status, StatusCode::OK, "{}", buckets.body);
        assert_eq!(bucket_ids(&buckets.body), ["b1", "b2"]);
        assert_no_secret(&buckets);
    }

    #[tokio::test]
    async fn an_administrator_cannot_be_bound_to_a_key() {
        let env = bind_env().await;
        let created = create_account(
            &env,
            "Boss",
            json!({ "role": "admin", "access_key_id": TENANT_KEY }),
        )
        .await;
        assert_bad_request(&created, "tenant_admin");
        assert!(
            crate::db::find_user_by_name(&env.app.db, "Boss")
                .await
                .unwrap()
                .is_none()
        );

        let admin = create_account(&env, "Boss", json!({ "role": "admin" })).await;
        assert_eq!(admin.status, StatusCode::CREATED, "{}", admin.body);
        let bound = patch_account(&env, id_of(&admin), json!({ "access_key_id": TENANT_KEY })).await;
        assert_bad_request(&bound, "tenant_admin");
        assert_eq!(stored_key(&env, id_of(&admin)).await, None);
    }

    #[tokio::test]
    async fn promoting_a_bound_viewer_needs_the_key_removed_in_the_same_request() {
        let env = bind_env().await;
        let created = create_account(&env, "Tess", json!({ "access_key_id": TENANT_KEY })).await;
        let id = id_of(&created);

        let refused = patch_account(&env, id, json!({ "role": "admin" })).await;
        assert_bad_request(&refused, "tenant_admin");
        let stored = crate::db::find_user_by_id(&env.app.db, id).await.unwrap().unwrap();
        assert_eq!(stored.role, "viewer");
        assert_eq!(stored.access_key_id.as_deref(), Some(TENANT_KEY));

        let both = patch_account(&env, id, json!({ "role": "admin", "access_key_id": null })).await;
        assert_eq!(both.status, StatusCode::OK, "{}", both.body);
        assert_eq!(key_of(&both), Value::Null);
        let stored = crate::db::find_user_by_id(&env.app.db, id).await.unwrap().unwrap();
        assert_eq!(stored.role, "admin");
        assert_eq!(stored.access_key_id, None);
    }

    #[tokio::test]
    async fn a_key_garage_does_not_know_is_refused_and_not_stored() {
        let env = bind_env().await;
        let before = crate::db::recent_audit(&env.app.db).await.unwrap().len();
        let created = create_account(&env, "Tess", json!({ "access_key_id": "GKnope" })).await;
        assert_bad_request(&created, "key_not_found");
        assert!(
            crate::db::find_user_by_name(&env.app.db, "Tess")
                .await
                .unwrap()
                .is_none()
        );
        let rows = crate::db::recent_audit(&env.app.db).await.unwrap();
        assert_eq!(rows.len(), before + 1, "{rows:?}");
        assert_eq!(rows[0].reason, "key_not_found");
        assert_eq!(rows[0].status, 400);

        let plain = create_account(&env, "Plain", json!({})).await;
        let id = id_of(&plain);
        let patched = patch_account(&env, id, json!({ "access_key_id": "GKnope" })).await;
        assert_bad_request(&patched, "key_not_found");
        assert_eq!(stored_key(&env, id).await, None);
    }

    #[tokio::test]
    async fn malformed_key_ids_are_refused_without_asking_garage() {
        let env = bind_env().await;
        let too_long = "K".repeat(129);
        for bad in ["", " ", "has space", "tab\there", "ключ", too_long.as_str()] {
            let created = create_account(&env, "Tess", json!({ "access_key_id": bad })).await;
            assert_bad_request(&created, "invalid_access_key_id");
        }
        let plain = create_account(&env, "Plain", json!({})).await;
        let patched = patch_account(&env, id_of(&plain), json!({ "access_key_id": "" })).await;
        assert_bad_request(&patched, "invalid_access_key_id");
        let lines = env.garage.lines();
        assert!(
            lines.iter().all(|line| !line.contains("GetKeyInfo")),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn an_unreachable_garage_is_not_reported_as_a_missing_key() {
        let dir = crate::util::temp_dir();
        let app = app_for(&dir, "http://127.0.0.1:9").await;
        let (admin, _) = login(&app).await;
        let created = exchange(
            &app,
            request(
                "POST",
                "/api/users",
                Some(
                    &json!({
                        "username": "Tess",
                        "password": "tenant horse",
                        "role": "viewer",
                        "access_key_id": TENANT_KEY,
                    })
                    .to_string(),
                ),
                Some(&admin),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::BAD_GATEWAY, "{}", created.body);
        assert_ne!(code_of(&created.body), "key_not_found");
        assert!(
            crate::db::find_user_by_name(&app.db, "Tess")
                .await
                .unwrap()
                .is_none()
        );
    }

    // When GetKeyInfo does not answer usably, the key's permissions are unknown: binding fails
    // closed with 502 `garage_unavailable` and stores nothing, on create and on change.
    #[tokio::test]
    async fn a_key_whose_permissions_cannot_be_read_is_refused_and_not_stored() {
        let env = bind_env().await;
        let plain = create_account(&env, "Plain", json!({})).await;
        let plain_id = id_of(&plain);
        let bound = create_account(&env, "Tess", json!({ "access_key_id": TENANT_KEY })).await;
        assert_eq!(bound.status, StatusCode::CREATED, "{}", bound.body);
        let bound_id = id_of(&bound);
        for key in FAILING_KEYS {
            let name = format!("New-{key}");
            let created = create_account(&env, &name, json!({ "access_key_id": key })).await;
            assert_eq!(created.status, StatusCode::BAD_GATEWAY, "{key}: {}", created.body);
            assert_eq!(code_of(&created.body), "garage_unavailable", "{key}");
            assert!(
                crate::db::find_user_by_name(&env.app.db, &name)
                    .await
                    .unwrap()
                    .is_none(),
                "{key}: the account must not be stored"
            );
            let patched = patch_account(&env, plain_id, json!({ "access_key_id": key })).await;
            assert_eq!(patched.status, StatusCode::BAD_GATEWAY, "{key}: {}", patched.body);
            assert_eq!(stored_key(&env, plain_id).await, None, "{key}");
            let rebound = patch_account(&env, bound_id, json!({ "access_key_id": key })).await;
            assert_eq!(rebound.status, StatusCode::BAD_GATEWAY, "{key}: {}", rebound.body);
            assert_eq!(stored_key(&env, bound_id).await.as_deref(), Some(TENANT_KEY), "{key}");
        }
        let lines = env.garage.lines();
        for key in FAILING_KEYS {
            assert!(
                lines.iter().any(|line| line.contains(&format!("GetKeyInfo?id={key}"))),
                "{key}: {lines:?}"
            );
        }
    }

    #[tokio::test]
    async fn share_still_requires_download_for_a_tenant() {
        let env = bind_env().await;
        let created = create_account(
            &env,
            "Tess",
            json!({ "access_key_id": TENANT_KEY, "can_download": false, "can_share": true }),
        )
        .await;
        assert_bad_request(&created, "share_requires_download");
    }

    #[tokio::test]
    async fn changing_or_removing_the_key_ends_every_session_and_is_audited() {
        let env = bind_env().await;
        let created = create_account(&env, "Tess", json!({ "access_key_id": TENANT_KEY })).await;
        let id = id_of(&created);
        let first = sign_in(&env.app, "Tess", "tenant horse").await;
        let second = sign_in(&env.app, "Tess", "tenant horse").await;
        assert_eq!(session_status(&env, &first).await, StatusCode::OK);
        assert_eq!(session_status(&env, &second).await, StatusCode::OK);

        // The same key again is not a change.
        let before = crate::db::recent_audit(&env.app.db).await.unwrap().len();
        let same = patch_account(&env, id, json!({ "access_key_id": TENANT_KEY })).await;
        assert_eq!(same.status, StatusCode::OK, "{}", same.body);
        assert_eq!(session_status(&env, &first).await, StatusCode::OK);
        assert_eq!(session_status(&env, &second).await, StatusCode::OK);
        let untouched = patch_account(&env, id, json!({ "can_share": true })).await;
        assert_eq!(untouched.status, StatusCode::OK, "{}", untouched.body);
        assert_eq!(key_of(&untouched), json!(TENANT_KEY));
        assert_eq!(session_status(&env, &first).await, StatusCode::OK);

        let rows_before = crate::db::recent_audit(&env.app.db).await.unwrap();
        assert_eq!(rows_before.len(), before + 2, "{rows_before:?}");
        let changed = patch_account(&env, id, json!({ "access_key_id": OTHER_KEY })).await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
        assert_eq!(key_of(&changed), json!(OTHER_KEY));
        assert_no_secret(&changed);
        assert_eq!(session_status(&env, &first).await, StatusCode::UNAUTHORIZED);
        assert_eq!(session_status(&env, &second).await, StatusCode::UNAUTHORIZED);
        let rows = crate::db::recent_audit(&env.app.db).await.unwrap();
        assert_eq!(rows.len(), rows_before.len() + 1, "{rows:?}");
        assert_eq!(rows[0].result, "success");
        assert_eq!(rows[0].method, "PATCH");
        assert_eq!(rows[0].target, "Tess");
        assert_eq!(rows[0].status, 200);

        let third = sign_in(&env.app, "Tess", "tenant horse").await;
        assert_eq!(session_status(&env, &third).await, StatusCode::OK);
        let removed = patch_account(&env, id, json!({ "access_key_id": null })).await;
        assert_eq!(removed.status, StatusCode::OK, "{}", removed.body);
        assert_eq!(key_of(&removed), Value::Null);
        assert_eq!(stored_key(&env, id).await, None);
        assert_eq!(session_status(&env, &third).await, StatusCode::UNAUTHORIZED);
        let rows_after = crate::db::recent_audit(&env.app.db).await.unwrap();
        // The sign-in of the third session is audited too.
        assert_eq!(rows_after.len(), rows.len() + 2, "{rows_after:?}");
        assert_eq!(rows_after[0].reason, "user_updated");
        assert_eq!(rows_after[0].target, "Tess");

        let fourth = sign_in(&env.app, "Tess", "tenant horse").await;
        let bound = patch_account(&env, id, json!({ "access_key_id": TENANT_KEY })).await;
        assert_eq!(bound.status, StatusCode::OK, "{}", bound.body);
        assert_eq!(session_status(&env, &fourth).await, StatusCode::UNAUTHORIZED);
    }

    fn assert_exceeds_role(response: &Exchange) {
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{}", response.body);
        assert_eq!(code_of(&response.body), "key_exceeds_role", "{}", response.body);
        let message = serde_json::from_str::<Value>(&response.body).unwrap()["message"]
            .as_str()
            .unwrap_or("")
            .to_string();
        assert!(message.contains("read-only key"), "{message}");
    }

    #[tokio::test]
    async fn a_viewer_cannot_be_created_with_a_key_that_can_write_or_own() {
        let env = bind_env().await;
        for (name, key) in [("Wes", WRITE_KEY), ("Owen", OWNER_KEY)] {
            let before = crate::db::recent_audit(&env.app.db).await.unwrap().len();
            let created = create_account(&env, name, json!({ "access_key_id": key })).await;
            assert_exceeds_role(&created);
            assert!(
                crate::db::find_user_by_name(&env.app.db, name)
                    .await
                    .unwrap()
                    .is_none(),
                "{name} must not be stored"
            );
            let rows = crate::db::recent_audit(&env.app.db).await.unwrap();
            assert_eq!(rows.len(), before + 1, "{rows:?}");
            assert_eq!(rows[0].result, "error");
            assert_eq!(rows[0].reason, "key_exceeds_role");
            assert_eq!(rows[0].method, "POST");
            assert_eq!(rows[0].target, name);
            assert_eq!(rows[0].status, 403);
        }
        // The permissions came from Garage, not from the request.
        let lines = env.garage.lines();
        assert!(lines.iter().any(|line| line.contains("GetKeyInfo?id=GKwriter")), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("GetKeyInfo?id=GKowner")), "{lines:?}");
    }

    #[tokio::test]
    async fn a_viewer_cannot_be_rebound_to_a_key_that_can_write_or_own() {
        let env = bind_env().await;
        let plain = create_account(&env, "Plain", json!({})).await;
        let plain_id = id_of(&plain);
        let bound = create_account(&env, "Tess", json!({ "access_key_id": TENANT_KEY })).await;
        assert_eq!(bound.status, StatusCode::CREATED, "{}", bound.body);
        let bound_id = id_of(&bound);
        let session = sign_in(&env.app, "Tess", "tenant horse").await;
        for key in [WRITE_KEY, OWNER_KEY] {
            let patched = patch_account(&env, plain_id, json!({ "access_key_id": key })).await;
            assert_exceeds_role(&patched);
            assert_eq!(stored_key(&env, plain_id).await, None);
            let rebound = patch_account(&env, bound_id, json!({ "access_key_id": key })).await;
            assert_exceeds_role(&rebound);
            assert_eq!(stored_key(&env, bound_id).await.as_deref(), Some(TENANT_KEY));
        }
        // A refused rebinding changes nothing, so the session of the account stays valid.
        assert_eq!(session_status(&env, &session).await, StatusCode::OK);
        // Demoting an administrator into a viewer bound to a writable key is refused the same way.
        let admin = create_account(&env, "Boss", json!({ "role": "admin" })).await;
        let demoted = patch_account(
            &env,
            id_of(&admin),
            json!({ "role": "viewer", "access_key_id": WRITE_KEY }),
        )
        .await;
        assert_exceeds_role(&demoted);
        let stored = crate::db::find_user_by_id(&env.app.db, id_of(&admin))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.role, "admin");
        assert_eq!(stored.access_key_id, None);
    }

    #[tokio::test]
    async fn a_viewer_can_bind_keys_without_bucket_grants_or_optional_flags() {
        let env = bind_env().await;
        for key in ["GKempty", "GKmissingWrite", "GKmissingOwner"] {
            let created = create_account(&env, key, json!({ "access_key_id": key })).await;
            assert_eq!(created.status, StatusCode::CREATED, "{key}: {}", created.body);
            assert_eq!(stored_key(&env, id_of(&created)).await.as_deref(), Some(key));
        }
    }

    #[tokio::test]
    async fn a_viewer_can_still_be_bound_to_a_read_only_key() {
        let env = bind_env().await;
        let created = create_account(&env, "Tess", json!({ "access_key_id": TENANT_KEY })).await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        let id = id_of(&created);
        let changed = patch_account(&env, id, json!({ "access_key_id": OTHER_KEY })).await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
        assert_eq!(stored_key(&env, id).await.as_deref(), Some(OTHER_KEY));
        let tenant = sign_in(&env.app, "Tess", "tenant horse").await;
        let listed = exchange(&env.app, request("GET", "/api/buckets", None, Some(&tenant))).await;
        assert_eq!(listed.status, StatusCode::OK, "{}", listed.body);
        assert_eq!(bucket_ids(&listed.body), ["b1", "b2"]);
    }
