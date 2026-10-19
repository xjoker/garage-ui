// Integration tests link the library without `cfg(test)`, so this is the release router.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn call(router: &axum::Router, method: &str, uri: &str) -> (StatusCode, Option<String>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    (response.status(), content_type)
}

// The fixture-backed preview and download routes and the upload stub exist only in unit-test
// builds. In a release build their paths fall through to the same handler as an unknown path.
#[tokio::test]
async fn release_router_has_no_test_fixture_routes() {
    let dir = std::env::temp_dir().join(format!("garage-ui-release-routes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: this test binary has a single test, and the variable is set before boot reads it.
    unsafe { std::env::set_var("DATA_DIR", &dir) };
    let app = garage_ui::boot().await.expect("boot");
    let router = garage_ui::router(app);

    for (method, path) in [
        ("GET", "/api/objects/preview?name=page.html"),
        ("GET", "/api/objects/download?name=icon.svg"),
        ("POST", "/api/objects"),
    ] {
        let unknown = call(&router, method, "/api/no-such-route").await;
        let got = call(&router, method, path).await;
        assert_eq!(got, unknown, "{method} {path} is still routed");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
