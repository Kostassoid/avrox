//! `uuid` over `string` representation.
//!
//! In `avro` mode these schemas become `apache_avro::Uuid`. That type's default
//! `serde` representation is the RFC 4122 byte layout, which a string-based
//! `uuid` schema rejects, so the generator supplies its own serialization:
//! direct positions get a field-level adapter and positions that cannot carry
//! one use a generated `Uuid` newtype that serializes as a string.

use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;

use crate::RustTypes;
use crate::schema::{LogicalType, NamedType, Schema, SchemaNode};

use super::{docs, naming};

/// Whether a schema is a `uuid` logical type over `string`.
fn is_string_uuid(node: &SchemaNode) -> bool {
    matches!(
        node,
        SchemaNode::Logical {
            kind: LogicalType::Uuid,
            base,
            ..
        } if matches!(base.as_ref(), SchemaNode::String)
    )
}

/// The configured representation of `uuid` over `string`.
#[derive(Clone, Copy)]
pub(super) struct Uuid {
    setting: RustTypes,
}

impl Uuid {
    pub(super) fn new(setting: RustTypes) -> Self {
        Self { setting }
    }

    fn enabled(self) -> bool {
        self.setting == RustTypes::ApacheAvro
    }

    /// The bare type used in positions that can carry a field-level `serde`
    /// attribute.
    pub(super) fn direct_type(self, node: &SchemaNode) -> Option<TokenStream> {
        (self.enabled() && is_string_uuid(node)).then(|| quote!(apache_avro::Uuid))
    }

    /// The wrapper used in positions that cannot carry a field-level
    /// attribute, or `None` when the underlying type is used.
    pub(super) fn wrapper_for_position(
        self,
        node: &SchemaNode,
        direct: bool,
    ) -> Option<UuidWrapper> {
        (self.enabled() && !direct && is_string_uuid(node)).then_some(UuidWrapper { name: "Uuid" })
    }

    /// The display name of a `uuid` over `string`, used for generated union
    /// variants.
    pub(super) fn display_name(self, node: &SchemaNode) -> Option<&'static str> {
        (self.enabled() && is_string_uuid(node)).then_some("Uuid")
    }

    /// The field-level `serde(with = "...")` adapter of a schema.
    pub(super) fn serde_with(self, node: &SchemaNode) -> Option<SerdeWith> {
        if !self.enabled() {
            return None;
        }
        let (inner, optional) = direct_logical(node)?;
        if !is_string_uuid(inner) {
            return None;
        }
        Some(if optional {
            SerdeWith {
                path: "__avrox_uuid::uuid_option",
                helper: Some(Helper::UuidOption),
            }
        } else {
            SerdeWith {
                path: "__avrox_uuid::uuid",
                helper: Some(Helper::Uuid),
            }
        })
    }

    /// The `Uuid` newtypes the schema needs, in a deterministic order.
    pub(super) fn used_wrappers(self, schema: &Schema) -> Vec<UuidWrapper> {
        fn scan(node: &SchemaNode, direct: bool, found: &mut bool) {
            if is_string_uuid(node) {
                if !direct {
                    *found = true;
                }
                return;
            }
            match node {
                SchemaNode::Array(items) | SchemaNode::Map(items) => scan(items, false, found),
                SchemaNode::Union(branches) => {
                    for branch in branches {
                        scan(branch, direct, found);
                    }
                }
                SchemaNode::Logical { base, .. } => scan(base, direct, found),
                _ => {}
            }
        }
        if !self.enabled() {
            return Vec::new();
        }
        let mut found = false;
        scan(&schema.root, true, &mut found);
        for named in &schema.types {
            if let NamedType::Record(record) = named {
                for field in &record.fields {
                    scan(&field.schema, true, &mut found);
                }
            }
        }
        if found {
            vec![UuidWrapper { name: "Uuid" }]
        } else {
            Vec::new()
        }
    }

    /// Schema type names that collide with the generated `Uuid` wrapper.
    pub(super) fn reserved_names(self, schema: &Schema) -> Vec<&'static str> {
        self.used_wrappers(schema)
            .into_iter()
            .map(|wrapper| wrapper.name)
            .collect()
    }

    /// The serde helper module a generated file needs, if any.
    pub(super) fn helper_module(self, helpers: &BTreeSet<Helper>) -> Option<TokenStream> {
        if !self.enabled() || helpers.is_empty() {
            return None;
        }
        let uuid =
            (helpers.contains(&Helper::Uuid) || helpers.contains(&Helper::UuidOption)).then(uuid);
        let uuid_option = helpers.contains(&Helper::UuidOption).then(uuid_option);
        Some(quote! {
            #[allow(dead_code)]
            mod __avrox_uuid {
                #uuid
                #uuid_option
            }
        })
    }
}

/// A newtype generated for a `uuid` over `string` in positions where a
/// field-level `serde` attribute cannot be used, such as array items and map
/// values.
pub(super) struct UuidWrapper {
    /// Rust name of the newtype.
    pub(super) name: &'static str,
}

/// A serde adapter module a generated file may need for `uuid` fields.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Helper {
    Uuid,
    UuidOption,
}

/// A field-level `serde(with = "...")` adapter for a `uuid` logical type.
pub(super) struct SerdeWith {
    /// Path of the module containing `serialize` and `deserialize`.
    pub(super) path: &'static str,
    /// Generated helper module needed by the path, if any.
    pub(super) helper: Option<Helper>,
}

/// The logical type a schema carries at a field-like position, and whether it
/// is wrapped in a nullable union.
fn direct_logical(node: &SchemaNode) -> Option<(&SchemaNode, bool)> {
    match node {
        SchemaNode::Logical { .. } => Some((node, false)),
        SchemaNode::Union(branches) => match branches.as_slice() {
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

fn nullable_logical(node: &SchemaNode) -> Option<(&SchemaNode, bool)> {
    if matches!(node, SchemaNode::Logical { .. }) {
        Some((node, true))
    } else {
        None
    }
}

/// Renders the `Uuid` newtype with `serde` implementations that encode it as a
/// UUID string.
pub(super) fn render(wrapper: &UuidWrapper) -> TokenStream {
    let ident = naming::make_ident(wrapper.name).0;
    let doc = docs::attributes(None, vec![docs::logical_line(LogicalType::Uuid)]);
    quote! {
        #(#doc)*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct #ident(pub apache_avro::Uuid);

        impl serde::Serialize for #ident {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0.to_string())
            }
        }

        impl<'de> serde::Deserialize<'de> for #ident {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                apache_avro::Uuid::parse_str(&value)
                    .map(Self)
                    .map_err(|error| serde::de::Error::custom(error))
            }
        }

        impl From<apache_avro::Uuid> for #ident {
            fn from(value: apache_avro::Uuid) -> Self {
                Self(value)
            }
        }

        impl std::ops::Deref for #ident {
            type Target = apache_avro::Uuid;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    }
}

fn uuid() -> TokenStream {
    quote! {
        pub(crate) mod uuid {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &apache_avro::Uuid,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&value.to_string())
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<apache_avro::Uuid, D::Error> {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                apache_avro::Uuid::parse_str(&value)
                    .map_err(|error| serde::de::Error::custom(error))
            }
        }
    }
}

fn uuid_option() -> TokenStream {
    quote! {
        pub(crate) mod uuid_option {
            pub(crate) fn serialize<S: serde::Serializer>(
                value: &Option<apache_avro::Uuid>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(value) => serializer.serialize_some(&value.to_string()),
                    None => serializer.serialize_none(),
                }
            }

            pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<apache_avro::Uuid>, D::Error> {
                let value = <Option<String> as serde::Deserialize>::deserialize(deserializer)?;
                value
                    .map(|value| {
                        apache_avro::Uuid::parse_str(&value)
                            .map_err(|error| serde::de::Error::custom(error))
                    })
                    .transpose()
            }
        }
    }
}
