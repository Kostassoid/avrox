//! Self-contained helper implementations emitted into generated code.

use proc_macro2::{Ident, Literal, TokenStream};
use quote::quote;

use crate::schema::{NamedType, Schema, SchemaNode};

/// The `Bytes` newtype generated in standard-types mode for `bytes` schemas.
pub(super) fn bytes_type() -> TokenStream {
    quote! {
        /// A byte sequence that serializes as Avro `bytes`.
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
        pub struct Bytes(pub Vec<u8>);

        impl serde::Serialize for Bytes {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_bytes(&self.0)
            }
        }

        impl<'de> serde::Deserialize<'de> for Bytes {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct BytesVisitor;

                impl<'de> serde::de::Visitor<'de> for BytesVisitor {
                    type Value = Bytes;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("a byte array")
                    }

                    fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                        Ok(Bytes(value.to_vec()))
                    }

                    fn visit_byte_buf<E: serde::de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
                        Ok(Bytes(value))
                    }

                    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                        let mut bytes = Vec::new();
                        while let Some(byte) = seq.next_element::<u8>()? {
                            bytes.push(byte);
                        }
                        Ok(Bytes(bytes))
                    }

                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        Ok(Bytes(value.as_bytes().to_vec()))
                    }
                }

                deserializer.deserialize_byte_buf(BytesVisitor)
            }
        }

        impl From<Vec<u8>> for Bytes {
            fn from(bytes: Vec<u8>) -> Self {
                Self(bytes)
            }
        }

        impl std::ops::Deref for Bytes {
            type Target = [u8];

            fn deref(&self) -> &[u8] {
                &self.0
            }
        }
    }
}

/// Whether the schema contains a `bytes` type.
pub(super) fn uses_bytes(schema: &Schema) -> bool {
    fn scan(node: &SchemaNode) -> bool {
        match node {
            SchemaNode::Bytes => true,
            SchemaNode::Array(items)
            | SchemaNode::Map(items)
            | SchemaNode::Logical { base: items, .. } => scan(items),
            SchemaNode::Union(branches) => branches.iter().any(scan),
            _ => false,
        }
    }
    scan(&schema.root)
        || schema.types.iter().any(|named| match named {
            NamedType::Record(record) => record.fields.iter().any(|field| scan(&field.schema)),
            NamedType::Enum(_) | NamedType::Fixed(_) => false,
        })
}

/// A `fixed` type as a newtype with `serde` implementations that serialize as
/// Avro bytes, used in standard-types mode.
pub(super) fn fixed_newtype(ident: &Ident, doc: &[TokenStream], size: u32) -> TokenStream {
    let array = Literal::usize_unsuffixed(size as usize);
    let expected = format!("a byte array of length {size}");
    quote! {
        #(#doc)*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct #ident(pub [u8; #array]);

        impl serde::Serialize for #ident {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_bytes(&self.0)
            }
        }

        impl<'de> serde::Deserialize<'de> for #ident {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct FixedVisitor;

                impl<'de> serde::de::Visitor<'de> for FixedVisitor {
                    type Value = #ident;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str(#expected)
                    }

                    fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                        <[u8; #array]>::try_from(value)
                            .map(#ident)
                            .map_err(|_| E::invalid_length(value.len(), &#expected))
                    }

                    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                        let mut bytes = Vec::with_capacity(#array);
                        while let Some(byte) = seq.next_element::<u8>()? {
                            bytes.push(byte);
                        }
                        <[u8; #array]>::try_from(bytes.as_slice())
                            .map(#ident)
                            .map_err(|_| serde::de::Error::invalid_length(bytes.len(), &#expected))
                    }

                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        self.visit_bytes(value.as_bytes())
                    }
                }

                deserializer.deserialize_byte_buf(FixedVisitor)
            }
        }
    }
}
