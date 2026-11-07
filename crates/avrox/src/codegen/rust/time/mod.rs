//! Time logical-type representation selection and discovery.
//!
//! [`Time`] is the single entry point used by the rest of the generator. The
//! representation-specific rules live in the [`chrono`] and [`std_time`]
//! submodules.

use std::collections::{BTreeSet, HashSet};

use proc_macro2::TokenStream;
use quote::quote;

use crate::RustTime;
use crate::schema::{LogicalType, NamedType, Schema, SchemaNode};

mod chrono;
mod render;
mod std_time;

pub(super) use render::render;

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
            RustTime::Primitives => None,
            RustTime::StdTime => std_time::direct_type(kind),
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
            RustTime::StdTime => {
                if direct {
                    None
                } else {
                    std_time::wrapper(kind)
                }
            }
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
    pub(super) fn serde_with(self, node: &SchemaNode) -> Option<SerdeWith> {
        match self.setting {
            RustTime::Primitives => None,
            RustTime::StdTime => std_time::serde_with(node),
            RustTime::Chrono => chrono::serde_with(node),
        }
    }

    /// The display name of a logical type, used for generated union variants.
    pub(super) fn display_name(self, kind: LogicalType) -> Option<&'static str> {
        match self.setting {
            RustTime::Primitives => None,
            RustTime::StdTime => std_time::type_name(kind),
            RustTime::Chrono => chrono::type_name(kind),
        }
    }

    /// The serde helper module a generated file needs, if any.
    pub(super) fn helper_module(self, helpers: &BTreeSet<Helper>) -> Option<TokenStream> {
        match self.setting {
            RustTime::Primitives => None,
            RustTime::StdTime => std_time::helper_module(helpers),
            RustTime::Chrono => chrono::helper_module(helpers),
        }
    }

    /// The time wrapper newtypes the schema needs under the current setting,
    /// in a deterministic order and without duplicates.
    pub(super) fn used_wrappers(self, schema: &Schema) -> Vec<TimeWrapper> {
        fn scan(node: &SchemaNode, direct: bool, time: Time, kinds: &mut Vec<LogicalType>) {
            match node {
                SchemaNode::Logical { kind, .. } => {
                    if time.wrapper_for_position(*kind, direct).is_some() {
                        kinds.push(*kind);
                    }
                }
                SchemaNode::Array(items) | SchemaNode::Map(items) => {
                    scan(items, false, time, kinds);
                }
                SchemaNode::Union(branches) => {
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
        scan(&schema.root, true, self, &mut kinds);
        for named in &schema.types {
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
    pub(super) fn reserved_names(self, schema: &Schema) -> Vec<&'static str> {
        if self.setting == RustTime::Primitives {
            return Vec::new();
        }
        let mut names = vec!["Time"];
        names.extend(
            self.used_wrappers(schema)
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

/// A serde adapter module a generated file may need when the selected
/// representation has no built-in adapter for a logical type.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Helper {
    Date,
    DateOption,
    TimeMillis,
    TimeMillisOption,
    TimeMicros,
    TimeMicrosOption,
    TimestampMillis,
    TimestampMillisOption,
    TimestampMicros,
    TimestampMicrosOption,
    TimestampNanos,
    TimestampNanosOption,
}

/// A field-level `serde(with = "...")` adapter for a time logical type.
pub(super) struct SerdeWith {
    /// Path of the module containing `serialize` and `deserialize`.
    pub(super) path: &'static str,
    /// Generated helper module needed by the path, if any.
    pub(super) helper: Option<Helper>,
}

/// The logical type a schema carries at a field-like position, and whether it
/// is wrapped in a nullable union.
pub(super) fn direct_logical(node: &SchemaNode) -> Option<(LogicalType, bool)> {
    match node {
        SchemaNode::Logical { kind, .. } => Some((*kind, false)),
        SchemaNode::Union(branches) => match branches.as_slice() {
            [branch] => direct_logical(branch),
            [first, second] => match (
                matches!(first, SchemaNode::Null),
                matches!(second, SchemaNode::Null),
            ) {
                (true, false) => nullable_logical(second),
                (false, true) => nullable_logical(first),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn nullable_logical(node: &SchemaNode) -> Option<(LogicalType, bool)> {
    let SchemaNode::Logical { kind, .. } = node else {
        return None;
    };
    Some((*kind, true))
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
