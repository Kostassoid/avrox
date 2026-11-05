//! Target language code generation.

mod rust;

use crate::schema::Document;
use crate::{GeneratedFile, Language, Result, Settings};

/// Generates code for the document in the configured target language.
pub(crate) fn generate(document: &Document, settings: &Settings) -> Result<Vec<GeneratedFile>> {
    match settings.language {
        Language::Rust => rust::generate(document, settings.layout, &settings.rust),
    }
}
