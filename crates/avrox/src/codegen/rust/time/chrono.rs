//! `chrono` representations of time-related logical types.

use proc_macro2::TokenStream;
use quote::quote;

use crate::schema::{LogicalType, Schema};

use super::{Helper, SerdeWith, TimeWrapper, Wire, build_wrapper};

/// The bare `chrono` type a logical type maps to in direct positions.
pub(super) fn direct_type(kind: LogicalType) -> Option<TokenStream> {
    match kind {
        LogicalType::Date => Some(quote!(chrono::NaiveDate)),
        LogicalType::TimeMillis | LogicalType::TimeMicros => Some(quote!(chrono::NaiveTime)),
        LogicalType::TimestampMillis
        | LogicalType::TimestampMicros
        | LogicalType::TimestampNanos => Some(quote!(chrono::DateTime<chrono::Utc>)),
        LogicalType::LocalTimestampMillis
        | LogicalType::LocalTimestampMicros
        | LogicalType::LocalTimestampNanos => Some(quote!(chrono::NaiveDateTime)),
        _ => None,
    }
}

/// The display name of the bare `chrono` type a logical type maps to.
pub(super) fn type_name(kind: LogicalType) -> Option<&'static str> {
    match kind {
        LogicalType::Date => Some("NaiveDate"),
        LogicalType::TimeMillis | LogicalType::TimeMicros => Some("NaiveTime"),
        LogicalType::TimestampMillis
        | LogicalType::TimestampMicros
        | LogicalType::TimestampNanos => Some("DateTime"),
        LogicalType::LocalTimestampMillis
        | LogicalType::LocalTimestampMicros
        | LogicalType::LocalTimestampNanos => Some("NaiveDateTime"),
        _ => None,
    }
}

/// The field-level `serde(with = "...")` adapter of a schema under
/// `RustTime::Chrono`, when it is a logical type or a nullable logical type.
pub(super) fn serde_with(schema: &Schema) -> Option<SerdeWith> {
    match schema {
        Schema::Logical { kind, .. } => logical_serde(*kind, false),
        Schema::Union(branches) => match branches.as_slice() {
            [branch] => serde_with(branch),
            [first, second] => match (
                matches!(first, Schema::Null),
                matches!(second, Schema::Null),
            ) {
                (true, false) => nullable_serde(second),
                (false, true) => nullable_serde(first),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn nullable_serde(schema: &Schema) -> Option<SerdeWith> {
    let Schema::Logical { kind, .. } = schema else {
        return None;
    };
    logical_serde(*kind, true)
}

fn logical_serde(kind: LogicalType, optional: bool) -> Option<SerdeWith> {
    let (path, helper) = match (kind, optional) {
        (LogicalType::Date, false) => ("__avrox_time::date", Some(Helper::Date)),
        (LogicalType::Date, true) => ("__avrox_time::date_option", Some(Helper::DateOption)),
        (LogicalType::TimeMillis, false) => ("__avrox_time::time_millis", Some(Helper::TimeMillis)),
        (LogicalType::TimeMillis, true) => (
            "__avrox_time::time_millis_option",
            Some(Helper::TimeMillisOption),
        ),
        (LogicalType::TimeMicros, false) => ("__avrox_time::time_micros", Some(Helper::TimeMicros)),
        (LogicalType::TimeMicros, true) => (
            "__avrox_time::time_micros_option",
            Some(Helper::TimeMicrosOption),
        ),
        (LogicalType::TimestampMillis, false) => ("chrono::serde::ts_milliseconds", None),
        (LogicalType::TimestampMillis, true) => ("chrono::serde::ts_milliseconds_option", None),
        (LogicalType::TimestampMicros, false) => ("chrono::serde::ts_microseconds", None),
        (LogicalType::TimestampMicros, true) => ("chrono::serde::ts_microseconds_option", None),
        (LogicalType::TimestampNanos, false) => ("chrono::serde::ts_nanoseconds", None),
        (LogicalType::TimestampNanos, true) => ("chrono::serde::ts_nanoseconds_option", None),
        (LogicalType::LocalTimestampMillis, false) => {
            ("chrono::naive::serde::ts_milliseconds", None)
        }
        (LogicalType::LocalTimestampMillis, true) => {
            ("chrono::naive::serde::ts_milliseconds_option", None)
        }
        (LogicalType::LocalTimestampMicros, false) => {
            ("chrono::naive::serde::ts_microseconds", None)
        }
        (LogicalType::LocalTimestampMicros, true) => {
            ("chrono::naive::serde::ts_microseconds_option", None)
        }
        (LogicalType::LocalTimestampNanos, false) => ("chrono::naive::serde::ts_nanoseconds", None),
        (LogicalType::LocalTimestampNanos, true) => {
            ("chrono::naive::serde::ts_nanoseconds_option", None)
        }
        _ => return None,
    };
    Some(SerdeWith { path, helper })
}

/// The wrappers for the wall-clock values of `RustTime::Chrono`.
fn wall_clock_wrapper(kind: LogicalType) -> Option<TimeWrapper> {
    let (name, inner, wire, serialize, deserialize) = match kind {
        LogicalType::Date => (
            "Date",
            quote!(chrono::NaiveDate),
            Wire::I32,
            quote! { Some(self.0.to_epoch_days()) },
            quote! { chrono::NaiveDate::from_epoch_days(v) },
        ),
        LogicalType::TimeMillis => (
            "TimeMillis",
            quote!(chrono::NaiveTime),
            Wire::I32,
            quote! {
                i32::try_from(
                    u64::from(chrono::Timelike::hour(&self.0)) * 3_600_000
                        + u64::from(chrono::Timelike::minute(&self.0)) * 60_000
                        + u64::from(chrono::Timelike::second(&self.0)) * 1000
                        + u64::from(chrono::Timelike::nanosecond(&self.0)) / 1_000_000,
                )
                .ok()
            },
            quote! {
                if v < 0 {
                    None
                } else {
                    u32::try_from(v / 1000).ok().and_then(|seconds| {
                        chrono::NaiveTime::from_num_seconds_from_midnight_opt(
                            seconds,
                            (v % 1000) as u32 * 1_000_000,
                        )
                    })
                }
            },
        ),
        LogicalType::TimeMicros => (
            "TimeMicros",
            quote!(chrono::NaiveTime),
            Wire::I64,
            quote! {
                i64::try_from(
                    u64::from(chrono::Timelike::hour(&self.0)) * 3_600_000_000
                        + u64::from(chrono::Timelike::minute(&self.0)) * 60_000_000
                        + u64::from(chrono::Timelike::second(&self.0)) * 1_000_000
                        + u64::from(chrono::Timelike::nanosecond(&self.0)) / 1_000,
                )
                .ok()
            },
            quote! {
                if v < 0 {
                    None
                } else {
                    u32::try_from(v / 1_000_000).ok().and_then(|seconds| {
                        chrono::NaiveTime::from_num_seconds_from_midnight_opt(
                            seconds,
                            (v % 1_000_000) as u32 * 1_000,
                        )
                    })
                }
            },
        ),
        _ => return None,
    };
    Some(build_wrapper(
        name,
        kind,
        inner,
        wire,
        serialize,
        deserialize,
    ))
}

/// The wrappers for the instants of `RustTime::Chrono`.
fn timestamp_wrapper(kind: LogicalType) -> Option<TimeWrapper> {
    let (name, inner, wire, serialize, deserialize) = match kind {
        LogicalType::TimestampMillis => (
            "TimestampMillis",
            quote!(chrono::DateTime<chrono::Utc>),
            Wire::I64,
            quote! { Some(self.0.timestamp_millis()) },
            quote! { chrono::DateTime::<chrono::Utc>::from_timestamp_millis(v) },
        ),
        LogicalType::TimestampMicros => (
            "TimestampMicros",
            quote!(chrono::DateTime<chrono::Utc>),
            Wire::I64,
            quote! { Some(self.0.timestamp_micros()) },
            quote! { chrono::DateTime::<chrono::Utc>::from_timestamp_micros(v) },
        ),
        LogicalType::TimestampNanos => (
            "TimestampNanos",
            quote!(chrono::DateTime<chrono::Utc>),
            Wire::I64,
            quote! { self.0.timestamp_nanos_opt() },
            quote! { Some(chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(v)) },
        ),
        LogicalType::LocalTimestampMillis => (
            "LocalTimestampMillis",
            quote!(chrono::NaiveDateTime),
            Wire::I64,
            quote! { Some(self.0.and_utc().timestamp_millis()) },
            quote! {
                chrono::DateTime::<chrono::Utc>::from_timestamp_millis(v)
                    .map(|datetime| datetime.naive_utc())
            },
        ),
        LogicalType::LocalTimestampMicros => (
            "LocalTimestampMicros",
            quote!(chrono::NaiveDateTime),
            Wire::I64,
            quote! { Some(self.0.and_utc().timestamp_micros()) },
            quote! {
                chrono::DateTime::<chrono::Utc>::from_timestamp_micros(v)
                    .map(|datetime| datetime.naive_utc())
            },
        ),
        LogicalType::LocalTimestampNanos => (
            "LocalTimestampNanos",
            quote!(chrono::NaiveDateTime),
            Wire::I64,
            quote! { self.0.and_utc().timestamp_nanos_opt() },
            quote! {
                Some(chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(v).naive_utc())
            },
        ),
        _ => return None,
    };
    Some(build_wrapper(
        name,
        kind,
        inner,
        wire,
        serialize,
        deserialize,
    ))
}

/// The wrapper for any time-related logical type.
pub(super) fn wrapper(kind: LogicalType) -> Option<TimeWrapper> {
    wall_clock_wrapper(kind).or_else(|| timestamp_wrapper(kind))
}
