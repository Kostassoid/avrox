//! Option-specific decorations of schema-to-Rust type mappings.
//!
//! Decorators run before the structural renderer and may replace a leaf type.
//! They form a small chain of responsibility: time wrappers get first choice,
//! followed by the selected representation. Returning `None` leaves the schema
//! untouched so the next decorator or the base renderer can handle it.

use proc_macro2::TokenStream;
use quote::quote;

use crate::schema::{FullName, LogicalType, NamedType, Schema, SchemaNode};
use crate::{RustOptions, RustTypes};

use super::emit::{FileContext, Import};
use super::model::Model;
use super::{naming, time, uuid};

/// An optional, option-specific replacement for a schema's Rust type.
trait TypeDecorator {
    fn decorate(
        &self,
        node: &SchemaNode,
        model: &Model<'_>,
        context: &mut FileContext,
        direct: bool,
    ) -> Option<TokenStream>;
}

/// The ordered decorators used by a renderer.
pub(super) struct Decorators {
    inner: Vec<Box<dyn TypeDecorator>>,
}

impl Decorators {
    pub(super) fn new(options: &RustOptions) -> Self {
        let inner: Vec<Box<dyn TypeDecorator>> = vec![
            Box::new(TimeDecorator {
                time: time::Time::new(options.time),
            }),
            Box::new(UuidDecorator {
                uuid: uuid::Uuid::new(options.types),
            }),
            Box::new(RepresentationDecorator {
                setting: options.types,
            }),
        ];
        Self { inner }
    }

    pub(super) fn apply(
        &self,
        node: &SchemaNode,
        model: &Model<'_>,
        context: &mut FileContext,
        direct: bool,
    ) -> Option<TokenStream> {
        for decorator in &self.inner {
            if let Some(tokens) = decorator.decorate(node, model, context, direct) {
                return Some(tokens);
            }
        }
        None
    }
}

/// Replaces time-related logical types with their configured representation.
struct TimeDecorator {
    time: time::Time,
}

impl TypeDecorator for TimeDecorator {
    fn decorate(
        &self,
        node: &SchemaNode,
        _model: &Model<'_>,
        context: &mut FileContext,
        direct: bool,
    ) -> Option<TokenStream> {
        let SchemaNode::Logical { kind, .. } = node else {
            return None;
        };
        if direct && let Some(tokens) = self.time.direct_type(*kind) {
            return Some(tokens);
        }
        let wrapper = self.time.wrapper_for_position(*kind, false)?;
        context.reference(wrapper.name);
        let (ident, _) = naming::make_ident(wrapper.name);
        Some(quote!(#ident))
    }
}

/// Replaces `uuid` over `string` with a type that serializes as a string.
struct UuidDecorator {
    uuid: uuid::Uuid,
}

impl TypeDecorator for UuidDecorator {
    fn decorate(
        &self,
        node: &SchemaNode,
        _model: &Model<'_>,
        context: &mut FileContext,
        direct: bool,
    ) -> Option<TokenStream> {
        if direct && let Some(tokens) = self.uuid.direct_type(node) {
            return Some(tokens);
        }
        let wrapper = self.uuid.wrapper_for_position(node, direct)?;
        context.reference(wrapper.name);
        let (ident, _) = naming::make_ident(wrapper.name);
        Some(quote!(#ident))
    }
}

/// Applies representation-specific byte and logical type mappings.
struct RepresentationDecorator {
    setting: RustTypes,
}

impl RepresentationDecorator {
    fn bytes(&self, context: &mut FileContext) -> TokenStream {
        match self.setting {
            RustTypes::ApacheAvro => {
                context.needs(Import::ByteBuf);
                quote!(ByteBuf)
            }
            RustTypes::Std => {
                context.reference("Bytes");
                quote!(Bytes)
            }
        }
    }

    /// The `apache-avro` type a `fixed` with a logical type maps to, if any.
    fn apache_fixed_type(schema: &Schema, name: &FullName) -> Option<TokenStream> {
        let NamedType::Fixed(fixed) = schema.get(name)? else {
            return None;
        };
        match fixed.logical {
            Some(LogicalType::Uuid) => Some(quote!(apache_avro::Uuid)),
            Some(LogicalType::Decimal { .. }) => Some(quote!(apache_avro::Decimal)),
            Some(LogicalType::Duration) => Some(quote!(apache_avro::Duration)),
            _ => None,
        }
    }
}

impl TypeDecorator for RepresentationDecorator {
    fn decorate(
        &self,
        node: &SchemaNode,
        model: &Model<'_>,
        context: &mut FileContext,
        _direct: bool,
    ) -> Option<TokenStream> {
        match node {
            SchemaNode::Bytes => Some(self.bytes(context)),
            SchemaNode::Logical { kind, .. } if self.setting == RustTypes::ApacheAvro => {
                apache_logical_type(*kind)
            }
            SchemaNode::Fixed(name) if self.setting == RustTypes::ApacheAvro => {
                Self::apache_fixed_type(model.schema, name)
            }
            _ => None,
        }
    }
}

/// The `apache-avro` type a logical type maps to in [`RustTypes::ApacheAvro`],
/// or `None` when the underlying primitive is used.
///
/// `uuid` is handled by [`UuidDecorator`] over `string` and by
/// [`RepresentationDecorator::apache_fixed_type`] over `fixed`.
fn apache_logical_type(kind: LogicalType) -> Option<TokenStream> {
    match kind {
        LogicalType::Decimal { .. } | LogicalType::BigDecimal => Some(quote!(apache_avro::Decimal)),
        _ => None,
    }
}
