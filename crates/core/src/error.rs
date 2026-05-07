use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("missing variable: {0}")]
    MissingVar(String),
    #[error("invalid template: {0}")]
    InvalidTemplate(String),
}
