//! Wealthfolio File Store
//!
//! Owner-agnostic storage for user-supplied files and their generated previews.

mod error;
mod key;
mod local;
mod store;

#[cfg(feature = "thumbnails")]
mod service;
#[cfg(feature = "thumbnails")]
mod thumbnail;

pub use error::{FileStoreError, Result};
pub use key::BlobKey;
pub use local::LocalBlobStore;
pub use store::BlobStore;

#[cfg(feature = "thumbnails")]
pub use service::{validate_filename, StoreRequest, StoredFile, StoredFileRef, StoredFileService};
