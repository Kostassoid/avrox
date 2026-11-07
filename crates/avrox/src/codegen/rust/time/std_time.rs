//! `std::time` representations of time-related logical types.

use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;

use crate::schema::{LogicalType, SchemaNode};

use super::{Helper, SerdeWith, TimeWrapper, Wire, build_wrapper, direct_logical};

/// The bare `std::time` type a logical type maps to in direct positions.
pub(super) fn direct_type(kind: LogicalType) -> Option<TokenStream> {
    match kind {
        LogicalType::TimeMillis | LogicalType::TimeMicros => Some(quote!(std::time::Duration)),
        LogicalType::TimestampMillis
        | LogicalType::TimestampMicros
        | LogicalType::TimestampNanos
        | LogicalType::LocalTimestampMillis
        | LogicalType::LocalTimestampMicros
        | LogicalType::LocalTimestampNanos => Some(quote!(std::time::SystemTime)),
        _ => None,
    }
}

/// The display name of the bare `std::time` type a logical type maps to.
pub(super) fn type_name(kind: LogicalType) -> Option<&'static str> {
    match kind {
        LogicalType::TimeMillis | LogicalType::TimeMicros => Some("Duration"),
        LogicalType::TimestampMillis
        | LogicalType::TimestampMicros
        | LogicalType::TimestampNanos
        | LogicalType::LocalTimestampMillis
        | LogicalType::LocalTimestampMicros
        | LogicalType::LocalTimestampNanos => Some("SystemTime"),
        _ => None,
    }
}

/// The field-level `serde(with = "...")` adapter of a schema under
/// `RustTime::StdTime`, when it is a logical type or a nullable logical type.
pub(super) fn serde_with(node: &SchemaNode) -> Option<SerdeWith> {
    let (kind, optional) = direct_logical(node)?;
    let (path, helper) = match (kind, optional) {
        (LogicalType::TimeMillis, false) => ("__avrox_time::time_millis", Helper::TimeMillis),
        (LogicalType::TimeMillis, true) => {
            ("__avrox_time::time_millis_option", Helper::TimeMillisOption)
        }
        (LogicalType::TimeMicros, false) => ("__avrox_time::time_micros", Helper::TimeMicros),
        (LogicalType::TimeMicros, true) => {
            ("__avrox_time::time_micros_option", Helper::TimeMicrosOption)
        }
        (LogicalType::TimestampMillis | LogicalType::LocalTimestampMillis, false) => {
            ("__avrox_time::timestamp_millis", Helper::TimestampMillis)
        }
        (LogicalType::TimestampMillis | LogicalType::LocalTimestampMillis, true) => (
            "__avrox_time::timestamp_millis_option",
            Helper::TimestampMillisOption,
        ),
        (LogicalType::TimestampMicros | LogicalType::LocalTimestampMicros, false) => {
            ("__avrox_time::timestamp_micros", Helper::TimestampMicros)
        }
        (LogicalType::TimestampMicros | LogicalType::LocalTimestampMicros, true) => (
            "__avrox_time::timestamp_micros_option",
            Helper::TimestampMicrosOption,
        ),
        (LogicalType::TimestampNanos | LogicalType::LocalTimestampNanos, false) => {
            ("__avrox_time::timestamp_nanos", Helper::TimestampNanos)
        }
        (LogicalType::TimestampNanos | LogicalType::LocalTimestampNanos, true) => (
            "__avrox_time::timestamp_nanos_option",
            Helper::TimestampNanosOption,
        ),
        _ => return None,
    };
    Some(SerdeWith {
        path,
        helper: Some(helper),
    })
}

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

/// Renders the local `__avrox_time` module with the requested adapters.
pub(super) fn helper_module(helpers: &BTreeSet<Helper>) -> Option<TokenStream> {
    if helpers.is_empty() {
        return None;
    }
    let timestamp_millis = (helpers.contains(&Helper::TimestampMillis)
        || helpers.contains(&Helper::TimestampMillisOption))
    .then(timestamp_millis);
    let timestamp_millis_option = helpers
        .contains(&Helper::TimestampMillisOption)
        .then(timestamp_millis_option);
    let timestamp_micros = (helpers.contains(&Helper::TimestampMicros)
        || helpers.contains(&Helper::TimestampMicrosOption))
    .then(timestamp_micros);
    let timestamp_micros_option = helpers
        .contains(&Helper::TimestampMicrosOption)
        .then(timestamp_micros_option);
    let timestamp_nanos = (helpers.contains(&Helper::TimestampNanos)
        || helpers.contains(&Helper::TimestampNanosOption))
    .then(timestamp_nanos);
    let timestamp_nanos_option = helpers
        .contains(&Helper::TimestampNanosOption)
        .then(timestamp_nanos_option);
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
            #timestamp_millis
            #timestamp_millis_option
            #timestamp_micros
            #timestamp_micros_option
            #timestamp_nanos
            #timestamp_nanos_option
            #time_millis
            #time_millis_option
            #time_micros
            #time_micros_option
        }
    })
}

fn timestamp_millis() -> TokenStream {
    quote! {
        pub(crate) mod timestamp_millis {
            pub(super) fn millis(value: &std::time::SystemTime) -> i64 {
                let millis = match value.duration_since(std::time::UNIX_EPOCH) {
                    Ok(elapsed) => elapsed.as_millis() as i128,
                    Err(before) => -(before.duration().as_millis() as i128),
                };
                millis.clamp(i64::MIN as i128, i64::MAX as i128) as i64
            }

            fn from_millis(value: i64) -> Option<std::time::SystemTime> {
                let duration = std::time::Duration::from_millis(value.unsigned_abs());
                if value < 0 {
                    std::time::UNIX_EPOCH.checked_sub(duration)
                } else {
                    std::time::UNIX_EPOCH.checked_add(duration)
                }
            }

            pub(crate) fn serialize<S: serde::Serializer>(
                value: &std::time::SystemTime,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_i64(millis(value))
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<std::time::SystemTime, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = std::time::SystemTime;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `timestamp-millis` value")
                    }

                    fn visit_i64<E: serde::de::Error>(
                        self,
                        value: i64,
                    ) -> Result<Self::Value, E> {
                        from_millis(value).ok_or_else(|| E::custom("value out of range"))
                    }
                }

                deserializer.deserialize_i64(Visitor)
            }
        }
    }
}

fn timestamp_millis_option() -> TokenStream {
    quote! {
        pub(crate) mod timestamp_millis_option {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &Option<std::time::SystemTime>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => {
                        serializer.serialize_some(&super::timestamp_millis::millis(value))
                    }
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<std::time::SystemTime>, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Option<std::time::SystemTime>;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `timestamp-millis` value or null")
                    }

                    fn visit_some<D: serde::Deserializer<'de>>(
                        self,
                        deserializer: D,
                    ) -> Result<Self::Value, D::Error> {
                        super::timestamp_millis::deserialize(deserializer).map(Some)
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

fn timestamp_micros() -> TokenStream {
    quote! {
        pub(crate) mod timestamp_micros {
            pub(super) fn micros(value: &std::time::SystemTime) -> i64 {
                let micros = match value.duration_since(std::time::UNIX_EPOCH) {
                    Ok(elapsed) => elapsed.as_micros() as i128,
                    Err(before) => -(before.duration().as_micros() as i128),
                };
                micros.clamp(i64::MIN as i128, i64::MAX as i128) as i64
            }

            fn from_micros(value: i64) -> Option<std::time::SystemTime> {
                let duration = std::time::Duration::from_micros(value.unsigned_abs());
                if value < 0 {
                    std::time::UNIX_EPOCH.checked_sub(duration)
                } else {
                    std::time::UNIX_EPOCH.checked_add(duration)
                }
            }

            pub(crate) fn serialize<S: serde::Serializer>(
                value: &std::time::SystemTime,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_i64(micros(value))
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<std::time::SystemTime, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = std::time::SystemTime;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `timestamp-micros` value")
                    }

                    fn visit_i64<E: serde::de::Error>(
                        self,
                        value: i64,
                    ) -> Result<Self::Value, E> {
                        from_micros(value).ok_or_else(|| E::custom("value out of range"))
                    }
                }

                deserializer.deserialize_i64(Visitor)
            }
        }
    }
}

fn timestamp_micros_option() -> TokenStream {
    quote! {
        pub(crate) mod timestamp_micros_option {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &Option<std::time::SystemTime>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => {
                        serializer.serialize_some(&super::timestamp_micros::micros(value))
                    }
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<std::time::SystemTime>, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Option<std::time::SystemTime>;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `timestamp-micros` value or null")
                    }

                    fn visit_some<D: serde::Deserializer<'de>>(
                        self,
                        deserializer: D,
                    ) -> Result<Self::Value, D::Error> {
                        super::timestamp_micros::deserialize(deserializer).map(Some)
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

fn timestamp_nanos() -> TokenStream {
    quote! {
        pub(crate) mod timestamp_nanos {
            pub(super) fn nanos(value: &std::time::SystemTime) -> i64 {
                let nanos = match value.duration_since(std::time::UNIX_EPOCH) {
                    Ok(elapsed) => elapsed.as_nanos() as i128,
                    Err(before) => -(before.duration().as_nanos() as i128),
                };
                nanos.clamp(i64::MIN as i128, i64::MAX as i128) as i64
            }

            fn from_nanos(value: i64) -> Option<std::time::SystemTime> {
                let duration = std::time::Duration::from_nanos(value.unsigned_abs());
                if value < 0 {
                    std::time::UNIX_EPOCH.checked_sub(duration)
                } else {
                    std::time::UNIX_EPOCH.checked_add(duration)
                }
            }

            pub(crate) fn serialize<S: serde::Serializer>(
                value: &std::time::SystemTime,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_i64(nanos(value))
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<std::time::SystemTime, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = std::time::SystemTime;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `timestamp-nanos` value")
                    }

                    fn visit_i64<E: serde::de::Error>(
                        self,
                        value: i64,
                    ) -> Result<Self::Value, E> {
                        from_nanos(value).ok_or_else(|| E::custom("value out of range"))
                    }
                }

                deserializer.deserialize_i64(Visitor)
            }
        }
    }
}

fn timestamp_nanos_option() -> TokenStream {
    quote! {
        pub(crate) mod timestamp_nanos_option {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &Option<std::time::SystemTime>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => {
                        serializer.serialize_some(&super::timestamp_nanos::nanos(value))
                    }
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<std::time::SystemTime>, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Option<std::time::SystemTime>;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("an Avro `timestamp-nanos` value or null")
                    }

                    fn visit_some<D: serde::Deserializer<'de>>(
                        self,
                        deserializer: D,
                    ) -> Result<Self::Value, D::Error> {
                        super::timestamp_nanos::deserialize(deserializer).map(Some)
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
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &std::time::Duration,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_i32(
                    i32::try_from(value.as_millis())
                        .map_err(|_| serde::ser::Error::custom("value out of range"))?,
                )
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<std::time::Duration, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = std::time::Duration;

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
                        u64::try_from(value)
                            .map(std::time::Duration::from_millis)
                            .map_err(|_| E::custom("value out of range"))
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
                value: &Option<std::time::Duration>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => serializer.serialize_some(
                        &i32::try_from(value.as_millis())
                            .map_err(|_| serde::ser::Error::custom("value out of range"))?,
                    ),
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<std::time::Duration>, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Option<std::time::Duration>;

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
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &std::time::Duration,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_i64(
                    i64::try_from(value.as_micros())
                        .map_err(|_| serde::ser::Error::custom("value out of range"))?,
                )
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<std::time::Duration, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = std::time::Duration;

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
                        u64::try_from(value)
                            .map(std::time::Duration::from_micros)
                            .map_err(|_| E::custom("value out of range"))
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
                value: &Option<std::time::Duration>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => serializer.serialize_some(
                        &i64::try_from(value.as_micros())
                            .map_err(|_| serde::ser::Error::custom("value out of range"))?,
                    ),
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<std::time::Duration>, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Option<std::time::Duration>;

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
