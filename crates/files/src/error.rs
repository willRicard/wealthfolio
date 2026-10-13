//! Error type for the file store.

#[derive(Debug, thiserror::Error)]
pub enum FileStoreError {
    /// The request itself is wrong: a malformed key, an unsupported content
    /// type, bytes that do not match their declared type.
    #[error("{0}")]
    InvalidInput(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    AlreadyExists(String),

    /// The storage backend failed: I/O, permissions, a network object store.
    #[error(transparent)]
    Backend(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, FileStoreError>;

impl From<std::io::Error> for FileStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Backend(error.into())
    }
}

impl From<tokio::task::JoinError> for FileStoreError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Backend(error.into())
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> FileStoreError {
    FileStoreError::InvalidInput(message.into())
}
