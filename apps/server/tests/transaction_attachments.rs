#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    response::Response,
    Router,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;
use wealthfolio_server::{api::app_router, build_state, config::Config, AppState};

const PNG: &[u8] = include_bytes!("../../../e2e/fixtures/transaction-attachments/receipt.png");
const PDF: &[u8] = include_bytes!("../../../e2e/fixtures/transaction-attachments/receipt.pdf");

fn config(path: &std::path::Path) -> Config {
    Config {
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        db_path: path.join("app.db").to_string_lossy().into_owned(),
        cors_allow: vec!["*".into()],
        request_timeout: Duration::from_secs(30),
        static_dir: "dist".into(),
        addons_root: path.join("addons").to_string_lossy().into_owned(),
        raw_secret_key: vec![7; 32],
        secrets_encryption_key: [7; 32],
        database_key: [9; 32],
        db_encryption_required: false,
        auth: None,
        oidc: None,
        mcp_enabled: false,
        mcp_audit_enabled: false,
        mcp_allowed_hosts: None,
    }
}
async fn setup() -> (tempfile::TempDir, Arc<AppState>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let state = build_state(&config).await.unwrap();
    // Seed local cash rows directly: these tests must make no provider requests.
    let conn = state.db_access.connect_rusqlite().unwrap();
    conn.execute_batch("INSERT INTO accounts (id,name,account_type,currency,is_default,is_active,is_archived,tracking_mode,created_at,updated_at) VALUES ('cash','Cash','CASH','USD',0,1,0,'TRANSACTIONS',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP);
        INSERT INTO activities (id,account_id,activity_type,status,activity_date,amount,currency,notes,metadata,is_user_modified,needs_review,created_at,updated_at) VALUES ('transaction-1','cash','WITHDRAWAL','POSTED','2026-10-09T00:00:00Z','42','USD','Regular payee','{\"flow\":{\"is_external\":true},\"source\":\"test\"}',0,0,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP);
        INSERT INTO activities (id,account_id,activity_type,status,activity_date,amount,currency,is_user_modified,needs_review,created_at,updated_at) VALUES ('transaction-2','cash','WITHDRAWAL','POSTED','2026-10-09T00:00:00Z','5','USD',0,0,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP);").unwrap();
    state
        .spending_settings_service
        .update(serde_json::from_value(json!({"enabled":true,"accountIds":["cash"]})).unwrap())
        .await
        .unwrap();
    let app = app_router(state.clone(), &config)
        .unwrap()
        .layer(axum::middleware::from_fn(
            wealthfolio_server::api::security_headers,
        ));
    (dir, state, app)
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Vec<u8>,
    content_type: &str,
) -> Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn json_body(response: Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 32 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
fn multipart(filename: &str, content_type: &str, data: &[u8]) -> Vec<u8> {
    let mut body = format!("--receipt\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n").into_bytes();
    body.extend(data);
    body.extend(b"\r\n--receipt--\r\n");
    body
}
async fn upload(app: &Router, filename: &str, content_type: &str, data: &[u8]) -> Response {
    request(
        app,
        "POST",
        "/api/v1/spending/transactions/transaction-1/attachments",
        multipart(filename, content_type, data),
        "multipart/form-data; boundary=receipt",
    )
    .await
}

#[tokio::test]
async fn detailed_notes_are_saved_with_the_transaction() {
    let (_dir, state, app) = setup().await;
    let before = state
        .activity_service
        .get_activity("transaction-1")
        .unwrap();
    let response = request(
        &app,
        "PUT",
        "/api/v1/activities",
        json!({
            "id":"transaction-1",
            "accountId":"cash",
            "activityType":"WITHDRAWAL",
            "activityDate":"2026-10-09T00:00:00Z",
            "amount":42,
            "currency":"USD",
            "comment":"Regular payee",
            "detailedNotes":"Receipt details\nSecond line"
        })
        .to_string()
        .into_bytes(),
        "application/json",
    )
    .await;
    assert!(response.status().is_success());
    let after = state
        .activity_service
        .get_activity("transaction-1")
        .unwrap();
    assert_eq!(before.notes, after.notes);
    assert_eq!(before.metadata, after.metadata);
    assert_eq!(
        after.detailed_notes.as_deref(),
        Some("Receipt details\nSecond line")
    );

    let oversized = request(
        &app,
        "PUT",
        "/api/v1/activities",
        json!({
            "id":"transaction-1",
            "accountId":"cash",
            "activityType":"WITHDRAWAL",
            "activityDate":"2026-10-09T00:00:00Z",
            "amount":42,
            "currency":"USD",
            "comment":"Regular payee",
            "detailedNotes":"x".repeat(20001)
        })
        .to_string()
        .into_bytes(),
        "application/json",
    )
    .await;
    assert_eq!(oversized.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn attachments_are_private_generated_previews_and_delete_with_transaction() {
    let (_dir, state, app) = setup().await;
    let response = upload(&app, "receipt.png", "image/png", PNG).await;
    assert!(response.status().is_success(), "{}", response.status());
    let attachment = json_body(response).await;
    let id = attachment["id"].as_str().unwrap();
    let path = format!("/api/v1/spending/transactions/transaction-1/attachments/{id}");
    let original = request(&app, "GET", &path, vec![], "").await;
    assert_eq!(original.status(), StatusCode::OK);
    assert_eq!(original.headers()["cache-control"], "private, no-store");
    assert_eq!(
        original.headers()["content-security-policy"],
        "sandbox; default-src 'none'"
    );
    assert_eq!(
        to_bytes(original.into_body(), 32 * 1024 * 1024)
            .await
            .unwrap()
            .as_ref(),
        PNG
    );
    let preview = request(&app, "GET", &format!("{path}/thumbnail"), vec![], "").await;
    assert_eq!(preview.headers()["content-type"], "image/webp");
    let bytes = to_bytes(preview.into_body(), 1024 * 1024).await.unwrap();
    assert_ne!(bytes.as_ref(), PNG);
    assert!(bytes.starts_with(b"RIFF"));
    assert_eq!(&bytes[8..12], b"WEBP");
    let wrong_transaction = path.replace("transaction-1", "transaction-2");
    assert_eq!(
        request(&app, "GET", &wrong_transaction, vec![], "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{wrong_transaction}/thumbnail"),
            vec![],
            ""
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, "DELETE", &wrong_transaction, vec![], "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    state
        .spending_settings_service
        .update(serde_json::from_value(json!({"enabled":false})).unwrap())
        .await
        .unwrap();
    assert_eq!(
        request(&app, "GET", &path, vec![], "").await.status(),
        StatusCode::NOT_FOUND
    );
    state
        .spending_settings_service
        .update(serde_json::from_value(json!({"enabled":true})).unwrap())
        .await
        .unwrap();
    assert!(request(
        &app,
        "DELETE",
        "/api/v1/activities/transaction-1",
        vec![],
        ""
    )
    .await
    .status()
    .is_success());
    assert_eq!(
        request(&app, "GET", &path, vec![], "").await.status(),
        StatusCode::NOT_FOUND
    );
    let root = std::path::Path::new(&state.data_root).join("transaction-attachments");
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}

#[tokio::test]
async fn upload_validation_and_attachment_count_are_enforced() {
    let (_dir, state, app) = setup().await;
    let cross_site = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/spending/transactions/transaction-1/attachments")
                .header("content-type", "multipart/form-data; boundary=receipt")
                .header("origin", "https://untrusted.example")
                .header("sec-fetch-site", "cross-site")
                .body(Body::from(multipart("receipt.png", "image/png", PNG)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_site.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        upload(&app, "script.svg", "image/svg+xml", b"<svg/>")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        upload(&app, "fake.png", "image/png", b"not an image")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        upload(&app, "../receipt.png", "image/png", PNG)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        upload(&app, "empty.png", "image/png", b"").await.status(),
        StatusCode::BAD_REQUEST
    );
    let oversized = upload(
        &app,
        "large.png",
        "image/png",
        &vec![0; 20 * 1024 * 1024 + 1],
    )
    .await;
    assert!(matches!(
        oversized.status(),
        StatusCode::BAD_REQUEST | StatusCode::PAYLOAD_TOO_LARGE
    ));
    for _ in 0..10 {
        assert!(upload(&app, "receipt.png", "image/png", PNG)
            .await
            .status()
            .is_success());
    }
    assert_eq!(
        upload(&app, "receipt.png", "image/png", PNG).await.status(),
        StatusCode::BAD_REQUEST
    );
    let attachments = state
        .transaction_attachments_service
        .list("transaction-1")
        .await
        .unwrap();
    assert_eq!(attachments.len(), 10);
    let id = &attachments[0].id;
    assert!(request(
        &app,
        "DELETE",
        &format!("/api/v1/spending/transactions/transaction-1/attachments/{id}"),
        vec![],
        ""
    )
    .await
    .status()
    .is_success());
    assert_eq!(
        state
            .transaction_attachments_service
            .list("transaction-1")
            .await
            .unwrap()
            .len(),
        9
    );
}

#[tokio::test]
async fn pdf_upload_generates_first_page_preview_when_pdfium_is_installed() {
    let (_dir, _state, app) = setup().await;
    let response = upload(&app, "receipt.pdf", "application/pdf", PDF).await;
    if std::env::var_os("WF_TEST_PDFIUM_REQUIRED").is_none()
        && response.status() == StatusCode::INTERNAL_SERVER_ERROR
    {
        // Without the native library, failure must be explicit and leave no metadata.
        let body = json_body(response).await;
        assert!(body["message"].as_str().unwrap().contains("PDFium"));
        return;
    }
    assert!(response.status().is_success(), "{}", response.status());
    let attachment = json_body(response).await;
    let preview = request(
        &app,
        "GET",
        &format!(
            "/api/v1/spending/transactions/transaction-1/attachments/{}/thumbnail",
            attachment["id"].as_str().unwrap()
        ),
        vec![],
        "",
    )
    .await;
    assert_eq!(preview.status(), StatusCode::OK);
    assert_eq!(preview.headers()["content-type"], "image/webp");
}

async fn scoped_request(
    app: &Router,
    method: &str,
    path: &str,
    data: Value,
    cookie: Option<&str>,
    scope: Option<&str>,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    if let Some(scope) = scope {
        builder = builder.header("x-wf-profile-scope", scope);
    }
    app.clone()
        .oneshot(builder.body(Body::from(data.to_string())).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn profile_scope_and_lock_protect_attachment_metadata_originals_and_thumbnails() {
    let (_dir, _state, app) = setup().await;
    let uploaded = json_body(upload(&app, "receipt.png", "image/png", PNG).await).await;
    let id = uploaded["id"].as_str().unwrap();
    let response = scoped_request(
        &app,
        "POST",
        "/api/v1/profiles/get_profile_state",
        json!({}),
        None,
        None,
    )
    .await;
    let cookie_a = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let initial = json_body(response).await;
    let first = initial["profiles"][0]["id"].clone();
    let scope_a = initial["session"]["scopeId"].as_str().unwrap().to_owned();
    let response = scoped_request(
        &app,
        "POST",
        "/api/v1/profiles/create_profile",
        json!({"name":"Other","avatarId":"default"}),
        Some(&cookie_a),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let second = json_body(response).await;
    let response = scoped_request(
        &app,
        "POST",
        "/api/v1/profiles/get_profile_state",
        json!({}),
        None,
        None,
    )
    .await;
    let cookie_b = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let response = scoped_request(
        &app,
        "POST",
        "/api/v1/profiles/unlock_profile",
        json!({"profileId":second["id"]}),
        Some(&cookie_b),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let grant = json_body(response).await;
    let scope_b = grant["scopeId"].as_str().unwrap();
    let paths = [
        "/api/v1/spending/transactions/transaction-1/attachments".to_owned(),
        format!("/api/v1/spending/transactions/transaction-1/attachments/{id}"),
        format!("/api/v1/spending/transactions/transaction-1/attachments/{id}/thumbnail"),
    ];
    for path in &paths {
        assert_eq!(
            scoped_request(
                &app,
                "GET",
                path,
                json!({}),
                Some(&cookie_a),
                Some(&scope_a)
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert_eq!(
            scoped_request(&app, "GET", path, json!({}), Some(&cookie_b), Some(scope_b))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            scoped_request(
                &app,
                "GET",
                path,
                json!({}),
                Some(&cookie_b),
                Some(&scope_a)
            )
            .await
            .status(),
            StatusCode::LOCKED
        );
    }
    let lock = scoped_request(
        &app,
        "POST",
        "/api/v1/profiles/lock_profile",
        json!({"profileId":first}),
        Some(&cookie_a),
        Some(&scope_a),
    )
    .await;
    assert!(lock.status().is_success());
    for path in &paths {
        assert_eq!(
            scoped_request(
                &app,
                "GET",
                path,
                json!({}),
                Some(&cookie_a),
                Some(&scope_a)
            )
            .await
            .status(),
            StatusCode::LOCKED
        );
    }
}
