//! `chrono` representations of time-related logical types.

use quote::quote;

use crate::schema::LogicalType;

use super::{TimeWrapper, Wire, build_wrapper};

/// The wrappers for the wall-clock values of `RustTime::Chrono`.
pub(super) fn wrapper(kind: LogicalType) -> Option<TimeWrapper> {
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
pub(super) fn timestamp_wrapper(kind: LogicalType) -> Option<TimeWrapper> {
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
