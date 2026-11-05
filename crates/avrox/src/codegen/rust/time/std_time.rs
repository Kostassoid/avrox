//! `std::time` representations of time-related logical types.

use quote::quote;

use crate::schema::LogicalType;

use super::{TimeWrapper, Wire, build_wrapper};

/// The wrappers for the times and instants of `RustTime::StdTime`.
pub(super) fn wrapper(kind: LogicalType) -> Option<TimeWrapper> {
    let (name, inner, wire, serialize, deserialize) = match kind {
        LogicalType::TimestampMillis | LogicalType::LocalTimestampMillis => (
            "TimestampMillis",
            quote!(std::time::SystemTime),
            Wire::I64,
            quote! {
                {
                    let millis = match self.0.duration_since(std::time::UNIX_EPOCH) {
                        Ok(elapsed) => elapsed.as_millis() as i128,
                        Err(before) => -(before.duration().as_millis() as i128),
                    };
                    Some(millis.clamp(i64::MIN as i128, i64::MAX as i128) as i64)
                }
            },
            quote! {
                {
                    let duration = std::time::Duration::from_millis(v.unsigned_abs());
                    if v < 0 {
                        std::time::UNIX_EPOCH.checked_sub(duration)
                    } else {
                        std::time::UNIX_EPOCH.checked_add(duration)
                    }
                }
            },
        ),
        LogicalType::TimestampMicros | LogicalType::LocalTimestampMicros => (
            "TimestampMicros",
            quote!(std::time::SystemTime),
            Wire::I64,
            quote! {
                {
                    let micros = match self.0.duration_since(std::time::UNIX_EPOCH) {
                        Ok(elapsed) => elapsed.as_micros() as i128,
                        Err(before) => -(before.duration().as_micros() as i128),
                    };
                    Some(micros.clamp(i64::MIN as i128, i64::MAX as i128) as i64)
                }
            },
            quote! {
                {
                    let duration = std::time::Duration::from_micros(v.unsigned_abs());
                    if v < 0 {
                        std::time::UNIX_EPOCH.checked_sub(duration)
                    } else {
                        std::time::UNIX_EPOCH.checked_add(duration)
                    }
                }
            },
        ),
        LogicalType::TimestampNanos | LogicalType::LocalTimestampNanos => (
            "TimestampNanos",
            quote!(std::time::SystemTime),
            Wire::I64,
            quote! {
                {
                    let nanos = match self.0.duration_since(std::time::UNIX_EPOCH) {
                        Ok(elapsed) => elapsed.as_nanos() as i128,
                        Err(before) => -(before.duration().as_nanos() as i128),
                    };
                    Some(nanos.clamp(i64::MIN as i128, i64::MAX as i128) as i64)
                }
            },
            quote! {
                {
                    let duration = std::time::Duration::from_nanos(v.unsigned_abs());
                    if v < 0 {
                        std::time::UNIX_EPOCH.checked_sub(duration)
                    } else {
                        std::time::UNIX_EPOCH.checked_add(duration)
                    }
                }
            },
        ),
        LogicalType::TimeMillis => (
            "TimeMillis",
            quote!(std::time::Duration),
            Wire::I32,
            quote! { i32::try_from(self.0.as_millis()).ok() },
            quote! { u64::try_from(v).ok().map(std::time::Duration::from_millis) },
        ),
        LogicalType::TimeMicros => (
            "TimeMicros",
            quote!(std::time::Duration),
            Wire::I64,
            quote! { i64::try_from(self.0.as_micros()).ok() },
            quote! { u64::try_from(v).ok().map(std::time::Duration::from_micros) },
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
