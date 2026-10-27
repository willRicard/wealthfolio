use super::*;

const KEY: [u8; 32] = [7u8; 32];

fn config() -> WebauthnConfig {
    WebauthnConfig::resolve(Some("folio.example.com".into()), None, None)
        .unwrap()
        .unwrap()
}

fn manager(dir: &std::path::Path) -> PasskeyManager {
    PasskeyManager::new(&config(), dir.join("passkeys.json"), KEY).unwrap()
}

// -- configuration ----------------------------------------------------------

#[test]
fn absent_rp_id_leaves_passkeys_disabled() {
    assert!(WebauthnConfig::resolve(None, None, None).unwrap().is_none());
    // An origin alone is not enough to turn the feature on.
    assert!(
        WebauthnConfig::resolve(None, Some("https://folio.example.com".into()), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn origin_defaults_to_https_on_the_rp_id() {
    let config = config();
    assert_eq!(config.rp_origin.as_str(), "https://folio.example.com/");
    assert_eq!(config.rp_name, "Wealthfolio");
}

#[test]
fn subdomain_origins_are_accepted_and_foreign_ones_are_not() {
    assert!(WebauthnConfig::resolve(
        Some("example.com".into()),
        Some("https://folio.example.com".into()),
        None,
    )
    .unwrap()
    .is_some());

    // The classic footgun: a suffix match that is not a subdomain.
    let error = WebauthnConfig::resolve(
        Some("example.com".into()),
        Some("https://notexample.com".into()),
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("must equal"), "{error}");
}

#[test]
fn plain_http_is_rejected_except_on_localhost() {
    let error = WebauthnConfig::resolve(
        Some("folio.example.com".into()),
        Some("http://folio.example.com".into()),
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("https"), "{error}");

    assert!(
        WebauthnConfig::resolve(
            Some("localhost".into()),
            Some("http://localhost:8088".into()),
            None,
        )
        .unwrap()
        .is_some(),
        "local development over http must keep working"
    );
}

#[test]
fn an_rp_id_that_is_really_a_url_is_rejected() {
    for value in [
        "https://folio.example.com",
        "folio.example.com:8088",
        "folio.example.com/app",
    ] {
        let error = WebauthnConfig::resolve(Some(value.into()), None, None).unwrap_err();
        assert!(error.to_string().contains("bare domain"), "{value}: {error}");
    }
}

// -- credential file --------------------------------------------------------

#[test]
fn a_missing_file_reads_as_empty_rather_than_failing() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    assert!(manager.list().unwrap().is_empty());
    assert!(!manager.has_credentials());
    // Reading must not create the file; nothing is written until registration.
    assert!(!dir.path().join("passkeys.json").exists());
}

#[test]
fn the_file_is_encrypted_at_rest_and_needs_the_right_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("passkeys.json");
    let store = PasskeyStore::new(path.clone(), KEY);
    store
        .with_file(|file| {
            file.user_handle = Some(Uuid::nil());
            Ok(())
        })
        .unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("ciphertext"), "the file must be sealed");
    assert!(
        !raw.contains("user_handle"),
        "plaintext field names must not leak"
    );

    assert_eq!(store.read().unwrap().user_handle, Some(Uuid::nil()));

    // Deliberately not `unwrap_err`: `PasskeyFile` holds credentials and is
    // kept un-`Debug` so it cannot be printed into a log or panic message.
    let wrong = PasskeyStore::new(path, [9u8; 32]);
    let error = match wrong.read() {
        Ok(_) => panic!("a file sealed with another key must not open"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("different WF_SECRET_KEY"), "{error}");
}

#[test]
fn a_write_replaces_the_file_without_leaving_a_temporary_behind() {
    let dir = tempfile::tempdir().unwrap();
    let store = PasskeyStore::new(dir.path().join("passkeys.json"), KEY);
    store.with_file(|_| Ok(())).unwrap();
    store.with_file(|_| Ok(())).unwrap();
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["passkeys.json".to_string()]);
}

// -- sealed ceremony state --------------------------------------------------

#[test]
fn sealed_state_round_trips_and_rejects_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    let (_options, sealed) = manager.start_login().unwrap();

    // The blob must not be readable by the client that carries it.
    assert!(!sealed.contains("challenge"));
    manager.open::<DiscoverableAuthentication>(&sealed).unwrap();

    for broken in [
        String::new(),
        "not-base64!".to_string(),
        format!("{sealed}x"),
        sealed.chars().rev().collect::<String>(),
    ] {
        assert!(
            matches!(
                manager.open::<DiscoverableAuthentication>(&broken),
                Err(PasskeyError::Rejected)
            ),
            "tampered state must be rejected, not accepted or 500: {broken}"
        );
    }
}

#[test]
fn expired_state_is_rejected_even_when_the_cookie_is_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());

    // Seal with an expiry already in the past: the client controls the
    // cookie's Max-Age, so the deadline has to live inside the blob.
    let payload = SealedTx {
        exp: now_secs() - 1,
        state: "state",
    };
    let plaintext = serde_json::to_vec(&payload).unwrap();
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = ChaCha20Poly1305::new(Key::from_slice(&KEY))
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .unwrap();
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ciphertext);
    let sealed = BASE64URL.encode(out);

    let error = manager.open::<String>(&sealed).unwrap_err();
    assert!(matches!(error, PasskeyError::Invalid(_)), "{error}");
}

// -- login ceremony ---------------------------------------------------------

#[test]
fn login_options_look_identical_with_and_without_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());

    let (empty, _) = manager.start_login().unwrap();
    let empty = serde_json::to_value(&empty).unwrap();

    // Registering is driven by the authenticator, so simulate enrolment by
    // giving the file a user handle — enough to change internal state.
    manager
        .store
        .with_file(|file| {
            file.user_handle = Some(Uuid::new_v4());
            Ok(())
        })
        .unwrap();
    let (enrolled, _) = manager.start_login().unwrap();
    let enrolled = serde_json::to_value(&enrolled).unwrap();

    // Challenges differ by design; everything else must match, or an anonymous
    // caller could tell whether the server has any passkeys enrolled.
    let strip = |mut value: serde_json::Value| {
        value["publicKey"]["challenge"] = serde_json::Value::Null;
        value
    };
    assert_eq!(strip(empty), strip(enrolled));
}

#[test]
fn login_requests_no_specific_credentials_so_autofill_can_work() {
    let dir = tempfile::tempdir().unwrap();
    let (options, _) = manager(dir.path()).start_login().unwrap();
    let json = serde_json::to_value(&options).unwrap();
    let allow = &json["publicKey"]["allowCredentials"];
    assert!(
        allow.is_null() || allow.as_array().is_some_and(|a| a.is_empty()),
        "discoverable login must not pin allowCredentials: {allow}"
    );
    assert_eq!(json["publicKey"]["userVerification"], "required");
}

#[test]
fn login_rejects_state_that_was_sealed_for_registration() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    let (_options, registration) = manager.start_registration().unwrap();
    assert!(
        manager
            .open::<DiscoverableAuthentication>(&registration)
            .is_err(),
        "a registration blob must not open as an authentication state"
    );
}

// -- registration ceremony --------------------------------------------------

#[test]
fn registration_demands_a_discoverable_credential() {
    let dir = tempfile::tempdir().unwrap();
    let (options, _) = manager(dir.path()).start_registration().unwrap();
    let json = serde_json::to_value(&options).unwrap();
    let selection = &json["publicKey"]["authenticatorSelection"];

    // webauthn-rs asks for `discouraged` by default, which would leave the
    // credential undiscoverable and silently unusable at sign-in.
    assert_eq!(selection["residentKey"], "required");
    assert_eq!(selection["requireResidentKey"], true);
    assert_eq!(selection["userVerification"], "required");
}

#[test]
fn registration_carries_a_stable_user_handle() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    let handle = Uuid::new_v4();
    manager
        .store
        .with_file(|file| {
            file.user_handle = Some(handle);
            Ok(())
        })
        .unwrap();

    // Every later registration must reuse the stored handle, or the new
    // credential would authenticate as a different user and be rejected.
    for _ in 0..2 {
        let (options, _) = manager.start_registration().unwrap();
        let json = serde_json::to_value(&options).unwrap();
        let encoded = json["publicKey"]["user"]["id"].as_str().unwrap().to_string();
        let bytes = BASE64URL.decode(&encoded).unwrap();
        assert_eq!(Uuid::from_slice(&bytes).unwrap(), handle);
    }
}

#[test]
fn a_rejected_assertion_never_reports_why() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    let (_options, sealed) = manager.start_login().unwrap();
    let credential: PublicKeyCredential = serde_json::from_value(serde_json::json!({
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
    }))
    .unwrap();

    let error = manager.finish_login(&credential, &sealed).unwrap_err();
    assert!(matches!(error, PasskeyError::Rejected), "{error}");
    assert_eq!(error.to_string(), "This passkey was not recognised");
}

// -- management -------------------------------------------------------------

#[test]
fn the_enrolment_flag_tracks_the_file_without_reading_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("passkeys.json");

    let manager = PasskeyManager::new(&config(), path.clone(), KEY).unwrap();
    assert!(!manager.has_credentials());

    // Writes go through the manager, which refreshes the cached flag.
    let id = Uuid::new_v4();
    manager
        .with_file(|file| {
            file.credentials.push(StoredCredential {
                id,
                name: "Test".into(),
                passkey: sample_passkey(),
                created_at: timestamp(),
                last_used_at: None,
            });
            Ok(())
        })
        .unwrap();
    assert!(manager.has_credentials());

    manager.remove(id).unwrap();
    assert!(!manager.has_credentials());

    // A fresh manager must agree, i.e. the flag reflects what was persisted.
    manager
        .with_file(|file| {
            file.credentials.push(StoredCredential {
                id: Uuid::new_v4(),
                name: "Test".into(),
                passkey: sample_passkey(),
                created_at: timestamp(),
                last_used_at: None,
            });
            Ok(())
        })
        .unwrap();
    assert!(PasskeyManager::new(&config(), path, KEY)
        .unwrap()
        .has_credentials());
}

#[test]
fn renaming_and_removing_an_unknown_passkey_reports_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    let missing = Uuid::new_v4();
    assert!(matches!(
        manager.rename(missing, "Phone"),
        Err(PasskeyError::NotFound)
    ));
    assert!(matches!(
        manager.remove(missing),
        Err(PasskeyError::NotFound)
    ));
}

#[test]
fn names_are_trimmed_and_bounded() {
    assert_eq!(normalize_name("  iPhone  ").unwrap(), "iPhone");
    assert!(normalize_name("   ").is_err());
    assert!(normalize_name(&"a".repeat(MAX_NAME_LEN)).is_ok());
    assert!(normalize_name(&"a".repeat(MAX_NAME_LEN + 1)).is_err());
}

// -- paths and cookies ------------------------------------------------------

#[test]
fn the_passkey_file_sits_beside_the_database() {
    assert_eq!(
        passkey_file_path("/data/app.db"),
        std::path::Path::new("/data/passkeys.json")
    );
    // A bare filename means the working directory, not the filesystem root.
    assert_eq!(
        passkey_file_path("app.db"),
        std::path::Path::new("./passkeys.json")
    );
}

#[test]
fn tx_cookies_are_scoped_to_their_own_ceremony() {
    let login = build_tx_cookie(LOGIN_TX_COOKIE, LOGIN_TX_PATH, "abc", true);
    assert!(login.contains("HttpOnly"));
    assert!(login.contains("SameSite=Lax"));
    assert!(login.contains("Secure"));
    assert!(login.contains("Path=/api/v1/auth/passkey"));
    assert!(login.contains("Max-Age=300"));

    // Without TLS the Secure attribute must be absent, or the browser drops
    // the cookie and local development breaks.
    assert!(!build_tx_cookie(LOGIN_TX_COOKIE, LOGIN_TX_PATH, "abc", false).contains("Secure"));

    // The two ceremonies must not share a cookie.
    assert_ne!(LOGIN_TX_COOKIE, REGISTER_TX_COOKIE);
    assert_ne!(LOGIN_TX_PATH, REGISTER_TX_PATH);

    assert!(clear_tx_cookie(LOGIN_TX_COOKIE, LOGIN_TX_PATH).contains("Max-Age=0"));
}

#[test]
fn cookies_are_read_by_exact_name() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "other=1; wf_pk_login=sealed; wf_session=jwt".parse().unwrap(),
    );
    assert_eq!(
        read_cookie(&headers, LOGIN_TX_COOKIE),
        Some("sealed".to_string())
    );
    assert_eq!(read_cookie(&headers, REGISTER_TX_COOKIE), None);

    // An empty value is as good as absent.
    let mut empty = axum::http::HeaderMap::new();
    empty.insert(
        axum::http::header::COOKIE,
        "wf_pk_login=".parse().unwrap(),
    );
    assert_eq!(read_cookie(&empty, LOGIN_TX_COOKIE), None);
}
