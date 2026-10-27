//! Passkey management: list, register, rename and remove the credentials that
//! can sign in to this server.
//!
//! Mounted inside `require_backup_session` but outside `profiles::admit`:
//! passkeys gate server sign-in and are not scoped to a profile, so these
//! handlers take [`AuthState`] rather than the `Arc<AppState>` that the
//! profile-scoped routers receive.

use axum::{
    extract::{Path, State},
    http::{header::SET_COOKIE, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauthn_rs::prelude::{RegisterPublicKeyCredential, Uuid};

use crate::{
    auth::AuthState,
    error::{ApiError, ApiResult},
    passkeys::{self, PasskeyError, PasskeyInfo},
};

impl From<PasskeyError> for ApiError {
    fn from(error: PasskeyError) -> Self {
        match error {
            PasskeyError::NotConfigured => {
                ApiError::NotImplemented(PasskeyError::NotConfigured.to_string())
            }
            PasskeyError::Invalid(message) => ApiError::BadRequest(message),
            PasskeyError::Rejected => ApiError::BadRequest(PasskeyError::Rejected.to_string()),
            PasskeyError::NotFound => ApiError::NotFound,
            PasskeyError::Internal(message) => ApiError::Internal(message),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PasskeyStatus {
    /// Whether the server is configured for passkeys at all. The Settings page
    /// uses this to explain the feature instead of showing an empty list.
    configured: bool,
    passkeys: Vec<PasskeyInfo>,
}

fn manager(state: &AuthState) -> ApiResult<&std::sync::Arc<passkeys::PasskeyManager>> {
    state
        .passkeys
        .as_ref()
        .ok_or_else(|| ApiError::from(PasskeyError::NotConfigured))
}

async fn list(State(state): State<AuthState>) -> ApiResult<Json<PasskeyStatus>> {
    let Some(passkeys) = state.passkeys.as_ref() else {
        return Ok(Json(PasskeyStatus {
            configured: false,
            passkeys: Vec::new(),
        }));
    };
    Ok(Json(PasskeyStatus {
        configured: true,
        passkeys: passkeys.list()?,
    }))
}

async fn register_start(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let passkeys_manager = manager(&state)?;
    let (options, sealed) = passkeys_manager.start_registration()?;

    let secure = state
        .auth
        .as_ref()
        .is_some_and(|auth| auth.should_secure_cookie(&headers));
    let cookie = passkeys::build_tx_cookie(
        passkeys::REGISTER_TX_COOKIE,
        passkeys::REGISTER_TX_PATH,
        &sealed,
        secure,
    );

    let mut response = Json(options).into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|e| ApiError::Internal(format!("Failed to set cookie: {e}")))?,
    );
    Ok(response)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterFinishRequest {
    name: String,
    credential: RegisterPublicKeyCredential,
}

async fn register_finish(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<RegisterFinishRequest>,
) -> ApiResult<Response> {
    let passkeys_manager = manager(&state)?;

    let sealed = passkeys::read_cookie(&headers, passkeys::REGISTER_TX_COOKIE).ok_or_else(|| {
        ApiError::BadRequest("This registration expired. Please try again.".into())
    })?;

    let clear = passkeys::clear_tx_cookie(passkeys::REGISTER_TX_COOKIE, passkeys::REGISTER_TX_PATH);
    let finished = passkeys_manager.finish_registration(&payload.credential, &sealed, &payload.name);

    let mut response = match finished {
        Ok(info) => Json(info).into_response(),
        Err(error) => ApiError::from(error).into_response(),
    };
    if let Ok(value) = HeaderValue::from_str(&clear) {
        response.headers_mut().append(SET_COOKIE, value);
    }
    Ok(response)
}

#[derive(Deserialize)]
struct RenameRequest {
    name: String,
}

async fn rename(
    State(state): State<AuthState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<RenameRequest>,
) -> ApiResult<Json<PasskeyInfo>> {
    Ok(Json(manager(&state)?.rename(id, &payload.name)?))
}

async fn remove(
    State(state): State<AuthState>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    manager(&state)?.remove(id)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub fn router<S: Clone + Send + Sync + 'static>(state: AuthState) -> Router<S> {
    Router::new()
        .route("/passkeys", get(list))
        .route("/passkeys/register/start", post(register_start))
        .route("/passkeys/register/finish", post(register_finish))
        .route(
            "/passkeys/{id}",
            axum::routing::patch(rename).delete(remove),
        )
        .with_state(state)
}
