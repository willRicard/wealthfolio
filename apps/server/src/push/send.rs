//! Delivery of one encrypted push message to one subscription.

use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL, Engine as _};
use reqwest::header::{AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE};
use web_push_native::{p256::PublicKey, Auth};

use super::{store::StoredSubscription, vapid, VapidConfig};

const TTL_SECS: u64 = 12 * 60 * 60;

const AUTH_SECRET_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    Delivered,
    Expired,
    Failed,
}

fn decode_keys(subscription: &StoredSubscription) -> anyhow::Result<(PublicKey, Auth)> {
    let p256dh = BASE64URL
        .decode(&subscription.p256dh)
        .map_err(|_| anyhow::anyhow!("Subscription p256dh is not base64url"))?;
    let public = PublicKey::from_sec1_bytes(&p256dh)
        .map_err(|_| anyhow::anyhow!("Subscription p256dh is not a P-256 point"))?;

    let auth = BASE64URL
        .decode(&subscription.auth)
        .map_err(|_| anyhow::anyhow!("Subscription auth secret is not base64url"))?;
    anyhow::ensure!(
        auth.len() == AUTH_SECRET_LEN,
        "Subscription auth secret must be {AUTH_SECRET_LEN} bytes"
    );

    Ok((public, Auth::clone_from_slice(&auth)))
}

pub(crate) async fn deliver(
    http: &reqwest::Client,
    vapid_config: &VapidConfig,
    subscription: &StoredSubscription,
    payload: &[u8],
) -> Delivery {
    let host = endpoint_host(&subscription.endpoint);

    let (public, auth) = match decode_keys(subscription) {
        Ok(keys) => keys,
        Err(error) => {
            tracing::warn!(host = %host, "Dropping unusable push subscription: {error}");
            return Delivery::Expired;
        }
    };

    let audience = match vapid::audience_of(&subscription.endpoint) {
        Ok(audience) => audience,
        Err(error) => {
            tracing::warn!(host = %host, "Dropping push subscription: {error}");
            return Delivery::Expired;
        }
    };

    let authorization = match vapid::authorization_header(
        &vapid_config.private_key,
        &vapid_config.public_key,
        &audience,
        &vapid_config.subject,
        chrono::Utc::now().timestamp(),
    ) {
        Ok(header) => header,
        Err(error) => {
            tracing::error!("Cannot build a VAPID authorization header: {error}");
            return Delivery::Failed;
        }
    };

    let ciphertext = match web_push_native::encrypt(payload.to_vec(), &public, &auth) {
        Ok(ciphertext) => ciphertext,
        Err(error) => {
            tracing::warn!(host = %host, "Failed to encrypt a push message: {error}");
            return Delivery::Failed;
        }
    };

    let response = http
        .post(&subscription.endpoint)
        .header("TTL", TTL_SECS.to_string())
        .header("Urgency", "normal")
        .header(CONTENT_ENCODING, "aes128gcm")
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(AUTHORIZATION, authorization)
        .timeout(Duration::from_secs(15))
        .body(ciphertext)
        .send()
        .await;

    match response {
        Ok(response) => classify(response.status().as_u16(), &host),
        Err(error) => {
            tracing::warn!(
                host = %host,
                timeout = error.is_timeout(),
                connect = error.is_connect(),
                "Push delivery request failed"
            );
            Delivery::Failed
        }
    }
}

fn classify(status: u16, host: &str) -> Delivery {
    match status {
        200..=299 => Delivery::Delivered,
        404 | 410 => {
            tracing::info!(host = %host, status, "Pruning an expired push subscription");
            Delivery::Expired
        }
        other => {
            tracing::warn!(host = %host, status = other, "Push service rejected a message");
            Delivery::Failed
        }
    }
}

fn endpoint_host(endpoint: &str) -> String {
    reqwest::Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

    use super::*;

    fn subscription(p256dh: &str, auth: &str) -> StoredSubscription {
        StoredSubscription {
            endpoint: "https://push.example.com/abc".into(),
            p256dh: p256dh.into(),
            auth: auth.into(),
            label: None,
            created_at: "2026-11-04T00:00:00Z".into(),
        }
    }

    const VALID_P256DH: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const VALID_AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";

    #[test]
    fn well_formed_subscription_keys_decode() {
        let (_public, auth) = decode_keys(&subscription(VALID_P256DH, VALID_AUTH)).unwrap();
        assert_eq!(auth.len(), AUTH_SECRET_LEN);
    }

    #[test]
    fn malformed_subscription_keys_are_rejected() {
        assert!(decode_keys(&subscription("not base64 !!", VALID_AUTH)).is_err());
        assert!(decode_keys(&subscription(VALID_P256DH, "c2hvcnQ")).is_err());
        // Valid base64url, but not a curve point.
        assert!(decode_keys(&subscription("AAAA", VALID_AUTH)).is_err());
    }

    #[test]
    fn status_codes_map_to_the_right_outcome() {
        assert_eq!(classify(201, "h"), Delivery::Delivered);
        assert_eq!(classify(200, "h"), Delivery::Delivered);
        assert_eq!(classify(404, "h"), Delivery::Expired);
        assert_eq!(classify(410, "h"), Delivery::Expired);
        assert_eq!(classify(429, "h"), Delivery::Failed);
        assert_eq!(classify(500, "h"), Delivery::Failed);
        assert_eq!(classify(400, "h"), Delivery::Failed);
    }

    #[test]
    fn only_the_host_is_extracted_for_logging() {
        assert_eq!(
            endpoint_host("https://fcm.googleapis.com/fcm/send/secret-token"),
            "fcm.googleapis.com"
        );
        assert_eq!(endpoint_host("garbage"), "unknown");
    }
}
