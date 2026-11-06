//! Time logical-type representation selection and discovery.
//!
//! [`Time`] is the single entry point used by the rest of the generator. The
//! representation-specific rules live in the [`chrono`] and [`std_time`]
//! submodules.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;

use crate::RustTime;
use crate::schema::{Document, LogicalType, NamedType, Schema};

mod chrono;
mod render;
mod serde;
mod std_time;

pub(super) use render::render;
pub(super) use serde::helper_module;

/// The configured representation of time-related logical types.
///
/// All callers go through this type; the `chrono`-specific rules stay inside
/// [`chrono`].
#[derive(Clone, Copy)]
pub(super) struct Time {
    setting: RustTime,
}

impl Time {
    pub(super) fn new(setting: RustTime) -> Self {
        Self { setting }
    }

    /// The bare domain type used in positions that can carry a field-level
    /// `serde` attribute, if the representation has one.
    pub(super) fn direct_type(self, kind: LogicalType) -> Option<TokenStream> {
        match self.setting {
            RustTime::Primitives | RustTime::StdTime => None,
            RustTime::Chrono => chrono::direct_type(kind),
        }
    }

    /// The wrapper used in positions that cannot carry a field-level
    /// attribute, or `None` when the underlying type is used.
    pub(super) fn wrapper_for_position(
        self,
        kind: LogicalType,
        direct: bool,
    ) -> Option<TimeWrapper> {
        match self.setting {
            RustTime::Primitives => None,
            RustTime::StdTime => std_time::wrapper(kind),
            RustTime::Chrono => {
                if direct {
                    None
                } else {
                    chrono::wrapper(kind)
                }
            }
        }
    }

    /// The field-level `serde(with = "...")` adapter of a schema, if the
    /// representation uses bare domain types.
    pub(super) fn serde_with(self, schema: &Schema) -> Option<SerdeWith> {
        match self.setting {
            RustTime::Primitives | RustTime::StdTime => None,
            RustTime::Chrono => chrono::serde_with(schema),
        }
    }

    /// The display name of a logical type, used for generated union variants.
    pub(super) fn display_name(self, kind: LogicalType) -> Option<&'static str> {
        match self.setting {
            RustTime::Primitives => None,
            RustTime::StdTime => self
                .wrapper_for_position(kind, true)
                .map(|wrapper| wrapper.name),
            RustTime::Chrono => chrono::type_name(kind),
        }
    }

    /// The time wrapper newtypes the document needs under the current setting,
    /// in a deterministic order and without duplicates.
    pub(super) fn used_wrappers(self, document: &Document) -> Vec<TimeWrapper> {
        fn scan(schema: &Schema, direct: bool, time: Time, kinds: &mut Vec<LogicalType>) {
            match schema {
                Schema::Logical { kind, .. } => {
                    if time.wrapper_for_position(*kind, direct).is_some() {
                        kinds.push(*kind);
                    }
                }
                Schema::Array(items) | Schema::Map(items) => scan(items, false, time, kinds),
                Schema::Union(branches) => {
                    for branch in branches {
                        scan(branch, direct, time, kinds);
                    }
                }
                _ => {}
            }
        }
        if self.setting == RustTime::Primitives {
            return Vec::new();
        }
        let mut kinds = Vec::new();
        scan(&document.root, true, self, &mut kinds);
        for named in &document.types {
            if let NamedType::Record(record) = named {
                for field in &record.fields {
                    scan(&field.schema, true, self, &mut kinds);
                }
            }
        }
        let mut seen = HashSet::new();
        TIME_KINDS
            .iter()
            .filter(|kind| kinds.contains(kind))
            .filter_map(|kind| self.wrapper_for_position(*kind, false))
            .filter(|wrapper| seen.insert(wrapper.name))
            .collect()
    }

    /// Schema type names that collide with generated time wrappers.
    pub(super) fn reserved_names(self, document: &Document) -> Vec<&'static str> {
        if self.setting == RustTime::Primitives {
            return Vec::new();
        }
        let mut names = vec!["Time"];
        names.extend(
            self.used_wrappers(document)
                .iter()
                .map(|wrapper| wrapper.name),
        );
        names.sort_unstable();
        names.dedup();
        names
    }
}

/// The wire type of a time wrapper: the Avro integer the value is encoded as.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wire {
    I32,
    I64,
}

impl Wire {
    fn ty(self) -> TokenStream {
        match self {
            Self::I32 => quote!(i32),
            Self::I64 => quote!(i64),
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
        }
    }
}

/// A newtype generated for a time-related logical type in positions where a
/// field-level `serde` attribute cannot be used, such as array items and map
/// values.
pub(super) struct TimeWrapper {
    /// Rust name of the newtype.
    pub(super) name: &'static str,
    /// The logical type it represents.
    kind: LogicalType,
    /// The wrapped type.
    inner: TokenStream,
    /// The Avro integer the value is encoded as.
    wire: Wire,
    /// What the deserializer expects, for error messages.
    expecting: String,
    /// Expression producing `Option<the wire type>` from `self.0`.
    serialize: TokenStream,
    /// Expression producing `Option<the wrapped type>` from a wire value `v`.
    deserialize: TokenStream,
}

/// A serde adapter module a generated file may need for a logical type that
/// `chrono` does not provide a built-in module for.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Helper {
    Date,
    DateOption,
    TimeMillis,
    TimeMillisOption,
    TimeMicros,
    TimeMicrosOption,
}

/// A field-level `serde(with = "...")` adapter for a time logical type.
pub(super) struct SerdeWith {
    /// Path of the module containing `serialize` and `deserialize`.
    pub(super) path: &'static str,
    /// Generated helper module needed by the path, if any.
    pub(super) helper: Option<Helper>,
}

/// Time-related logical types, in the order their wrappers are emitted.
const TIME_KINDS: [LogicalType; 9] = [
    LogicalType::Date,
    LogicalType::TimeMillis,
    LogicalType::TimeMicros,
    LogicalType::TimestampMillis,
    LogicalType::TimestampMicros,
    LogicalType::TimestampNanos,
    LogicalType::LocalTimestampMillis,
    LogicalType::LocalTimestampMicros,
    LogicalType::LocalTimestampNanos,
];

/// Assembles a [`TimeWrapper`] with the standard metadata.
fn build_wrapper(
    name: &'static str,
    kind: LogicalType,
    inner: TokenStream,
    wire: Wire,
    serialize: TokenStream,
    deserialize: TokenStream,
) -> TimeWrapper {
    TimeWrapper {
        name,
        kind,
        inner,
        wire,
        expecting: format!("an Avro `{}` value", kind.avro_name()),
        serialize,
        deserialize,
    }
}
