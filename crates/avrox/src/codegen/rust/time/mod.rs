//! Time logical-type wrapper selection and discovery.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;

use crate::RustTime;
use crate::schema::{Document, LogicalType, NamedType, Schema};

mod chrono;
mod render;
mod std_time;

pub(super) use render::render;

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

/// A newtype generated for a time-related logical type under
/// [`RustTime::StdTime`] or [`RustTime::Chrono`].
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

/// The wrapper a time-related logical type maps to under the given settings,
/// or `None` when its underlying integer is used.
pub(super) fn wrapper(time: RustTime, kind: LogicalType) -> Option<TimeWrapper> {
    match time {
        RustTime::Primitives => None,
        RustTime::StdTime => std_time::wrapper(kind),
        RustTime::Chrono => chrono::wrapper(kind).or_else(|| chrono::timestamp_wrapper(kind)),
    }
}

/// The time wrapper newtypes the document needs under the current setting, in
/// a deterministic order and without duplicates.
pub(super) fn used_wrappers(document: &Document, time: RustTime) -> Vec<TimeWrapper> {
    fn scan(schema: &Schema, kinds: &mut Vec<LogicalType>) {
        match schema {
            Schema::Logical { kind, .. } => kinds.push(*kind),
            Schema::Array(items) | Schema::Map(items) => scan(items, kinds),
            Schema::Union(branches) => {
                for branch in branches {
                    scan(branch, kinds);
                }
            }
            _ => {}
        }
    }
    if time == RustTime::Primitives {
        return Vec::new();
    }
    let mut kinds = Vec::new();
    scan(&document.root, &mut kinds);
    for named in &document.types {
        if let NamedType::Record(record) = named {
            for field in &record.fields {
                scan(&field.schema, &mut kinds);
            }
        }
    }
    let mut seen = HashSet::new();
    TIME_KINDS
        .iter()
        .filter(|kind| kinds.contains(kind))
        .filter_map(|kind| wrapper(time, *kind))
        .filter(|wrapper| seen.insert(wrapper.name.to_owned()))
        .collect()
}

/// Schema type names that collide with generated time wrappers (or the
/// generated `time` module) under the given [`RustTime`] setting.
pub(super) fn reserved_names(time: RustTime) -> &'static [&'static str] {
    match time {
        RustTime::Primitives => &[],
        RustTime::StdTime => &[
            "Time",
            "TimestampMillis",
            "TimestampMicros",
            "TimestampNanos",
            "TimeMillis",
            "TimeMicros",
        ],
        RustTime::Chrono => &[
            "Time",
            "Date",
            "TimestampMillis",
            "TimestampMicros",
            "TimestampNanos",
            "TimeMillis",
            "TimeMicros",
            "LocalTimestampMillis",
            "LocalTimestampMicros",
            "LocalTimestampNanos",
        ],
    }
}

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
