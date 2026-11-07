//! Rendering of schema definitions and structural Rust types.

use std::collections::HashSet;

use proc_macro2::{Literal, TokenStream};
use quote::quote;

use crate::RustTypes;
use crate::error::{Error, Result};
use crate::schema::{Enum, Field, Fixed, FullName, NamedType, Record, Schema};

use super::decorators::Decorators;
use super::emit::{FileContext, Import};
use super::model::{Model, Node};
use super::union::UnionRendering;
use super::{derives, docs, helpers, naming, union};

/// Renders the indexed model. Option-specific leaf mappings are delegated to
/// decorators so this module only handles Rust's structural type rules.
pub(super) struct Renderer<'model, 'document> {
    model: &'model Model<'document>,
    decorators: Decorators,
}

impl<'model, 'document> Renderer<'model, 'document> {
    pub(super) fn new(model: &'model Model<'document>) -> Self {
        Self {
            model,
            decorators: Decorators::new(&model.options),
        }
    }

    pub(super) fn render_named(
        &self,
        named: &NamedType,
        context: &mut FileContext,
    ) -> Result<TokenStream> {
        match named {
            NamedType::Record(record) => self.render_record(record, context),
            NamedType::Enum(enum_) => Ok(self.render_enum(enum_, context)),
            NamedType::Fixed(fixed) => Ok(self.render_fixed(fixed, context)),
        }
    }

    fn render_record(&self, record: &Record, context: &mut FileContext) -> Result<TokenStream> {
        context.needs(Import::Serde);
        let (ident, _) = self.model.type_ident(&record.name);
        let fields = record
            .fields
            .iter()
            .map(|field| self.render_field(field, &record.name, context))
            .collect::<Result<Vec<_>>>()?;
        let doc = docs::attributes(record.doc.as_deref(), Vec::new());
        let derives = derives::attribute(
            &["Debug", "Clone", "PartialEq", "Serialize", "Deserialize"],
            &self.model.derives,
        );
        Ok(quote! {
            #(#doc)*
            #derives
            pub struct #ident {
                #(#fields)*
            }
        })
    }

    fn render_field(
        &self,
        field: &Field,
        record: &FullName,
        context: &mut FileContext,
    ) -> Result<TokenStream> {
        let snake = naming::to_snake_case(&field.name);
        let (ident, escaped) = naming::make_ident(&snake);
        let ty = self.render_type(
            &field.schema,
            Some(Node::Type(record.clone())),
            true,
            context,
        )?;
        let doc = docs::attributes(field.doc.as_deref(), docs::logical_lines(&field.schema));
        let with = self.serde_with(&field.schema, context);
        let name = &field.name;
        let rename = if escaped || snake != *name {
            quote! { #[serde(rename = #name)] }
        } else {
            quote! {}
        };
        Ok(quote! {
            #(#doc)*
            #rename
            #with
            pub #ident: #ty,
        })
    }

    /// The `serde(with = "...")` attribute a field needs for a logical type
    /// that the current settings render as a bare domain type.
    fn serde_with(&self, schema: &Schema, context: &mut FileContext) -> TokenStream {
        if let Some(with) = self.model.time.serde_with(schema) {
            if let Some(helper) = with.helper {
                context.needs_time_helper(helper);
            }
            let path = with.path;
            return quote! { #[serde(with = #path)] };
        }
        let Some(with) = self.model.uuid.serde_with(schema) else {
            return quote! {};
        };
        if let Some(helper) = with.helper {
            context.needs_uuid_helper(helper);
        }
        let path = with.path;
        quote! { #[serde(with = #path)] }
    }

    fn render_enum(&self, enum_: &Enum, context: &mut FileContext) -> TokenStream {
        context.needs(Import::Serde);
        let (ident, _) = self.model.type_ident(&enum_.name);
        let mut used: HashSet<String> = HashSet::new();
        let mut variants = Vec::with_capacity(enum_.symbols.len());
        for symbol in &enum_.symbols {
            let camel = naming::to_upper_camel_case(symbol);
            let variant_name = unique_variant_name(&camel, &mut used);
            let (variant, escaped) = naming::make_ident(&variant_name);
            let rename = if escaped || variant_name != *symbol {
                quote! { #[serde(rename = #symbol)] }
            } else {
                quote! {}
            };
            let default = if enum_.default.as_deref() == Some(symbol.as_str()) {
                quote! { #[default] }
            } else {
                quote! {}
            };
            variants.push(quote! {
                #rename
                #default
                #variant,
            });
        }
        let mut base = vec!["Debug", "Clone", "Copy", "PartialEq", "Eq", "Hash"];
        if enum_.default.is_some() {
            base.push("Default");
        }
        base.extend(["Serialize", "Deserialize"]);
        let derives = derives::attribute(&base, &self.model.derives);
        let doc = docs::attributes(enum_.doc.as_deref(), Vec::new());
        quote! {
            #(#doc)*
            #derives
            pub enum #ident {
                #(#variants)*
            }
        }
    }

    fn render_fixed(&self, fixed: &Fixed, context: &mut FileContext) -> TokenStream {
        let (ident, _) = self.model.type_ident(&fixed.name);
        let doc = docs::attributes(fixed.doc.as_deref(), docs::fixed_logical_lines(fixed));
        match self.model.options.types {
            RustTypes::ApacheAvro => {
                context.needs(Import::ByteArray);
                let size = Literal::usize_unsuffixed(fixed.size as usize);
                quote! {
                    #(#doc)*
                    pub type #ident = ByteArray<#size>;
                }
            }
            RustTypes::Std => helpers::fixed_newtype(&ident, &doc, fixed.size),
        }
    }

    pub(super) fn render_union_enum(
        &self,
        id: usize,
        context: &mut FileContext,
    ) -> Result<TokenStream> {
        context.needs(Import::Serde);
        let union_info = &self.model.unions[id];
        let Some(name) = &union_info.name else {
            return Err(Error::Internal("inline union has no name".to_owned()));
        };
        let (ident, _) = naming::make_ident(name);
        let mut used: HashSet<String> = HashSet::new();
        let mut variants = Vec::with_capacity(union_info.branches.len());
        for branch in &union_info.branches {
            let display = union::display_name(
                branch,
                &self.model.type_names,
                self.model.time,
                self.model.uuid,
            );
            let variant_name = unique_variant_name(&display, &mut used);
            let (variant, _) = naming::make_ident(&variant_name);
            let ty = self.render_type(branch, Some(Node::Union(id)), true, context)?;
            let with = self.serde_with(branch, context);
            variants.push(quote! {
                #with
                #variant(#ty),
            });
        }
        let doc = format!(
            " Avro union of {}.",
            union::display_names(
                &union_info.branches,
                &self.model.type_names,
                self.model.time,
                self.model.uuid,
            )
            .into_iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
        );
        let doc_attributes = vec![quote! { #[doc = #doc] }];
        let derives = derives::attribute(
            &["Debug", "Clone", "PartialEq", "Serialize", "Deserialize"],
            &self.model.derives,
        );
        Ok(quote! {
            #(#doc_attributes)*
            #derives
            pub enum #ident {
                #(#variants)*
            }
        })
    }

    /// Renders the Rust type for a schema at a position nested inside
    /// `container`, which is `None` in indirection positions such as array
    /// items and map values. `direct` records whether the position can carry a
    /// field-level `serde` attribute.
    fn render_type(
        &self,
        schema: &Schema,
        container: Option<Node>,
        direct: bool,
        context: &mut FileContext,
    ) -> Result<TokenStream> {
        if let Some(tokens) = self.decorators.apply(schema, self.model, context, direct) {
            return Ok(tokens);
        }
        match schema {
            Schema::Null => Ok(quote!(())),
            Schema::Boolean => Ok(quote!(bool)),
            Schema::Int => Ok(quote!(i32)),
            Schema::Long => Ok(quote!(i64)),
            Schema::Float => Ok(quote!(f32)),
            Schema::Double => Ok(quote!(f64)),
            Schema::String => Ok(quote!(String)),
            Schema::Record(name) | Schema::Enum(name) | Schema::Fixed(name) => {
                Ok(self.render_named_ref(name, container.as_ref(), context))
            }
            Schema::Array(items) => {
                let items = self.render_type(items, None, false, context)?;
                Ok(quote!(Vec<#items>))
            }
            Schema::Map(values) => {
                context.needs(Import::HashMap);
                let values = self.render_type(values, None, false, context)?;
                Ok(quote!(HashMap<String, #values>))
            }
            Schema::Union(branches) => self.render_union(branches, container, direct, context),
            Schema::Logical { base, .. } => self.render_type(base, container, direct, context),
            Schema::Bytes => Err(Error::Internal(
                "bytes schema was not handled by a representation decorator".to_owned(),
            )),
        }
    }

    /// A reference to a generated named type, boxed when it recurses through
    /// directly nested positions.
    fn render_named_ref(
        &self,
        name: &FullName,
        container: Option<&Node>,
        context: &mut FileContext,
    ) -> TokenStream {
        let rust_name = &self.model.type_names[name];
        context.reference_unless_own(rust_name);
        let (ident, _) = naming::make_ident(rust_name);
        let boxed = container.is_some_and(|container| self.model.needs_box(container, name));
        if boxed {
            quote!(Box<#ident>)
        } else {
            quote!(#ident)
        }
    }

    fn render_union(
        &self,
        branches: &[Schema],
        container: Option<Node>,
        direct: bool,
        context: &mut FileContext,
    ) -> Result<TokenStream> {
        let key = union::key(branches);
        let Some(id) = self.model.union_id(branches) else {
            return Err(Error::Internal(format!("union `{key}` was not registered")));
        };
        match union::rendering(branches) {
            UnionRendering::Transparent => {
                self.render_type(&branches[0], container, direct, context)
            }
            UnionRendering::Nullable { other } => {
                let inner =
                    self.render_type(&branches[other], Some(Node::Union(id)), direct, context)?;
                Ok(quote!(Option<#inner>))
            }
            UnionRendering::Enum => {
                let name = self.model.unions[id].name.as_deref();
                let Some(name) = name else {
                    return Err(Error::Internal("union enum has no name".to_owned()));
                };
                context.reference_unless_own(name);
                let (ident, _) = naming::make_ident(name);
                Ok(quote!(#ident))
            }
        }
    }
}

/// Derives a unique enum variant name from a candidate.
fn unique_variant_name(candidate: &str, used: &mut HashSet<String>) -> String {
    if used.insert(candidate.to_owned()) {
        return candidate.to_owned();
    }
    let mut suffix = 2;
    while !used.insert(format!("{candidate}{suffix}")) {
        suffix += 1;
    }
    format!("{candidate}{suffix}")
}
