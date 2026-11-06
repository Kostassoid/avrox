//! Code generation from Avro schemas.
//!
//! [`generate`] parses an Avro schema in JSON format (`.avsc`) and produces
//! strongly typed code with the serialization boilerplate for the target
//! language, in the same vein as `protoc` for protocol buffers.
//!
//! Generated Rust code integrates with the
//! [`apache-avro`](https://docs.rs/apache-avro) crate through `serde`
//! derives: records become structs, enums become enums, nullable unions
//! become `Option`s and other unions become generated enums. Logical types
//! are documented on the fields and types carrying them.
//!
//! [`RustTypes`] controls which types the output uses: [`RustTypes::ApacheAvro`]
//! (the default) relies on `apache-avro` and `serde_bytes` types, while
//! [`RustTypes::Std`] uses only standard library types plus types generated
//! into the output. Both round-trip through `apache-avro`'s binary codec.
//!
//! [`RustTime`] controls how time-related logical types are represented:
//! underlying integers (the default), [`std::time`] newtypes or bare `chrono`
//! types with `serde` adapters.
//!
//! [`RustOptions::derives`] adds derives to generated records, enums and union
//! enums. Common derives can be given by their short name (`Hash`); derives
//! from other crates should be fully qualified (`derive_more::Display`).
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
//!
//! Settings can be assembled incrementally with the builder methods:
//!
//! ```
//! let settings = avrox::Settings::new()
//!     .layout(avrox::Layout::Module)
//!     .rust_types(avrox::RustTypes::Std)
//!     .rust_time(avrox::RustTime::Chrono)
//!     .rust_derives(["Hash", "derive_more::Display"]);
//!
//! assert_eq!(settings.layout, avrox::Layout::Module);
//! assert_eq!(settings.rust.derives.len(), 2);
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
    /// Options for Rust code generation.
    pub rust: RustOptions,
}

impl Default for Settings {
    /// Defaults to Rust, the latest supported Avro version, a single output
    /// file and [`RustTypes::ApacheAvro`].
    fn default() -> Self {
        Self {
            language: Language::Rust,
            avro_version: AvroVersion::V1_12,
            layout: Layout::SingleFile,
            rust: RustOptions::default(),
        }
    }
}

impl Settings {
    /// Creates settings with the same values as [`Settings::default`].
    ///
    /// Handy as the starting point of a builder chain:
    ///
    /// ```
    /// let settings = avrox::Settings::new()
    ///     .layout(avrox::Layout::Module)
    ///     .rust_types(avrox::RustTypes::Std);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the target language.
    #[must_use]
    pub fn language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }

    /// Sets the Avro specification version the schema is written against.
    #[must_use]
    pub fn avro_version(mut self, avro_version: AvroVersion) -> Self {
        self.avro_version = avro_version;
        self
    }

    /// Sets how the generated code is split into files.
    #[must_use]
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Replaces the Rust code generation options.
    #[must_use]
    pub fn rust(mut self, rust: RustOptions) -> Self {
        self.rust = rust;
        self
    }

    /// Sets which types the generated Rust code uses.
    #[must_use]
    pub fn rust_types(mut self, types: RustTypes) -> Self {
        self.rust.types = types;
        self
    }

    /// Sets how time-related logical types are represented in Rust.
    #[must_use]
    pub fn rust_time(mut self, time: RustTime) -> Self {
        self.rust.time = time;
        self
    }

    /// Adds a derive to generated Rust records, enums and union enums.
    ///
    /// Common derives can be given by their short name (`Hash`); derives from
    /// other crates should use their fully qualified path
    /// (`derive_more::Display`).
    #[must_use]
    pub fn rust_derive(mut self, derive: impl Into<String>) -> Self {
        self.rust.derives.push(derive.into());
        self
    }

    /// Replaces the additional derives of generated Rust records, enums and
    /// union enums.
    #[must_use]
    pub fn rust_derives<I, S>(mut self, derives: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rust.derives = derives.into_iter().map(Into::into).collect();
        self
    }
}

/// Options for Rust code generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustOptions {
    /// Which types the generated code uses.
    pub types: RustTypes,
    /// How time-related logical types are represented.
    pub time: RustTime,
    /// Additional derives to add to generated records, enums and union
    /// enums.
    ///
    /// Common derives can be given by their short name (for example `Hash` or
    /// `Display`). Derives from other crates should use their fully qualified
    /// path (for example `derive_more::Display`). Derives that the generated
    /// types already have are ignored.
    pub derives: Vec<String>,
}

impl Default for RustOptions {
    /// Defaults to [`RustTypes::ApacheAvro`], [`RustTime::Primitives`] and no
    /// additional derives.
    fn default() -> Self {
        Self {
            types: RustTypes::ApacheAvro,
            time: RustTime::Primitives,
            derives: Vec::new(),
        }
    }
}

impl RustOptions {
    /// Creates options with the same values as [`RustOptions::default`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets which types the generated code uses.
    #[must_use]
    pub fn types(mut self, types: RustTypes) -> Self {
        self.types = types;
        self
    }

    /// Sets how time-related logical types are represented.
    #[must_use]
    pub fn time(mut self, time: RustTime) -> Self {
        self.time = time;
        self
    }

    /// Adds a derive to generated records, enums and union enums.
    ///
    /// Common derives can be given by their short name (`Hash`); derives from
    /// other crates should use their fully qualified path
    /// (`derive_more::Display`).
    #[must_use]
    pub fn derive(mut self, derive: impl Into<String>) -> Self {
        self.derives.push(derive.into());
        self
    }

    /// Replaces the additional derives of generated records, enums and union
    /// enums.
    #[must_use]
    pub fn derives<I, S>(mut self, derives: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.derives = derives.into_iter().map(Into::into).collect();
        self
    }
}

/// How time-related logical types (`date`, `time-*`, `timestamp-*` and
/// `local-timestamp-*`) are represented in generated Rust code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RustTime {
    /// Their underlying integer types (`i32`/`i64`), documented on the fields
    /// carrying them. This is the default and needs no extra dependencies.
    Primitives,
    /// [`std::time`]: timestamps keep their natural [`std::time::SystemTime`]
    /// type and times-of-day use [`std::time::Duration`], with field-level
    /// `serde` adapters that encode them as the Avro integers of their logical
    /// types. `date` has no standard library representation and stays an
    /// `i32`. Logical types nested inside arrays or maps use generated
    /// newtypes because `serde` cannot attach field attributes to collection
    /// elements. Needs no extra dependencies.
    StdTime,
    /// [`chrono`]: time-related logical types keep their natural domain types
    /// ([`chrono::NaiveDate`], [`chrono::NaiveTime`], [`chrono::DateTime`] and
    /// [`chrono::NaiveDateTime`]) with field-level `serde` adapters that encode
    /// them as the Avro integers of their logical types. Logical types nested
    /// inside arrays or maps use generated newtypes because `serde` cannot
    /// attach field attributes to collection elements. The consuming crate
    /// needs `chrono` 0.4.40 or later with the `serde` feature enabled.
    Chrono,
}

/// Which types the generated Rust code uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RustTypes {
    /// Relies on types from the official `apache-avro` crate: `bytes` and
    /// `fixed` use `serde_bytes`, and the `uuid`, `decimal` and `duration`
    /// logical types become `apache_avro::Uuid`, `apache_avro::Decimal` and
    /// `apache_avro::Duration`. The generated code requires `apache-avro`,
    /// `serde` and `serde_bytes` to compile.
    ApacheAvro,
    /// Uses only standard library types plus types generated into the output
    /// (a `Bytes` newtype and one newtype per `fixed` type, with `serde`
    /// implementations that serialize as Avro bytes). The generated code
    /// requires only `serde` to compile and still round-trips through
    /// `apache-avro`'s binary codec. Logical types keep their underlying
    /// primitive representations.
    Std,
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
