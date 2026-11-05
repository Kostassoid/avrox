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

    /// The schema is valid, but uses constructs the generator does not
    /// support for the target language.
    #[error("unsupported schema: {0}")]
    Unsupported(String),

    /// The generator produced something unexpected; this indicates a bug in
    /// `avrox` itself.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Builds a schema violation error at `path`.
pub(crate) fn invalid_schema(path: &str, message: impl Into<String>) -> Error {
    Error::Schema {
        path: path.to_owned(),
        message: message.into(),
    }
}

/// Fails with a schema violation error at `path`.
pub(crate) fn schema_error<T>(path: &str, message: impl Into<String>) -> Result<T> {
    Err(invalid_schema(path, message))
}
