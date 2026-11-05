//! Sealed, file-backed storage for push subscriptions and alert state.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use chrono::{DateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

const CURRENT_VERSION: u32 = 1;

pub(crate) const MAX_SUBSCRIPTIONS: usize = 20;

pub(crate) const MAX_ENDPOINT_LEN: usize = 1024;
pub(crate) const MAX_LABEL_LEN: usize = 64;

pub(crate) const PUSH_FILE_NAME: &str = "push.json";

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    nonce: String,
    ciphertext: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StoredSubscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    #[serde(default)]
    pub label: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PushPreferences {
    pub transactions: bool,
    pub budgets: bool,
}

impl Default for PushPreferences {
    fn default() -> Self {
        Self {
            transactions: true,
            budgets: true,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct AlertState {
    #[serde(default)]
    pub last_activity_seen_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub budget_period_key: Option<String>,
    #[serde(default)]
    pub budget_levels: BTreeMap<String, u8>,
}

pub(crate) fn budget_level_key(taxonomy_id: &str, category_id: &str) -> String {
    format!("{taxonomy_id}|{category_id}")
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct PushFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub subscriptions: Vec<StoredSubscription>,
    #[serde(default)]
    pub preferences: PushPreferences,
    #[serde(default)]
    pub state: AlertState,
}

pub(crate) struct PushStore {
    path: PathBuf,
    key: [u8; 32],
    lock: Mutex<()>,
}

impl PushStore {
    pub(crate) fn new(path: PathBuf, key: [u8; 32]) -> Self {
        Self {
            path,
            key,
            lock: Mutex::new(()),
        }
    }

    pub(crate) fn read(&self) -> anyhow::Result<PushFile> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Push store lock poisoned"))?;
        self.load_locked()
    }

    pub(crate) fn with_file<T, F>(&self, op: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut PushFile) -> anyhow::Result<T>,
    {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Push store lock poisoned"))?;
        let mut file = self.load_locked()?;
        let result = op(&mut file)?;
        self.persist_locked(&file)?;
        Ok(result)
    }

    fn load_locked(&self) -> anyhow::Result<PushFile> {
        if !self.path.exists() {
            return Ok(PushFile::default());
        }
        let raw = fs::read(&self.path)?;
        if raw.is_empty() {
            return Ok(PushFile::default());
        }
        let envelope: Envelope = serde_json::from_slice(&raw).map_err(|e| {
            anyhow::anyhow!(
                "The push notification file at {} is malformed: {e}",
                self.path.display()
            )
        })?;
        let nonce_bytes = BASE64URL.decode(&envelope.nonce)?;
        let ciphertext = BASE64URL.decode(&envelope.ciphertext)?;
        if nonce_bytes.len() != 12 {
            anyhow::bail!("Invalid nonce length in the push notification file");
        }
        let plaintext = ChaCha20Poly1305::new(Key::from_slice(&self.key))
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
            .map_err(|_| {
                anyhow::anyhow!(
                    "Cannot decrypt {}: it was sealed with a different WF_SECRET_KEY. \
                     Restore the original key, or delete the file to re-enable \
                     notifications on each browser.",
                    self.path.display()
                )
            })?;
        Ok(serde_json::from_slice(&plaintext)?)
    }

    fn persist_locked(&self, file: &PushFile) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_vec(file)?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = ChaCha20Poly1305::new(Key::from_slice(&self.key))
            .encrypt(Nonce::from_slice(&nonce_bytes), serialized.as_ref())
            .map_err(|_| anyhow::anyhow!("Failed to encrypt the push notification file"))?;
        let envelope = Envelope {
            version: CURRENT_VERSION,
            nonce: BASE64URL.encode(nonce_bytes),
            ciphertext: BASE64URL.encode(ciphertext),
        };
        write_private(
            &self.path,
            serde_json::to_string_pretty(&envelope)?.as_bytes(),
        )
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

    use super::*;

    fn store(dir: &tempfile::TempDir) -> PushStore {
        PushStore::new(dir.path().join(PUSH_FILE_NAME), [7u8; 32])
    }

    fn subscription(endpoint: &str) -> StoredSubscription {
        StoredSubscription {
            endpoint: endpoint.to_string(),
            p256dh: "p256dh".into(),
            auth: "auth".into(),
            label: None,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn missing_file_reads_as_empty_with_defaults_on() {
        let dir = tempfile::tempdir().unwrap();
        let file = store(&dir).read().unwrap();
        assert!(file.subscriptions.is_empty());
        assert!(file.preferences.transactions);
        assert!(file.preferences.budgets);
        assert!(file.state.last_activity_seen_at.is_none());
    }

    #[test]
    fn written_state_round_trips_through_the_sealed_envelope() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let now = Utc::now();
        store
            .with_file(|file| {
                file.subscriptions.push(subscription("https://push/1"));
                file.preferences.budgets = false;
                file.state.last_activity_seen_at = Some(now);
                file.state.budget_period_key = Some("2026-11".into());
                file.state
                    .budget_levels
                    .insert(budget_level_key("spending_categories", "food"), 80);
                Ok(())
            })
            .unwrap();

        let reloaded = store.read().unwrap();
        assert_eq!(reloaded.subscriptions.len(), 1);
        assert_eq!(reloaded.subscriptions[0].endpoint, "https://push/1");
        assert!(!reloaded.preferences.budgets);
        assert!(reloaded.preferences.transactions);
        assert_eq!(reloaded.state.last_activity_seen_at, Some(now));
        assert_eq!(
            reloaded
                .state
                .budget_levels
                .get(&budget_level_key("spending_categories", "food")),
            Some(&80)
        );
    }

    #[test]
    fn the_file_on_disk_is_an_envelope_that_leaks_no_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        store
            .with_file(|file| {
                file.subscriptions
                    .push(subscription("https://fcm.example/secret-endpoint"));
                Ok(())
            })
            .unwrap();
        let raw = fs::read_to_string(dir.path().join(PUSH_FILE_NAME)).unwrap();
        assert!(raw.contains("ciphertext"));
        assert!(!raw.contains("secret-endpoint"));
    }

    #[test]
    fn a_different_key_cannot_open_the_file() {
        let dir = tempfile::tempdir().unwrap();
        store(&dir)
            .with_file(|file| {
                file.subscriptions.push(subscription("https://push/1"));
                Ok(())
            })
            .unwrap();
        let other = PushStore::new(dir.path().join(PUSH_FILE_NAME), [9u8; 32]);
        assert!(other.read().is_err());
    }
}
