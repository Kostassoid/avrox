//! Rendering shared by all time wrapper newtypes.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::super::{docs, naming};
use super::TimeWrapper;

/// Renders a time wrapper newtype with `serde` implementations that encode it
/// as the Avro integer of its logical type.
pub(in crate::codegen::rust) fn render(wrapper: &TimeWrapper) -> TokenStream {
    let ident = naming::make_ident(wrapper.name).0;
    let doc = docs::attributes(None, vec![docs::logical_line(wrapper.kind)]);
    let inner = &wrapper.inner;
    let expecting = &wrapper.expecting;
    let serialize = &wrapper.serialize;
    let deserialize = &wrapper.deserialize;
    let wire = wrapper.wire.ty();
    let serialize_fn = format_ident!("serialize_{}", wrapper.wire.suffix());
    let deserialize_fn = format_ident!("deserialize_{}", wrapper.wire.suffix());
    let visit_fn = format_ident!("visit_{}", wrapper.wire.suffix());
    quote! {
        #(#doc)*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct #ident(pub #inner);

        impl serde::Serialize for #ident {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.#serialize_fn(
                    (#serialize).ok_or_else(|| serde::ser::Error::custom("value out of range"))?,
                )
            }
        }

        impl<'de> serde::Deserialize<'de> for #ident {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = #ident;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str(#expecting)
                    }

                    fn #visit_fn<E: serde::de::Error>(self, v: #wire) -> Result<Self::Value, E> {
                        (#deserialize)
                            .map(#ident)
                            .ok_or_else(|| E::custom("value out of range"))
                    }
                }

                deserializer.#deserialize_fn(Visitor)
            }
        }

        impl From<#inner> for #ident {
            fn from(value: #inner) -> Self {
                Self(value)
            }
        }

        impl std::ops::Deref for #ident {
            type Target = #inner;

            fn deref(&self) -> &#inner {
                &self.0
            }
        }
    }
}
