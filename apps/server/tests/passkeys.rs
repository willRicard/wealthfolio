#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use std::net::SocketAddr;

use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::{header, Method, Request, Response},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::{rngs::OsRng, RngCore};
use tempfile::tempdir;
use tower::ServiceExt;
use wealthfolio_server::{api::app_router, build_state, config::Config};

const PASSWORD: &str = "correct horse battery staple";
const RP_ID: &str = "folio.example.com";

async fn build_router(data_dir: &std::path::Path, webauthn: bool) -> axum::Router {
    std::env::set_var("WF_DATA_DIR", "");
    std::env::set_var("WF_DB_PATH", data_dir.join("test.db"));

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(PASSWORD.as_bytes(), &salt)
        .unwrap()
        .to_string();
    std::env::set_var("WF_AUTH_PASSWORD_HASH", password_hash);

    let mut secret_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut secret_bytes);
    std::env::set_var("WF_SECRET_KEY", BASE64.encode(secret_bytes));
    std::env::set_var("WF_CORS_ALLOW_ORIGINS", "https://folio.example.com");

    if webauthn {
        std::env::set_var("WF_WEBAUTHN_RP_ID", RP_ID);
    } else {
        std::env::remove_var("WF_WEBAUTHN_RP_ID");
    }

    let config = Config::from_env().unwrap();
    let state = build_state(&config).await.unwrap();
    app_router(state, &config).unwrap()
}

fn cleanup_env() {
    for key in [
        "WF_DB_PATH",
        "WF_AUTH_PASSWORD_HASH",
        "WF_SECRET_KEY",
        "WF_CORS_ALLOW_ORIGINS",
        "WF_WEBAUTHN_RP_ID",
    ] {
        std::env::remove_var(key);
    }
}

fn request(method: Method, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9999))))
}

fn set_cookies(response: &Response<Body>) -> Vec<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_owned)
        .collect()
}

fn cookie_value(cookies: &[String], name: &str) -> Option<String> {
    cookies.iter().find_map(|cookie| {
        cookie
            .split(';')
            .next()?
            .trim()
            .strip_prefix(&format!("{name}="))
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

async fn json(response: Response<Body>) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

async fn sign_in(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            request(Method::POST, "/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "password": PASSWORD }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    cookie_value(&set_cookies(&response), "wf_session").expect("login must set a session cookie")
}

fn bogus_assertion() -> serde_json::Value {
    serde_json::json!({
        "id": "AAAA",
        "rawId": "AAAA",
        "type": "public-key",
        "response": {
            "clientDataJSON": "AAAA",
            "authenticatorData": "AAAA",
            "signature": "AAAA",
            "userHandle": null
        },
        "extensions": {}
    })
}

#[tokio::test]
async fn passkey_routes_behave_with_and_without_webauthn_configured() {
    // ---------------------------------------------------------------- enabled
    let tmp = tempdir().unwrap();
    let app = build_router(tmp.path(), true).await;

    // Configured but empty: the login page must not offer a method that cannot
    // yet succeed.
    let status = json(
        app.clone()
            .oneshot(request(Method::GET, "/api/v1/auth/status").body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status["requiresPassword"], true);
    assert_eq!(status["passkeyEnabled"], false);

    // -- anonymous challenge ------------------------------------------------
    let start = app
        .clone()
        .oneshot(
            request(Method::POST, "/api/v1/auth/passkey/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), 200);
    let cookies = set_cookies(&start);
    let tx = cookie_value(&cookies, "wf_pk_login").expect("start must seal a challenge cookie");
    let tx_cookie = cookies
        .iter()
        .find(|c| c.starts_with("wf_pk_login="))
        .unwrap();
    assert!(tx_cookie.contains("HttpOnly"), "{tx_cookie}");
    assert!(
        tx_cookie.contains("Path=/api/v1/auth/passkey"),
        "the challenge cookie must not be sent to unrelated routes: {tx_cookie}"
    );

    let options = json(start).await;
    let public_key = &options["publicKey"];
    assert_eq!(public_key["rpId"], RP_ID);
    assert_eq!(public_key["userVerification"], "required");
    let allow = &public_key["allowCredentials"];
    assert!(
        allow.is_null() || allow.as_array().is_some_and(|a| a.is_empty()),
        "autofill needs an empty allowCredentials: {allow}"
    );

    // Challenges must be fresh on every call.
    let second = app
        .clone()
        .oneshot(
            request(Method::POST, "/api/v1/auth/passkey/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let second_tx = cookie_value(&set_cookies(&second), "wf_pk_login").unwrap();
    assert_ne!(tx, second_tx, "each challenge must be unique");
    let second_options = json(second).await;
    assert_ne!(
        public_key["challenge"], second_options["publicKey"]["challenge"],
        "each challenge must be unique"
    );

    // -- finishing without, and with, a challenge ---------------------------
    let no_cookie = app
        .clone()
        .oneshot(
            request(Method::POST, "/api/v1/auth/passkey/finish")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(bogus_assertion().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        no_cookie.status(),
        400,
        "a missing challenge is a retryable client error, not a server fault"
    );

    let rejected = app
        .clone()
        .oneshot(
            request(Method::POST, "/api/v1/auth/passkey/finish")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, format!("wf_pk_login={tx}"))
                .body(Body::from(bogus_assertion().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), 401);
    let cleared = set_cookies(&rejected);
    assert!(
        cleared.iter().any(|c| c.starts_with("wf_pk_login=") && c.contains("Max-Age=0")),
        "a spent challenge must be cleared even when the assertion fails: {cleared:?}"
    );
    assert!(
        cookie_value(&cleared, "wf_session").is_none(),
        "a failed assertion must never mint a session"
    );

    // -- management requires a session --------------------------------------
    for (method, uri) in [
        (Method::GET, "/api/v1/passkeys"),
        (Method::POST, "/api/v1/passkeys/register/start"),
    ] {
        let response = app
            .clone()
            .oneshot(request(method.clone(), uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 401, "{method} {uri} must require a session");
    }

    let session = sign_in(&app).await;
    let authed = |method: Method, uri: &str| {
        request(method, uri).header(header::COOKIE, format!("wf_session={session}"))
    };

    let listed = app
        .clone()
        .oneshot(authed(Method::GET, "/api/v1/passkeys").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let listed = json(listed).await;
    assert_eq!(listed["configured"], true);
    assert_eq!(listed["passkeys"].as_array().unwrap().len(), 0);

    // -- registration options ------------------------------------------------
    let register = app
        .clone()
        .oneshot(
            authed(Method::POST, "/api/v1/passkeys/register/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), 200);
    let register_cookies = set_cookies(&register);
    assert!(
        cookie_value(&register_cookies, "wf_pk_reg").is_some(),
        "registration must seal its own challenge, separate from login"
    );
    let creation = json(register).await;
    let selection = &creation["publicKey"]["authenticatorSelection"];
    assert_eq!(
        selection["residentKey"], "required",
        "a non-discoverable credential would register and then never work at sign-in"
    );
    assert_eq!(selection["requireResidentKey"], true);
    assert_eq!(creation["publicKey"]["rp"]["id"], RP_ID);

    // -- removing something that is not there --------------------------------
    let missing = uuid::Uuid::new_v4();
    let response = app
        .clone()
        .oneshot(
            authed(Method::DELETE, &format!("/api/v1/passkeys/{missing}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    // --------------------------------------------------------------- disabled
    let tmp = tempdir().unwrap();
    let app = build_router(tmp.path(), false).await;

    let status = json(
        app.clone()
            .oneshot(request(Method::GET, "/api/v1/auth/status").body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status["passkeyEnabled"], false);

    let start = app
        .clone()
        .oneshot(
            request(Method::POST, "/api/v1/auth/passkey/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        start.status(),
        404,
        "an unconfigured server must not hand out challenges"
    );

    let session = sign_in(&app).await;
    let listed = app
        .clone()
        .oneshot(
            request(Method::GET, "/api/v1/passkeys")
                .header(header::COOKIE, format!("wf_session={session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        listed.status(),
        200,
        "the settings page should render its explanation, not an error"
    );
    assert_eq!(json(listed).await["configured"], false);

    cleanup_env();
}
