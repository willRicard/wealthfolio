//! Private S3-compatible object storage, including OVHCloud.

use crate::{BlobKey, BlobStore, FileStoreError, Result, S3_BACKEND_ID};
use async_trait::async_trait;
use object_store::{
    aws::{AmazonS3, AmazonS3Builder, S3ConditionalPut},
    path::Path,
    Attribute, Attributes, ObjectStore, PutMode, PutOptions, RetryConfig,
};

pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
}

impl S3Config {
    pub fn from_env() -> Result<Self> {
        let required = |name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| crate::error::invalid(format!("{name} is required for S3 storage")))
        };
        let endpoint = required("WF_S3_URL")?;
        let region = std::env::var("WF_S3_REGION")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                let url = url::Url::parse(&endpoint).ok()?;
                url.host_str()?
                    .strip_prefix("s3.")?
                    .strip_suffix(".io.cloud.ovh.net")
                    .map(str::to_owned)
            })
            .ok_or_else(|| crate::error::invalid("WF_S3_REGION is required for this endpoint"))?;
        Ok(Self {
            endpoint,
            region,
            bucket: required("WF_S3_BUCKET")?,
            access_key: required("WF_S3_ACCESS")?,
            secret_key: required("WF_S3_SECRET")?,
        })
    }
}

pub struct S3BlobStore {
    store: AmazonS3,
}

fn backend_error() -> FileStoreError {
    FileStoreError::Backend(anyhow::anyhow!("S3 storage request failed"))
}

fn map_error(error: object_store::Error) -> FileStoreError {
    match error {
        object_store::Error::NotFound { .. } => FileStoreError::NotFound("Stored file".into()),
        object_store::Error::AlreadyExists { .. } | object_store::Error::Precondition { .. } => {
            FileStoreError::AlreadyExists("Storage key already in use".into())
        }
        _ => backend_error(),
    }
}

impl S3BlobStore {
    pub fn new(config: S3Config) -> Result<Self> {
        let url = url::Url::parse(&config.endpoint)
            .map_err(|_| crate::error::invalid("Invalid WF_S3_URL"))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
        {
            return Err(crate::error::invalid("WF_S3_URL must be an HTTPS endpoint"));
        }
        let store = AmazonS3Builder::new()
            .with_endpoint(config.endpoint)
            .with_bucket_name(config.bucket)
            .with_region(config.region)
            .with_access_key_id(config.access_key)
            .with_secret_access_key(config.secret_key)
            .with_virtual_hosted_style_request(false)
            .with_conditional_put(S3ConditionalPut::ETagMatch)
            .with_retry(RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .build()
            .map_err(|_| crate::error::invalid("Invalid S3 storage configuration"))?;
        Ok(Self { store })
    }
}

#[async_trait]
impl BlobStore for S3BlobStore {
    fn backend_id(&self) -> &'static str {
        S3_BACKEND_ID
    }

    async fn put(&self, key: &BlobKey, bytes: Vec<u8>, content_type: &str) -> Result<()> {
        let mut attributes = Attributes::new();
        attributes.insert(Attribute::ContentType, content_type.to_owned().into());
        self.store
            .put_opts(
                &Path::from(key.as_str()),
                bytes.into(),
                PutOptions {
                    mode: PutMode::Create,
                    attributes,
                    ..Default::default()
                },
            )
            .await
            .map_err(map_error)?;
        Ok(())
    }

    async fn get(&self, key: &BlobKey) -> Result<Vec<u8>> {
        let result = self
            .store
            .get(&Path::from(key.as_str()))
            .await
            .map_err(map_error)?;
        Ok(result.bytes().await.map_err(map_error)?.to_vec())
    }

    async fn delete(&self, key: &BlobKey) -> Result<()> {
        match self.store.delete(&Path::from(key.as_str())).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(map_error(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[tokio::test]
    #[ignore = "writes and deletes a temporary object in the configured S3 bucket"]
    async fn live_s3_round_trip() {
        dotenvy::dotenv().ok();
        let store = S3BlobStore::new(S3Config::from_env().unwrap()).unwrap();
        let key = BlobKey::mint(uuid::Uuid::new_v4(), "storagecheck").unwrap();
        store
            .put(&key, b"storage check".to_vec(), "text/plain")
            .await
            .unwrap();
        let read = store.get(&key).await;
        let collision = store.put(&key, b"overwrite".to_vec(), "text/plain").await;
        let removed = store.delete(&key).await;
        assert_eq!(read.unwrap(), b"storage check");
        assert!(matches!(collision, Err(FileStoreError::AlreadyExists(_))));
        removed.unwrap();
        store.delete(&key).await.unwrap();
        assert!(matches!(
            store.get(&key).await,
            Err(FileStoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn signed_requests_preserve_content_type_no_clobber_and_idempotent_delete() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (method, status, body) in [
                ("PUT", "200 OK", ""),
                ("GET", "200 OK", "one"),
                ("PUT", "412 Precondition Failed", ""),
                ("DELETE", "204 No Content", ""),
                ("DELETE", "404 Not Found", ""),
                ("GET", "404 Not Found", ""),
                ("GET", "403 Forbidden", "private provider response"),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let header_end = loop {
                    let mut buffer = [0; 4096];
                    let count = stream.read(&mut buffer).unwrap();
                    assert_ne!(count, 0);
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
                assert!(headers.starts_with(&format!(
                    "{} /test-bucket/ab/test.original ",
                    method.to_ascii_lowercase()
                )));
                assert!(headers.contains("authorization: aws4-hmac-sha256 "));
                assert!(headers.contains("/eu-west-par/s3/aws4_request"));
                assert!(!headers.contains("x-amz-acl:"));
                if method == "PUT" {
                    assert!(headers.contains("content-type: image/webp\r\n"));
                    assert!(headers.contains("if-none-match: *\r\n"));
                    while request.len() < header_end + 3 {
                        let mut buffer = [0; 4096];
                        let count = stream.read(&mut buffer).unwrap();
                        assert_ne!(count, 0);
                        request.extend_from_slice(&buffer[..count]);
                    }
                }
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nETag: \"test\"\r\nLast-Modified: Wed, 01 Jan 2025 00:00:00 GMT\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let store = S3BlobStore {
            store: AmazonS3Builder::new()
                .with_endpoint(endpoint)
                .with_allow_http(true)
                .with_bucket_name("test-bucket")
                .with_region("eu-west-par")
                .with_access_key_id("test-access")
                .with_secret_access_key("test-secret")
                .with_conditional_put(S3ConditionalPut::ETagMatch)
                .with_retry(RetryConfig {
                    max_retries: 0,
                    ..Default::default()
                })
                .build()
                .unwrap(),
        };
        let key = BlobKey::parse("ab/test.original").unwrap();
        store
            .put(&key, b"one".to_vec(), "image/webp")
            .await
            .unwrap();
        assert_eq!(store.get(&key).await.unwrap(), b"one");
        assert!(matches!(
            store.put(&key, b"two".to_vec(), "image/webp").await,
            Err(FileStoreError::AlreadyExists(_))
        ));
        store.delete(&key).await.unwrap();
        store.delete(&key).await.unwrap();
        assert!(matches!(
            store.get(&key).await,
            Err(FileStoreError::NotFound(_))
        ));
        let error = store.get(&key).await.unwrap_err();
        assert_eq!(error.to_string(), "S3 storage request failed");
        server.join().unwrap();
    }
}
