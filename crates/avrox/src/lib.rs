//! Code generation from Avro schemas.

pub mod error;

pub use crate::error::{Error, Result};

/// Generates code from the given Avro `schema`.
///
/// # Errors
///
/// Returns an [`Error`] if the schema cannot be processed.
pub fn generate(_schema: &str) -> Result<String> {
    Err(Error::Unimplemented)
}
