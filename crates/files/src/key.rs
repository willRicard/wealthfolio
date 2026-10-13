//! Backend-neutral object names.

use crate::error::{invalid, Result};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobKey(String);

impl BlobKey {
    pub fn parse(value: &str) -> Result<Self> {
        if value.is_empty() || value.len() > 255 {
            return Err(invalid("Invalid storage key"));
        }
        if value.starts_with('/') || value.ends_with('/') {
            return Err(invalid("Invalid storage key"));
        }
        for segment in value.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." {
                return Err(invalid("Invalid storage key"));
            }
        }
        let allowed = value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'/' | b'.' | b'_' | b'-')
        });
        if !allowed {
            return Err(invalid("Invalid storage key"));
        }
        Ok(Self(value.to_owned()))
    }

    /// Mints `"{first two hex digits}/{uuid}.{extension}"`.
    pub fn mint(id: Uuid, extension: &str) -> Result<Self> {
        if extension.is_empty()
            || !extension
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(invalid("Invalid storage key extension"));
        }
        let id = id.simple().to_string();
        let prefix = id.get(..2).ok_or_else(|| invalid("Invalid storage key"))?;
        Self::parse(&format!("{prefix}/{id}.{extension}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_and_shape_are_rejected() {
        for key in [
            "",
            "/leading",
            "trailing/",
            "a//b",
            "../escape",
            "a/../b",
            "a/./b",
            "UPPER",
            "space here",
            "back\\slash",
            "null\0byte",
        ] {
            assert!(
                BlobKey::parse(key).is_err(),
                "expected {key:?} to be rejected"
            );
        }
        assert!(BlobKey::parse(&"a".repeat(256)).is_err());
    }

    #[test]
    fn minted_keys_shard_by_prefix_and_carry_no_owner() {
        let id = Uuid::new_v4();
        let key = BlobKey::mint(id, "original").expect("minting a key from a UUID cannot fail");
        let simple = id.simple().to_string();
        assert_eq!(
            key.as_str(),
            format!("{}/{}.original", &simple[..2], simple)
        );
    }

    #[test]
    fn minted_extensions_are_validated() {
        let id = Uuid::new_v4();
        assert!(BlobKey::mint(id, "").is_err());
        assert!(BlobKey::mint(id, "../x").is_err());
        assert!(BlobKey::mint(id, "WEBP").is_err());
    }
}
