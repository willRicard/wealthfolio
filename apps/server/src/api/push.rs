use std::sync::Arc;

use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;

use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
    push::{
        PushPayload, PushPreferences, StoredSubscription, MAX_ENDPOINT_LEN, MAX_LABEL_LEN,
        MAX_SUBSCRIPTIONS,
    },
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PushConfigResponse {
    enabled: bool,
    public_key: Option<String>,
}

async fn get_config(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<PushConfigResponse>> {
    Ok(Json(match state.push_service.as_ref() {
        Some(push) => PushConfigResponse {
            enabled: true,
            public_key: Some(push.public_key_b64()),
        },
        None => PushConfigResponse {
            enabled: false,
            public_key: None,
        },
    }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionKeys {
    p256dh: String,
    auth: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscribeBody {
    endpoint: String,
    keys: SubscriptionKeys,
    #[serde(default)]
    label: Option<String>,
}

fn push_service(state: &AppState) -> ApiResult<&crate::push::PushService> {
    state.push_service.as_deref().ok_or_else(|| {
        ApiError::NotImplemented(
            "Push notifications are not configured on this server. Set WF_VAPID_PUBLIC_KEY \
             and WF_VAPID_PRIVATE_KEY."
                .to_string(),
        )
    })
}

fn validate_endpoint(endpoint: &str) -> Result<(), String> {
    if endpoint.len() > MAX_ENDPOINT_LEN {
        return Err(format!(
            "Push endpoint must be at most {MAX_ENDPOINT_LEN} characters"
        ));
    }
    let url =
        reqwest::Url::parse(endpoint).map_err(|_| "Push endpoint is not a URL".to_string())?;
    if url.scheme() != "https" {
        return Err("Push endpoint must use https".to_string());
    }
    if url.host_str().is_none() {
        return Err("Push endpoint must include a host".to_string());
    }
    Ok(())
}

async fn subscribe(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<SubscribeBody>,
) -> ApiResult<StatusCode> {
    let push = push_service(&state)?;
    validate_endpoint(&body.endpoint).map_err(ApiError::BadRequest)?;

    let label = body
        .label
        .map(|label| label.trim().chars().take(MAX_LABEL_LEN).collect::<String>())
        .filter(|label| !label.is_empty());

    let subscription = StoredSubscription {
        endpoint: body.endpoint,
        p256dh: body.keys.p256dh,
        auth: body.keys.auth,
        label,
        created_at: Utc::now().to_rfc3339(),
    };

    push.store()
        .with_file(|file| {
            file.subscriptions
                .retain(|existing| existing.endpoint != subscription.endpoint);
            if file.subscriptions.len() >= MAX_SUBSCRIPTIONS {
                anyhow::bail!(
                    "This server already has {MAX_SUBSCRIPTIONS} registered browsers. \
                     Remove one before adding another."
                );
            }
            file.subscriptions.push(subscription);
            if file.state.last_activity_seen_at.is_none() {
                file.state.last_activity_seen_at = Some(Utc::now());
            }
            Ok(())
        })
        .map_err(ApiError::Anyhow)?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnsubscribeBody {
    endpoint: String,
}

async fn unsubscribe(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<UnsubscribeBody>,
) -> ApiResult<StatusCode> {
    let push = push_service(&state)?;
    push.store()
        .with_file(|file| {
            file.subscriptions
                .retain(|existing| existing.endpoint != body.endpoint);
            Ok(())
        })
        .map_err(ApiError::Anyhow)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PreferencesResponse {
    transactions: bool,
    budgets: bool,
    subscription_count: usize,
}

async fn get_preferences(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<PreferencesResponse>> {
    let push = push_service(&state)?;
    let file = push.store().read().map_err(ApiError::Anyhow)?;
    Ok(Json(PreferencesResponse {
        transactions: file.preferences.transactions,
        budgets: file.preferences.budgets,
        subscription_count: file.subscriptions.len(),
    }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreferencesBody {
    transactions: bool,
    budgets: bool,
}

async fn update_preferences(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<PreferencesBody>,
) -> ApiResult<StatusCode> {
    let push = push_service(&state)?;
    push.store()
        .with_file(|file| {
            file.preferences = PushPreferences {
                transactions: body.transactions,
                budgets: body.budgets,
            };
            Ok(())
        })
        .map_err(ApiError::Anyhow)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TestResponse {
    delivered: usize,
}

async fn send_test(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<TestResponse>> {
    let push = push_service(&state)?;
    let delivered = push
        .send_to_all(&[PushPayload::test()])
        .await
        .map_err(ApiError::Anyhow)?;
    Ok(Json(TestResponse { delivered }))
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/push/config", get(get_config))
        .route("/push/subscriptions", post(subscribe).delete(unsubscribe))
        .route(
            "/push/preferences",
            get(get_preferences).put(update_preferences),
        )
        .route("/push/test", post(send_test))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

    use super::*;

    #[test]
    fn only_https_endpoints_with_a_host_are_accepted() {
        assert!(validate_endpoint("https://fcm.googleapis.com/fcm/send/abc").is_ok());
        assert!(validate_endpoint("http://fcm.googleapis.com/fcm/send/abc").is_err());
        assert!(validate_endpoint("file:///etc/passwd").is_err());
        assert!(validate_endpoint("not a url").is_err());
    }

    #[test]
    fn an_over_long_endpoint_is_rejected_before_parsing() {
        let endpoint = format!("https://push.example.com/{}", "a".repeat(MAX_ENDPOINT_LEN));
        assert!(validate_endpoint(&endpoint).is_err());
    }
}
