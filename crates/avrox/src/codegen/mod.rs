//! Target language code generation.

mod go;
mod rust;

use crate::schema::Schema;
use crate::{GeneratedFile, Language, Result, Settings};

/// Generates code for the schema in the configured target language.
pub(crate) fn generate(schema: &Schema, settings: &Settings) -> Result<Vec<GeneratedFile>> {
    match settings.language {
        Language::Rust => rust::generate(schema, settings),
        Language::Go => go::generate(schema, settings),
    }
}
