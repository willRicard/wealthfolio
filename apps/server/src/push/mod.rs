//! Web Push notifications for the web runtime.

mod payload;
mod send;
mod store;
mod vapid;
mod worker;

use std::path::PathBuf;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL, Engine as _};

pub(crate) use payload::PushPayload;
pub(crate) use store::{
    PushPreferences, PushStore, StoredSubscription, MAX_ENDPOINT_LEN, MAX_LABEL_LEN,
    MAX_SUBSCRIPTIONS, PUSH_FILE_NAME,
};
pub(crate) use worker::start_push_worker;

const PUBLIC_KEY_LEN: usize = 65;
const PRIVATE_KEY_LEN: usize = 32;

#[derive(Clone)]
pub struct VapidConfig {
    pub(crate) public_key: Vec<u8>,
    pub(crate) private_key: [u8; 32],
    pub(crate) subject: String,
}

impl VapidConfig {
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let read = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        Self::resolve(
            read("WF_VAPID_PUBLIC_KEY"),
            read("WF_VAPID_PRIVATE_KEY"),
            read("WF_VAPID_SUBJECT"),
        )
    }

    pub(crate) fn resolve(
        public_key: Option<String>,
        private_key: Option<String>,
        subject: Option<String>,
    ) -> anyhow::Result<Option<Self>> {
        let (public_key, private_key) = match (public_key, private_key) {
            (None, None) => return Ok(None),
            (Some(public_key), Some(private_key)) => (public_key, private_key),
            (Some(_), None) => anyhow::bail!(
                "WF_VAPID_PUBLIC_KEY is set but WF_VAPID_PRIVATE_KEY is not. \
                 Set both halves of the key pair, or neither to disable push notifications."
            ),
            (None, Some(_)) => anyhow::bail!(
                "WF_VAPID_PRIVATE_KEY is set but WF_VAPID_PUBLIC_KEY is not. \
                 Set both halves of the key pair, or neither to disable push notifications."
            ),
        };

        let public_key = BASE64URL.decode(&public_key).map_err(|_| {
            anyhow::anyhow!(
                "WF_VAPID_PUBLIC_KEY must be base64url without padding, as produced by \
                 `npx web-push generate-vapid-keys`"
            )
        })?;
        anyhow::ensure!(
            public_key.len() == PUBLIC_KEY_LEN && public_key[0] == 0x04,
            "WF_VAPID_PUBLIC_KEY must be an uncompressed P-256 point ({PUBLIC_KEY_LEN} bytes \
             starting with 0x04), got {} bytes",
            public_key.len()
        );

        let private_bytes = BASE64URL.decode(&private_key).map_err(|_| {
            anyhow::anyhow!("WF_VAPID_PRIVATE_KEY must be base64url without padding")
        })?;
        let private_key: [u8; PRIVATE_KEY_LEN] =
            private_bytes.try_into().map_err(|bytes: Vec<u8>| {
                anyhow::anyhow!(
                    "WF_VAPID_PRIVATE_KEY must be {PRIVATE_KEY_LEN} bytes, got {}",
                    bytes.len()
                )
            })?;

        let derived = p256::ecdsa::SigningKey::from_slice(&private_key)
            .map_err(|_| anyhow::anyhow!("WF_VAPID_PRIVATE_KEY is not a valid P-256 scalar"))?;
        let derived_public = {
            use p256::elliptic_curve::sec1::ToEncodedPoint;
            p256::ecdsa::VerifyingKey::from(&derived)
                .as_affine()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec()
        };
        anyhow::ensure!(
            derived_public == public_key,
            "WF_VAPID_PUBLIC_KEY does not match WF_VAPID_PRIVATE_KEY. \
             Both must come from the same generated pair."
        );

        let subject = subject.unwrap_or_else(|| "mailto:admin@localhost".to_string());
        anyhow::ensure!(
            subject.starts_with("mailto:") || subject.starts_with("https://"),
            "WF_VAPID_SUBJECT must be a mailto: or https:// contact, got \"{subject}\""
        );

        Ok(Some(Self {
            public_key,
            private_key,
            subject,
        }))
    }
}

pub struct PushService {
    vapid: VapidConfig,
    http: reqwest::Client,
    store: PushStore,
}

impl PushService {
    pub(crate) fn new(vapid: VapidConfig, path: PathBuf, seal_key: [u8; 32]) -> Self {
        Self {
            vapid,
            http: reqwest::Client::new(),
            store: PushStore::new(path, seal_key),
        }
    }

    pub(crate) fn store(&self) -> &PushStore {
        &self.store
    }

    pub(crate) fn public_key_b64(&self) -> String {
        BASE64URL.encode(&self.vapid.public_key)
    }

    pub(crate) async fn send_to_all(&self, payloads: &[PushPayload]) -> anyhow::Result<usize> {
        if payloads.is_empty() {
            return Ok(0);
        }
        let subscriptions = self.store.read()?.subscriptions;
        if subscriptions.is_empty() {
            return Ok(0);
        }

        let mut expired = Vec::new();
        let mut delivered = 0usize;

        for subscription in &subscriptions {
            for payload in payloads {
                let body = match serde_json::to_vec(payload) {
                    Ok(body) => body,
                    Err(error) => {
                        tracing::error!("Cannot serialize a push payload: {error}");
                        continue;
                    }
                };
                match send::deliver(&self.http, &self.vapid, subscription, &body).await {
                    send::Delivery::Delivered => delivered += 1,
                    send::Delivery::Expired => {
                        expired.push(subscription.endpoint.clone());
                        break;
                    }
                    send::Delivery::Failed => {}
                }
            }
        }

        if !expired.is_empty() {
            self.store.with_file(|file| {
                file.subscriptions
                    .retain(|existing| !expired.contains(&existing.endpoint));
                Ok(())
            })?;
        }

        Ok(delivered)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

    use super::*;

    const PRIVATE_BYTES: [u8; 32] = [0x11; 32];
    const OTHER_PRIVATE_BYTES: [u8; 32] = [0x22; 32];

    fn private_b64() -> String {
        BASE64URL.encode(PRIVATE_BYTES)
    }

    fn public_b64_of(private: &[u8; 32]) -> String {
        use p256::elliptic_curve::sec1::ToEncodedPoint;
        let signing = p256::ecdsa::SigningKey::from_slice(private).unwrap();
        BASE64URL.encode(
            p256::ecdsa::VerifyingKey::from(&signing)
                .as_affine()
                .to_encoded_point(false)
                .as_bytes(),
        )
    }

    fn public_b64() -> String {
        public_b64_of(&PRIVATE_BYTES)
    }

    #[test]
    fn both_keys_absent_disables_push_without_an_error() {
        assert!(VapidConfig::resolve(None, None, None).unwrap().is_none());
    }

    #[test]
    fn a_half_configured_pair_refuses_to_start() {
        assert!(VapidConfig::resolve(Some(public_b64()), None, None).is_err());
        assert!(VapidConfig::resolve(None, Some(private_b64()), None).is_err());
    }

    #[test]
    fn a_matched_pair_resolves_with_the_default_subject() {
        let config = VapidConfig::resolve(Some(public_b64()), Some(private_b64()), None)
            .unwrap()
            .unwrap();
        assert_eq!(config.public_key.len(), PUBLIC_KEY_LEN);
        assert_eq!(config.public_key[0], 0x04);
        assert_eq!(config.subject, "mailto:admin@localhost");
    }

    #[test]
    fn keys_from_different_pairs_are_rejected() {
        let other_public = public_b64_of(&OTHER_PRIVATE_BYTES);
        let error = match VapidConfig::resolve(Some(other_public), Some(private_b64()), None) {
            Ok(_) => panic!("mismatched halves must not resolve"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn malformed_keys_are_rejected_with_a_specific_message() {
        assert!(
            VapidConfig::resolve(Some("not base64 !!".into()), Some(private_b64()), None).is_err()
        );
        assert!(
            VapidConfig::resolve(Some(public_b64()), Some(BASE64URL.encode([1u8; 16])), None)
                .is_err()
        );
    }

    #[test]
    fn the_subject_must_be_a_contact_uri() {
        assert!(VapidConfig::resolve(
            Some(public_b64()),
            Some(private_b64()),
            Some("admin@example.com".into())
        )
        .is_err());
        let config = VapidConfig::resolve(
            Some(public_b64()),
            Some(private_b64()),
            Some("https://example.com/contact".into()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(config.subject, "https://example.com/contact");
    }
}
