//! Code generation from Avro schemas.
//!
//! [`generate`] parses an Avro schema in JSON format (`.avsc`) and produces
//! strongly typed code with the serialization boilerplate for the target
//! language, in the same vein as `protoc` for protocol buffers.
//!
//! Generated Rust code integrates with the
//! [`apache-avro`](https://docs.rs/apache-avro) crate through `serde`
//! derives: records become structs, enums become enums, fixed types become
//! `serde_bytes::ByteArray` aliases, `bytes` becomes `serde_bytes::ByteBuf`,
//! nullable unions become `Option`s and other unions become generated enums.
//! Logical types are represented by their underlying primitive types and
//! documented accordingly. The consuming crate needs `serde` (with `derive`)
//! and `serde_bytes` as dependencies.
//!
//! # Example
//!
//! ```
//! let schema = r#"{
//!     "type": "record",
//!     "name": "User",
//!     "fields": [
//!         {"name": "id", "type": "long"},
//!         {"name": "name", "type": "string"}
//!     ]
//! }"#;
//!
//! let files = avrox::generate(schema, &avrox::Settings::default())?;
//! assert_eq!(files.len(), 1);
//! assert_eq!(files[0].path, "user.rs");
//! # Ok::<(), avrox::Error>(())
//! ```

mod codegen;
pub mod error;
pub mod schema;

pub use error::{Error, Result};
pub use schema::Document;

/// Settings for code generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Target language of the generated code.
    pub language: Language,
    /// Avro specification version the schema is written against.
    pub avro_version: AvroVersion,
    /// How the generated code is split into files.
    pub layout: Layout,
}

impl Default for Settings {
    /// Defaults to Rust, the latest supported Avro version and a single
    /// output file.
    fn default() -> Self {
        Self {
            language: Language::Rust,
            avro_version: AvroVersion::V1_12,
            layout: Layout::SingleFile,
        }
    }
}

/// Target language for generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Language {
    /// Rust, integrating with `serde` and the `apache-avro` crate.
    Rust,
}

/// How generated code is split into files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Layout {
    /// Everything in a single file, named after the root type of the schema.
    SingleFile,
    /// One file per named type, plus a `mod.rs` that declares them, re-exports
    /// them and holds the generated union enums. For Rust the result can be
    /// used as a module directory directly.
    Module,
}

/// Avro specification version a schema is written against.
///
/// Version-specific features (e.g. `timestamp-nanos` and `big-decimal`,
/// available since 1.12) are only recognized in the versions that support
/// them; elsewhere they are ignored and the underlying type is used, as
/// required by the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum AvroVersion {
    /// Avro 1.10.
    V1_10,
    /// Avro 1.11.
    V1_11,
    /// Avro 1.12.
    V1_12,
}

/// A file produced by the generator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    /// Path of the file, relative to the output directory.
    pub path: String,
    /// Contents of the file.
    pub contents: String,
}

/// Generates code from an Avro schema in JSON format (`.avsc`).
///
/// # Errors
///
/// Returns an error if the schema is not valid JSON, if it violates the Avro
/// specification, or if it uses constructs the generator does not support for
/// the configured target language.
pub fn generate(schema: &str, settings: &Settings) -> Result<Vec<GeneratedFile>> {
    let document = schema::parse(schema, settings.avro_version)?;
    codegen::generate(&document, settings)
}
