//! `chrono` representations of time-related logical types.

use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;

use crate::schema::{LogicalType, Schema};

use super::{Helper, SerdeWith, TimeWrapper, Wire, build_wrapper, direct_logical};

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
    let (kind, optional) = direct_logical(schema)?;
    logical_serde(kind, optional)
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

/// Renders the local `__avrox_time` module with the requested adapters.
pub(super) fn helper_module(helpers: &BTreeSet<Helper>) -> Option<TokenStream> {
    if helpers.is_empty() {
        return None;
    }
    let date =
        (helpers.contains(&Helper::Date) || helpers.contains(&Helper::DateOption)).then(date);
    let date_option = helpers.contains(&Helper::DateOption).then(date_option);
    let time_millis = (helpers.contains(&Helper::TimeMillis)
        || helpers.contains(&Helper::TimeMillisOption))
    .then(time_millis);
    let time_millis_option = helpers
        .contains(&Helper::TimeMillisOption)
        .then(time_millis_option);
    let time_micros = (helpers.contains(&Helper::TimeMicros)
        || helpers.contains(&Helper::TimeMicrosOption))
    .then(time_micros);
    let time_micros_option = helpers
        .contains(&Helper::TimeMicrosOption)
        .then(time_micros_option);
    Some(quote! {
        #[allow(dead_code)]
        mod __avrox_time {
            #date
            #date_option
            #time_millis
            #time_millis_option
            #time_micros
            #time_micros_option
        }
    })
}

fn date() -> TokenStream {
    quote! {
        pub(crate) mod date {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &chrono::NaiveDate,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_i32(value.to_epoch_days())
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<chrono::NaiveDate, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = chrono::NaiveDate;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `date` value")
                    }

                    fn visit_i32<E: serde::de::Error>(
                        self,
                        value: i32,
                    ) -> Result<Self::Value, E> {
                        chrono::NaiveDate::from_epoch_days(value)
                            .ok_or_else(|| E::custom("value out of range"))
                    }
                }

                deserializer.deserialize_i32(Visitor)
            }
        }
    }
}

fn date_option() -> TokenStream {
    quote! {
        pub(crate) mod date_option {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &Option<chrono::NaiveDate>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => serializer.serialize_some(&value.to_epoch_days()),
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<chrono::NaiveDate>, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Option<chrono::NaiveDate>;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `date` value or null")
                    }

                    fn visit_some<D: serde::Deserializer<'de>>(
                        self,
                        deserializer: D,
                    ) -> Result<Self::Value, D::Error> {
                        super::date::deserialize(deserializer).map(Some)
                    }

                    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                        Ok(None)
                    }

                    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                        Ok(None)
                    }
                }

                deserializer.deserialize_option(Visitor)
            }
        }
    }
}

fn time_millis() -> TokenStream {
    quote! {
        pub(crate) mod time_millis {
            pub(super) fn millis(value: &chrono::NaiveTime) -> Option<i32> {
                i32::try_from(
                    u64::from(chrono::Timelike::hour(value)) * 3_600_000
                        + u64::from(chrono::Timelike::minute(value)) * 60_000
                        + u64::from(chrono::Timelike::second(value)) * 1_000
                        + u64::from(chrono::Timelike::nanosecond(value)) / 1_000_000,
                )
                .ok()
            }

            fn from_millis(value: i32) -> Option<chrono::NaiveTime> {
                if value < 0 {
                    return None;
                }
                u32::try_from(value / 1_000).ok().and_then(|seconds| {
                    chrono::NaiveTime::from_num_seconds_from_midnight_opt(
                        seconds,
                        (value % 1_000) as u32 * 1_000_000,
                    )
                })
            }

            pub(crate) fn serialize<S: serde::Serializer>(
                value: &chrono::NaiveTime,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_i32(
                    millis(value)
                        .ok_or_else(|| serde::ser::Error::custom("value out of range"))?,
                )
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<chrono::NaiveTime, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = chrono::NaiveTime;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `time-millis` value")
                    }

                    fn visit_i32<E: serde::de::Error>(
                        self,
                        value: i32,
                    ) -> Result<Self::Value, E> {
                        from_millis(value)
                            .ok_or_else(|| E::custom("value out of range"))
                    }
                }

                deserializer.deserialize_i32(Visitor)
            }
        }
    }
}

fn time_millis_option() -> TokenStream {
    quote! {
        pub(crate) mod time_millis_option {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &Option<chrono::NaiveTime>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => serializer.serialize_some(
                        &super::time_millis::millis(value)
                            .ok_or_else(|| serde::ser::Error::custom("value out of range"))?,
                    ),
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<chrono::NaiveTime>, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Option<chrono::NaiveTime>;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `time-millis` value or null")
                    }

                    fn visit_some<D: serde::Deserializer<'de>>(
                        self,
                        deserializer: D,
                    ) -> Result<Self::Value, D::Error> {
                        super::time_millis::deserialize(deserializer).map(Some)
                    }

                    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                        Ok(None)
                    }

                    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                        Ok(None)
                    }
                }

                deserializer.deserialize_option(Visitor)
            }
        }
    }
}

fn time_micros() -> TokenStream {
    quote! {
        pub(crate) mod time_micros {
            pub(super) fn micros(value: &chrono::NaiveTime) -> Option<i64> {
                i64::try_from(
                    u64::from(chrono::Timelike::hour(value)) * 3_600_000_000
                        + u64::from(chrono::Timelike::minute(value)) * 60_000_000
                        + u64::from(chrono::Timelike::second(value)) * 1_000_000
                        + u64::from(chrono::Timelike::nanosecond(value)) / 1_000,
                )
                .ok()
            }

            fn from_micros(value: i64) -> Option<chrono::NaiveTime> {
                if value < 0 {
                    return None;
                }
                u32::try_from(value / 1_000_000).ok().and_then(|seconds| {
                    chrono::NaiveTime::from_num_seconds_from_midnight_opt(
                        seconds,
                        (value % 1_000_000) as u32 * 1_000,
                    )
                })
            }

            pub(crate) fn serialize<S: serde::Serializer>(
                value: &chrono::NaiveTime,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_i64(
                    micros(value)
                        .ok_or_else(|| serde::ser::Error::custom("value out of range"))?,
                )
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<chrono::NaiveTime, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = chrono::NaiveTime;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `time-micros` value")
                    }

                    fn visit_i64<E: serde::de::Error>(
                        self,
                        value: i64,
                    ) -> Result<Self::Value, E> {
                        from_micros(value)
                            .ok_or_else(|| E::custom("value out of range"))
                    }
                }

                deserializer.deserialize_i64(Visitor)
            }
        }
    }
}

fn time_micros_option() -> TokenStream {
    quote! {
        pub(crate) mod time_micros_option {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &Option<chrono::NaiveTime>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => serializer.serialize_some(
                        &super::time_micros::micros(value)
                            .ok_or_else(|| serde::ser::Error::custom("value out of range"))?,
                    ),
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<chrono::NaiveTime>, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Option<chrono::NaiveTime>;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `time-micros` value or null")
                    }

                    fn visit_some<D: serde::Deserializer<'de>>(
                        self,
                        deserializer: D,
                    ) -> Result<Self::Value, D::Error> {
                        super::time_micros::deserialize(deserializer).map(Some)
                    }

                    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                        Ok(None)
                    }

                    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                        Ok(None)
                    }
                }

                deserializer.deserialize_option(Visitor)
            }
        }
    }
}
