//! Generated serde adapters for logical types `chrono` has no modules for.

use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;

use super::Helper;

/// Renders the local `__avrox_time` module with the requested adapters.
pub(in crate::codegen::rust) fn helper_module(helpers: &BTreeSet<Helper>) -> Option<TokenStream> {
    if helpers.is_empty() {
        return None;
    }
    let date = helpers.contains(&Helper::Date).then(date);
    let date_option = helpers.contains(&Helper::DateOption).then(date_option);
    let time_millis = helpers.contains(&Helper::TimeMillis).then(time_millis);
    let time_millis_option = helpers
        .contains(&Helper::TimeMillisOption)
        .then(time_millis_option);
    let time_micros = helpers.contains(&Helper::TimeMicros).then(time_micros);
    let time_micros_option = helpers
        .contains(&Helper::TimeMicrosOption)
        .then(time_micros_option);
    Some(quote! {
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
