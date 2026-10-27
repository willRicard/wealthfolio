use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use chrono::{SecondsFormat, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use webauthn_rs::prelude::*;
use webauthn_rs_proto::ResidentKeyRequirement;

pub const TX_TTL_SECS: u64 = 300;

pub const LOGIN_TX_COOKIE: &str = "wf_pk_login";
pub const LOGIN_TX_PATH: &str = "/api/v1/auth/passkey";
pub const REGISTER_TX_COOKIE: &str = "wf_pk_reg";
pub const REGISTER_TX_PATH: &str = "/api/v1/passkeys";

const CURRENT_VERSION: u32 = 1;
const MAX_NAME_LEN: usize = 64;
const MAX_CREDENTIALS: usize = 50;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct WebauthnConfig {
    pub rp_id: String,
    pub rp_origin: Url,
    pub rp_name: String,
}

impl WebauthnConfig {
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let read = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        Self::resolve(
            read("WF_WEBAUTHN_RP_ID"),
            read("WF_WEBAUTHN_ORIGIN"),
            read("WF_WEBAUTHN_RP_NAME"),
        )
    }

    pub(crate) fn resolve(
        rp_id: Option<String>,
        origin: Option<String>,
        rp_name: Option<String>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(rp_id) = rp_id else {
            return Ok(None);
        };

        if rp_id.contains("://") || rp_id.contains('/') || rp_id.contains(':') {
            anyhow::bail!(
                "WF_WEBAUTHN_RP_ID must be a bare domain such as \"folio.example.com\", \
                 not a URL or host:port. Set WF_WEBAUTHN_ORIGIN for the full origin."
            );
        }

        let origin_raw = origin.unwrap_or_else(|| format!("https://{rp_id}"));

        let rp_origin = Url::parse(&origin_raw)
            .map_err(|e| anyhow::anyhow!("Invalid WF_WEBAUTHN_ORIGIN \"{origin_raw}\": {e}"))?;

        let host = rp_origin
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("WF_WEBAUTHN_ORIGIN must include a host"))?
            .to_ascii_lowercase();
        let rp_id_lower = rp_id.to_ascii_lowercase();

        // WebAuthn requires the origin's effective domain to equal the RP ID or
        // be a subdomain of it; browsers reject anything else outright.
        if host != rp_id_lower && !host.ends_with(&format!(".{rp_id_lower}")) {
            anyhow::bail!(
                "WF_WEBAUTHN_ORIGIN host \"{host}\" must equal WF_WEBAUTHN_RP_ID \"{rp_id}\" \
                 or be a subdomain of it. Browsers reject any other combination."
            );
        }

        let is_localhost = host == "localhost" || host == "127.0.0.1" || host == "[::1]";
        if rp_origin.scheme() != "https" && !is_localhost {
            anyhow::bail!(
                "WF_WEBAUTHN_ORIGIN must use https (\"{origin_raw}\" does not). \
                 WebAuthn only permits plain http on localhost."
            );
        }

        let rp_name = rp_name.unwrap_or_else(|| "Wealthfolio".to_string());

        Ok(Some(Self {
            rp_id,
            rp_origin,
            rp_name,
        }))
    }
}

// ---------------------------------------------------------------------------
// Encrypted credential file
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    nonce: String,
    ciphertext: String,
}

#[derive(Serialize, Deserialize, Default)]
struct PasskeyFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    user_handle: Option<Uuid>,
    #[serde(default)]
    credentials: Vec<StoredCredential>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredCredential {
    pub id: Uuid,
    pub name: String,
    pub passkey: Passkey,
    pub created_at: String,
    #[serde(default)]
    pub last_used_at: Option<String>,
}

struct PasskeyStore {
    path: PathBuf,
    key: [u8; 32],
    lock: Mutex<()>,
}

impl PasskeyStore {
    fn new(path: PathBuf, key: [u8; 32]) -> Self {
        Self {
            path,
            key,
            lock: Mutex::new(()),
        }
    }

    fn read(&self) -> anyhow::Result<PasskeyFile> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Passkey store lock poisoned"))?;
        self.load_locked()
    }

    fn with_file<T, F>(&self, op: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut PasskeyFile) -> anyhow::Result<T>,
    {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Passkey store lock poisoned"))?;
        let mut file = self.load_locked()?;
        let result = op(&mut file)?;
        self.persist_locked(&file)?;
        Ok(result)
    }

    fn load_locked(&self) -> anyhow::Result<PasskeyFile> {
        if !self.path.exists() {
            return Ok(PasskeyFile::default());
        }
        let raw = fs::read(&self.path)?;
        if raw.is_empty() {
            return Ok(PasskeyFile::default());
        }
        let envelope: Envelope = serde_json::from_slice(&raw).map_err(|e| {
            anyhow::anyhow!("The passkey file at {} is malformed: {e}", self.path.display())
        })?;
        let nonce_bytes = BASE64URL.decode(&envelope.nonce)?;
        let ciphertext = BASE64URL.decode(&envelope.ciphertext)?;
        if nonce_bytes.len() != 12 {
            anyhow::bail!("Invalid nonce length in the passkey file");
        }
        let plaintext = ChaCha20Poly1305::new(Key::from_slice(&self.key))
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
            .map_err(|_| {
                anyhow::anyhow!(
                    "Cannot decrypt {}: it was sealed with a different WF_SECRET_KEY. \
                     Restore the original key, or delete the file to re-register passkeys.",
                    self.path.display()
                )
            })?;
        Ok(serde_json::from_slice(&plaintext)?)
    }

    fn persist_locked(&self, file: &PasskeyFile) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_vec(file)?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = ChaCha20Poly1305::new(Key::from_slice(&self.key))
            .encrypt(Nonce::from_slice(&nonce_bytes), serialized.as_ref())
            .map_err(|_| anyhow::anyhow!("Failed to encrypt the passkey file"))?;
        let envelope = Envelope {
            version: CURRENT_VERSION,
            nonce: BASE64URL.encode(nonce_bytes),
            ciphertext: BASE64URL.encode(ciphertext),
        };
        write_private(&self.path, serde_json::to_string_pretty(&envelope)?.as_bytes())
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

// ---------------------------------------------------------------------------
// Sealed ceremony state
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SealedTx<T> {
    exp: u64,
    state: T,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Public view model
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyInfo {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl From<&StoredCredential> for PasskeyInfo {
    fn from(credential: &StoredCredential) -> Self {
        Self {
            id: credential.id,
            name: credential.name.clone(),
            created_at: credential.created_at.clone(),
            last_used_at: credential.last_used_at.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PasskeyError {
    #[error("Passkeys are not configured on this server")]
    NotConfigured,
    #[error("{0}")]
    Invalid(String),
    #[error("This passkey was not recognised")]
    Rejected,
    #[error("No passkey matches that identifier")]
    NotFound,
    #[error("{0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct RegistrationTx {
    user_handle: Uuid,
    state: PasskeyRegistration,
}

pub struct PasskeyManager {
    webauthn: Webauthn,
    rp_name: String,
    store: PasskeyStore,
    seal_key: [u8; 32],
    has_credentials: std::sync::atomic::AtomicBool,
}

impl PasskeyManager {
    pub fn new(
        config: &WebauthnConfig,
        path: PathBuf,
        seal_key: [u8; 32],
    ) -> anyhow::Result<Self> {
        let webauthn = WebauthnBuilder::new(&config.rp_id, &config.rp_origin)
            .map_err(|e| anyhow::anyhow!("Invalid WebAuthn relying party configuration: {e:?}"))?
            .rp_name(&config.rp_name)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build the WebAuthn relying party: {e:?}"))?;
        let store = PasskeyStore::new(path, seal_key);
        // Fail at startup rather than at the first sign-in attempt.
        let existing = store
            .read()
            .map_err(|e| anyhow::anyhow!("Cannot open the passkey file: {e}"))?;
        Ok(Self {
            webauthn,
            rp_name: config.rp_name.clone(),
            seal_key,
            has_credentials: std::sync::atomic::AtomicBool::new(!existing.credentials.is_empty()),
            store,
        })
    }

    pub fn has_credentials(&self) -> bool {
        self.has_credentials
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn with_file<T, F>(&self, op: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut PasskeyFile) -> anyhow::Result<T>,
    {
        let mut remaining = 0usize;
        let result = self.store.with_file(|file| {
            let value = op(file)?;
            remaining = file.credentials.len();
            Ok(value)
        })?;
        // Only after the write actually landed, so a failed persist cannot
        // leave the flag claiming something the file does not say.
        self.has_credentials
            .store(remaining > 0, std::sync::atomic::Ordering::Relaxed);
        Ok(result)
    }

    pub fn list(&self) -> Result<Vec<PasskeyInfo>, PasskeyError> {
        let file = self.store.read().map_err(internal)?;
        Ok(file.credentials.iter().map(PasskeyInfo::from).collect())
    }

    // -- sealing ------------------------------------------------------------

    fn seal<T: Serialize>(&self, state: &T) -> Result<String, PasskeyError> {
        let payload = SealedTx {
            exp: now_secs().saturating_add(TX_TTL_SECS),
            state,
        };
        let plaintext = serde_json::to_vec(&payload).map_err(internal)?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = ChaCha20Poly1305::new(Key::from_slice(&self.seal_key))
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_ref())
            .map_err(|_| PasskeyError::Internal("Failed to seal ceremony state".into()))?;
        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(BASE64URL.encode(out))
    }

    fn open<T: DeserializeOwned>(&self, value: &str) -> Result<T, PasskeyError> {
        let raw = BASE64URL
            .decode(value)
            .map_err(|_| PasskeyError::Rejected)?;
        if raw.len() < 12 {
            return Err(PasskeyError::Rejected);
        }
        let (nonce_bytes, ciphertext) = raw.split_at(12);
        let plaintext = ChaCha20Poly1305::new(Key::from_slice(&self.seal_key))
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|_| PasskeyError::Rejected)?;
        let payload: SealedTx<T> =
            serde_json::from_slice(&plaintext).map_err(|_| PasskeyError::Rejected)?;
        if payload.exp <= now_secs() {
            return Err(PasskeyError::Invalid(
                "This sign-in attempt expired. Please try again.".into(),
            ));
        }
        Ok(payload.state)
    }

    // -- registration -------------------------------------------------------

    pub fn start_registration(
        &self,
    ) -> Result<(CreationChallengeResponse, String), PasskeyError> {
        let file = self.store.read().map_err(internal)?;
        if file.credentials.len() >= MAX_CREDENTIALS {
            return Err(PasskeyError::Invalid(format!(
                "This server already has {MAX_CREDENTIALS} passkeys registered. \
                 Remove one before adding another."
            )));
        }
        let user_handle = file.user_handle.unwrap_or_else(Uuid::new_v4);
        let exclude = file
            .credentials
            .iter()
            .map(|c| c.passkey.cred_id().clone())
            .collect::<Vec<_>>();

        let (mut options, state) = self
            .webauthn
            .start_passkey_registration(user_handle, "wealthfolio", &self.rp_name, Some(exclude))
            .map_err(|e| PasskeyError::Internal(format!("{e:?}")))?;

        // `start_passkey_registration` asks for `residentKey: discouraged`, so
        // credentials are discoverable only "on an opportunistic basis" (its
        // own words). Discoverable is exactly what this flow needs: sign-in
        // sends an empty `allowCredentials` and relies on the authenticator
        // finding the credential itself. Ask for it explicitly instead.
        if let Some(selection) = options.public_key.authenticator_selection.as_mut() {
            selection.resident_key = Some(ResidentKeyRequirement::Required);
            selection.require_resident_key = true;
        }

        let sealed = self.seal(&RegistrationTx {
            user_handle,
            state,
        })?;
        Ok((options, sealed))
    }

    pub fn finish_registration(
        &self,
        credential: &RegisterPublicKeyCredential,
        sealed: &str,
        name: &str,
    ) -> Result<PasskeyInfo, PasskeyError> {
        let name = normalize_name(name)?;
        let tx: RegistrationTx = self.open(sealed)?;

        // `credProps.rk` is the authenticator telling us whether it really
        // stored a discoverable credential. If it says no, this passkey would
        // register fine and then never appear at sign-in, which is a far worse
        // failure than refusing it here.
        if let Some(props) = credential.extensions.cred_props.as_ref() {
            if props.rk == Some(false) {
                return Err(PasskeyError::Invalid(
                    "This authenticator stored a non-discoverable credential, which cannot be \
                     used for passwordless sign-in. Try a device passkey (Touch ID, Face ID, \
                     Windows Hello) or a security key with resident-key support."
                        .into(),
                ));
            }
        }

        let passkey = self
            .webauthn
            .finish_passkey_registration(credential, &tx.state)
            .map_err(|e| {
                tracing::warn!(code = "PASSKEY_REGISTER_REJECTED", error = ?e);
                PasskeyError::Rejected
            })?;

        self
            .with_file(|file| {
                if file.credentials.len() >= MAX_CREDENTIALS {
                    anyhow::bail!("Too many passkeys are registered");
                }
                // A handle that moved on between start and finish means this
                // credential would authenticate as nobody. Refuse rather than
                // store something that can never sign in.
                match file.user_handle {
                    Some(existing) if existing != tx.user_handle => anyhow::bail!(
                        "The passkeys on this server changed while you were registering. \
                         Please try again."
                    ),
                    _ => {}
                }
                // The same authenticator re-registering must not create a
                // second row that shadows the first.
                if file
                    .credentials
                    .iter()
                    .any(|c| c.passkey.cred_id() == passkey.cred_id())
                {
                    anyhow::bail!("This passkey is already registered");
                }
                let record = StoredCredential {
                    id: Uuid::new_v4(),
                    name: name.clone(),
                    passkey: passkey.clone(),
                    created_at: timestamp(),
                    last_used_at: None,
                };
                let info = PasskeyInfo::from(&record);
                file.version = CURRENT_VERSION;
                file.user_handle = Some(tx.user_handle);
                file.credentials.push(record);
                Ok(info)
            })
            .map_err(|e| PasskeyError::Invalid(e.to_string()))
    }

    // -- authentication -----------------------------------------------------

    pub fn start_login(&self) -> Result<(RequestChallengeResponse, String), PasskeyError> {
        let (options, state) = self
            .webauthn
            .start_discoverable_authentication()
            .map_err(|e| PasskeyError::Internal(format!("{e:?}")))?;
        let sealed = self.seal(&state)?;
        Ok((options, sealed))
    }

    pub fn finish_login(
        &self,
        credential: &PublicKeyCredential,
        sealed: &str,
    ) -> Result<(), PasskeyError> {
        let state: DiscoverableAuthentication = self.open(sealed)?;

        let (claimed_handle, cred_id) = self
            .webauthn
            .identify_discoverable_authentication(credential)
            .map_err(|_| PasskeyError::Rejected)?;

        let file = self.store.read().map_err(internal)?;
        if file.user_handle != Some(claimed_handle) {
            return Err(PasskeyError::Rejected);
        }
        let matched = file
            .credentials
            .iter()
            .find(|c| c.passkey.cred_id().as_ref() == cred_id)
            .ok_or(PasskeyError::Rejected)?;
        let record_id = matched.id;

        let allowed = [DiscoverableKey::from(&matched.passkey)];
        let result = self
            .webauthn
            .finish_discoverable_authentication(credential, state, &allowed)
            .map_err(|e| {
                tracing::warn!(code = "PASSKEY_LOGIN_REJECTED", error = ?e);
                PasskeyError::Rejected
            })?;

        // Persist the signature counter and backup flags when the authenticator
        // moved them; skipping this is what makes cloned-credential detection
        // useless. Re-read under the write lock rather than reusing the copy
        // above, so a concurrent change is not clobbered.
        self
            .with_file(|file| {
                if let Some(record) = file.credentials.iter_mut().find(|c| c.id == record_id) {
                    record.passkey.update_credential(&result);
                    record.last_used_at = Some(timestamp());
                }
                Ok(())
            })
            .map_err(internal)?;

        Ok(())
    }

    // -- management ---------------------------------------------------------

    pub fn rename(&self, id: Uuid, name: &str) -> Result<PasskeyInfo, PasskeyError> {
        let name = normalize_name(name)?;
        self
            .with_file(|file| {
                let record = file
                    .credentials
                    .iter_mut()
                    .find(|c| c.id == id)
                    .ok_or_else(|| anyhow::anyhow!("not found"))?;
                record.name = name.clone();
                Ok(PasskeyInfo::from(&*record))
            })
            .map_err(|_| PasskeyError::NotFound)
    }

    pub fn remove(&self, id: Uuid) -> Result<(), PasskeyError> {
        self
            .with_file(|file| {
                let before = file.credentials.len();
                file.credentials.retain(|c| c.id != id);
                if file.credentials.len() == before {
                    anyhow::bail!("not found");
                }
                Ok(())
            })
            .map_err(|_| PasskeyError::NotFound)
    }
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn normalize_name(name: &str) -> Result<String, PasskeyError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(PasskeyError::Invalid("Give the passkey a name".into()));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(PasskeyError::Invalid(format!(
            "Passkey names are limited to {MAX_NAME_LEN} characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn internal<E: std::fmt::Display>(error: E) -> PasskeyError {
    PasskeyError::Internal(error.to_string())
}

// ---------------------------------------------------------------------------
// Cookie helpers
// ---------------------------------------------------------------------------

pub fn build_tx_cookie(name: &str, path: &str, value: &str, secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!("{name}={value}; HttpOnly; SameSite=Lax; Path={path}; Max-Age={TX_TTL_SECS}{secure_attr}")
}

pub fn clear_tx_cookie(name: &str, path: &str) -> String {
    format!("{name}=; HttpOnly; SameSite=Lax; Path={path}; Max-Age=0")
}

pub fn read_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let header = headers
        .get(axum::http::header::COOKIE)?
        .to_str()
        .ok()?;
    for pair in header.split(';') {
        if let Some((key, value)) = pair.trim().split_once('=') {
            if key.trim() == name {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

pub fn passkey_file_path(db_path: &str) -> PathBuf {
    Path::new(db_path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("passkeys.json")
}

#[cfg(test)]
mod tests;
