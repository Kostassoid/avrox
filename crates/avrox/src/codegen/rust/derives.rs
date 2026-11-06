//! Additional derives requested through [`RustOptions::derives`].
//!
//! Settings hold plain strings so callers do not need to depend on `syn`.
//! They are parsed into paths once, then merged with the derives the generator
//! emits itself, ignoring duplicates by their final name.

use std::collections::HashSet;

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

use crate::error::{Error, Result};

/// A parsed additional derive.
pub(super) struct Derive {
    tokens: TokenStream,
    name: String,
}

/// Parses the configured derive names or paths.
pub(super) fn parse(derives: &[String]) -> Result<Vec<Derive>> {
    derives
        .iter()
        .map(|derive| {
            let path = syn::parse_str::<syn::Path>(derive)
                .map_err(|error| Error::Setting(format!("invalid derive `{derive}`: {error}")))?;
            let tokens = derive
                .parse::<TokenStream>()
                .map_err(|error| Error::Setting(format!("invalid derive `{derive}`: {error}")))?;
            let name = path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                .unwrap_or_default();
            Ok(Derive { tokens, name })
        })
        .collect()
}

/// Renders a `derive` attribute containing the generator's own derives
/// followed by the additional ones, without duplicates.
pub(super) fn attribute(base: &[&str], extra: &[Derive]) -> TokenStream {
    let mut names = HashSet::with_capacity(base.len() + extra.len());
    let mut derives = Vec::with_capacity(base.len() + extra.len());
    for name in base {
        if names.insert((*name).to_owned()) {
            let ident = Ident::new(name, Span::call_site());
            derives.push(quote!(#ident));
        }
    }
    for derive in extra {
        if names.insert(derive.name.clone()) {
            let tokens = &derive.tokens;
            derives.push(quote!(#tokens));
        }
    }
    quote!(#[derive(#(#derives),*)])
}
