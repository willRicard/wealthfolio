use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcurementError {
    #[error("Invalid procurement input: {0}")]
    InvalidInput(String),
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },
}
