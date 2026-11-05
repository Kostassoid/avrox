//! Error reporting.

use thiserror::Error;

/// Result of an operation that can fail with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// An error encountered while parsing a schema or generating code.
#[derive(Debug, Error)]
pub enum Error {
    /// The schema is not valid JSON.
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// The schema is not a valid Avro schema.
    #[error("invalid schema at `{path}`: {message}")]
    Schema {
        /// Path to the offending part of the schema JSON.
        path: String,
        /// Description of the violation.
        message: String,
    },

    /// The schema is valid, but not supported by the generator.
    #[error("unsupported schema: {0}")]
    Unsupported(String),
}

pub(crate) fn schema_error<T>(path: &str, message: impl Into<String>) -> Result<T> {
    Err(Error::Schema {
        path: path.to_owned(),
        message: message.into(),
    })
}
