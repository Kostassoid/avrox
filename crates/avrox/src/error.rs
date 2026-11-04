//! Error reporting.

use thiserror::Error;

/// Result of an operation that can fail with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// An error encountered while generating code.
#[derive(Debug, Error)]
pub enum Error {
    /// Reading or writing a file failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The requested functionality is not implemented yet.
    #[error("not implemented yet")]
    Unimplemented,
}
