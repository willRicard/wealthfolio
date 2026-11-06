#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! End-to-end coverage of the Web Push API: subscription storage, preferences,
//! the test-send endpoint, and the auth guard. Everything runs in one test
//! because `Config::from_env` reads process-global environment variables, which
//! parallel test threads would race on.

use std::net::SocketAddr;

use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::{header, Method, Request},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::{rngs::OsRng, RngCore};
use tempfile::tempdir;
use tower::ServiceExt;
use wealthfolio_server::{api::app_router, build_state, config::Config};

const VAPID_PRIVATE_KEY: &str = "9D0U90ZLAsMKDM0u5mzTvWQ-TJIFUmogL-cANnqWLC4";
const VAPID_PUBLIC_KEY: &str =
    "BGe4QPEYRzZL9x2Vlk1evz1zbuPL_IoY6GlrT_PE4u-_QGs52m1i2pCu5U8htJHiolijevP1Z8cnw3fo-HSQ4xw";

const ENDPOINT: &str = "https://push.example.com/registration/abc123";

fn set_env(password: &str) -> tempfile::TempDir {
    let tmp = tempdir().unwrap();
    std::env::set_var("WF_DATA_DIR", "");
    std::env::set_var("WF_DB_PATH", tmp.path().join("test.db"));

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string();
    std::env::set_var("WF_AUTH_PASSWORD_HASH", password_hash);

    let mut secret_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut secret_bytes);
    std::env::set_var("WF_SECRET_KEY", BASE64.encode(secret_bytes));
    std::env::set_var("WF_CORS_ALLOW_ORIGINS", "http://localhost:3000");
    std::env::set_var("WF_VAPID_PUBLIC_KEY", VAPID_PUBLIC_KEY);
    std::env::set_var("WF_VAPID_PRIVATE_KEY", VAPID_PRIVATE_KEY);

    tmp
}

fn cleanup_env() {
    for key in [
        "WF_DB_PATH",
        "WF_AUTH_PASSWORD_HASH",
        "WF_SECRET_KEY",
        "WF_CORS_ALLOW_ORIGINS",
        "WF_VAPID_PUBLIC_KEY",
        "WF_VAPID_PRIVATE_KEY",
    ] {
        std::env::remove_var(key);
    }
}

fn request(method: Method, uri: &str, body: Option<String>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        return builder.body(Body::from(body)).unwrap();
    }
    builder.body(Body::empty()).unwrap()
}

async fn login(app: &axum::Router, password: &str) -> String {
    let mut login_req = request(
        Method::POST,
        "/api/v1/auth/login",
        Some(serde_json::json!({ "password": password }).to_string()),
    );
    login_req
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    let login_response = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(login_response.status(), 200);
    let set_cookie = login_response
        .headers()
        .get(header::SET_COOKIE)
        .expect("login should set a cookie")
        .to_str()
        .unwrap()
        .to_owned();
    set_cookie
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("wf_session=")
        .to_string()
}

async fn call(
    app: &axum::Router,
    cookie: Option<&str>,
    method: Method,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, format!("wf_session={cookie}"));
    }
    let request = match body {
        Some(value) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(value.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

#[tokio::test]
async fn push_api_full_flow() {
    let password = "super-secret";
    let _tmp = set_env(password);
    let config = Config::from_env().unwrap();
    let state = build_state(&config).await.unwrap();
    let app = app_router(state, &config).unwrap();

    let (status, _) = call(&app, None, Method::GET, "/api/v1/push/config", None).await;
    assert_eq!(status, 401);

    let cookie = login(&app, password).await;

    let (status, body) = call(
        &app,
        Some(&cookie),
        Method::GET,
        "/api/v1/push/config",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["publicKey"], VAPID_PUBLIC_KEY);

    let (status, _) = call(
        &app,
        Some(&cookie),
        Method::POST,
        "/api/v1/push/subscriptions",
        Some(serde_json::json!({
            "endpoint": "http://push.example.com/registration/abc123",
            "keys": { "p256dh": "cDI2NmRrZXk", "auth": "MTYteWJ5dGVzLXNlY3JldA" }
        })),
    )
    .await;
    assert_eq!(status, 400);

    let (status, _) = call(
        &app,
        Some(&cookie),
        Method::POST,
        "/api/v1/push/subscriptions",
        Some(serde_json::json!({
            "endpoint": ENDPOINT,
            "keys": { "p256dh": "cDI2NmRrZXk", "auth": "MTYteWJ5dGVzLXNlY3JldA" },
            "label": "Test browser"
        })),
    )
    .await;
    assert_eq!(status, 204);

    let (status, body) = call(
        &app,
        Some(&cookie),
        Method::GET,
        "/api/v1/push/preferences",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["transactions"], true);
    assert_eq!(body["budgets"], true);
    assert_eq!(body["subscriptionCount"], 1);

    let (status, _) = call(
        &app,
        Some(&cookie),
        Method::PUT,
        "/api/v1/push/preferences",
        Some(serde_json::json!({ "transactions": false, "budgets": true })),
    )
    .await;
    assert_eq!(status, 204);
    let (_, body) = call(
        &app,
        Some(&cookie),
        Method::GET,
        "/api/v1/push/preferences",
        None,
    )
    .await;
    assert_eq!(body["transactions"], false);
    assert_eq!(body["budgets"], true);

    let (status, _) = call(
        &app,
        Some(&cookie),
        Method::DELETE,
        "/api/v1/push/subscriptions",
        Some(serde_json::json!({ "endpoint": ENDPOINT })),
    )
    .await;
    assert_eq!(status, 204);
    let (_, body) = call(
        &app,
        Some(&cookie),
        Method::GET,
        "/api/v1/push/preferences",
        None,
    )
    .await;
    assert_eq!(body["subscriptionCount"], 0);

    let (status, body) = call(&app, Some(&cookie), Method::POST, "/api/v1/push/test", None).await;
    assert_eq!(status, 200);
    assert_eq!(body["delivered"], 0);

    cleanup_env();
}
