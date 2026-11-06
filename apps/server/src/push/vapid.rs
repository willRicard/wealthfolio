//! VAPID (RFC 8292) request authorization.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL, Engine as _};
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use serde::Serialize;

const TOKEN_TTL_SECS: i64 = 12 * 60 * 60;

#[derive(Serialize)]
struct JwtHeader {
    typ: &'static str,
    alg: &'static str,
}

#[derive(Serialize)]
struct JwtClaims<'a> {
    aud: &'a str,
    exp: i64,
    sub: &'a str,
}

pub(crate) fn audience_of(endpoint: &str) -> anyhow::Result<String> {
    let url =
        reqwest::Url::parse(endpoint).map_err(|e| anyhow::anyhow!("Invalid push endpoint: {e}"))?;
    anyhow::ensure!(
        url.scheme() == "https",
        "Push endpoints must use https, got {}",
        url.scheme()
    );
    Ok(url.origin().ascii_serialization())
}

pub(crate) fn authorization_header(
    private_key: &[u8; 32],
    public_key: &[u8],
    audience: &str,
    subject: &str,
    issued_at_unix: i64,
) -> anyhow::Result<String> {
    let signing_key = SigningKey::from_slice(private_key)
        .map_err(|_| anyhow::anyhow!("WF_VAPID_PRIVATE_KEY is not a valid P-256 scalar"))?;

    let header = BASE64URL.encode(serde_json::to_vec(&JwtHeader {
        typ: "JWT",
        alg: "ES256",
    })?);
    let claims = BASE64URL.encode(serde_json::to_vec(&JwtClaims {
        aud: audience,
        exp: issued_at_unix.saturating_add(TOKEN_TTL_SECS),
        sub: subject,
    })?);

    let signing_input = format!("{header}.{claims}");
    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    let token = format!("{signing_input}.{}", BASE64URL.encode(signature.to_bytes()));

    Ok(format!(
        "vapid t={token}, k={}",
        BASE64URL.encode(public_key)
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

    use super::*;
    use p256::ecdsa::{signature::Verifier, VerifyingKey};

    const PRIVATE: [u8; 32] = [
        0x1f, 0x2e, 0x3d, 0x4c, 0x5b, 0x6a, 0x79, 0x88, 0x97, 0xa6, 0xb5, 0xc4, 0xd3, 0xe2, 0xf1,
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff, 0x01,
    ];

    fn public_bytes() -> Vec<u8> {
        use p256::elliptic_curve::sec1::ToEncodedPoint;
        let signing = SigningKey::from_slice(&PRIVATE).unwrap();
        VerifyingKey::from(&signing)
            .as_affine()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec()
    }

    #[test]
    fn audience_is_the_endpoint_origin_only() {
        assert_eq!(
            audience_of("https://fcm.googleapis.com/fcm/send/abc123?x=1").unwrap(),
            "https://fcm.googleapis.com"
        );
        assert_eq!(
            audience_of("https://push.example.com:8443/path").unwrap(),
            "https://push.example.com:8443"
        );
    }

    #[test]
    fn plain_http_endpoints_are_refused() {
        assert!(audience_of("http://push.example.com/x").is_err());
        assert!(audience_of("not a url").is_err());
    }

    #[test]
    fn the_header_carries_a_verifiable_es256_token() {
        let public = public_bytes();
        let header = authorization_header(
            &PRIVATE,
            &public,
            "https://push.example.com",
            "mailto:admin@example.com",
            1_762_000_000,
        )
        .unwrap();

        let token = header
            .strip_prefix("vapid t=")
            .and_then(|rest| rest.split_once(", k="))
            .expect("header shape is `vapid t=<jwt>, k=<key>`");
        let (jwt, key_part) = token;
        assert_eq!(key_part, BASE64URL.encode(&public));

        let mut parts = jwt.split('.');
        let header_b64 = parts.next().unwrap();
        let claims_b64 = parts.next().unwrap();
        let signature_b64 = parts.next().unwrap();
        assert!(parts.next().is_none());

        let signature = Signature::from_slice(&BASE64URL.decode(signature_b64).unwrap()).unwrap();
        let signing_key = SigningKey::from_slice(&PRIVATE).unwrap();
        VerifyingKey::from(&signing_key)
            .verify(format!("{header_b64}.{claims_b64}").as_bytes(), &signature)
            .expect("token verifies against the VAPID public key");

        let header_json: serde_json::Value =
            serde_json::from_slice(&BASE64URL.decode(header_b64).unwrap()).unwrap();
        assert_eq!(header_json["alg"], "ES256");
        assert_eq!(header_json["typ"], "JWT");

        let claims: serde_json::Value =
            serde_json::from_slice(&BASE64URL.decode(claims_b64).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://push.example.com");
        assert_eq!(claims["sub"], "mailto:admin@example.com");
        assert_eq!(claims["exp"], 1_762_000_000 + TOKEN_TTL_SECS);
    }

    #[test]
    fn expiry_stays_within_the_rfc_limit_of_24_hours() {
        const { assert!(TOKEN_TTL_SECS <= 24 * 60 * 60) };
    }
}
