//! Wealthfolio File Store
//!
//! Owner-agnostic storage for user-supplied files and their generated previews.

mod error;
mod key;
mod local;
mod s3;
mod store;

#[cfg(feature = "thumbnails")]
mod service;
#[cfg(feature = "thumbnails")]
mod thumbnail;

pub use error::{FileStoreError, Result};
pub use key::BlobKey;
pub use local::LocalBlobStore;
pub use s3::{S3BlobStore, S3Config};
pub use store::{BlobStore, LOCAL_BACKEND_ID, S3_BACKEND_ID};

#[cfg(feature = "thumbnails")]
pub use service::{
    stored_filename, validate_filename, StoreRequest, StoredFile, StoredFileRef, StoredFileService,
};
