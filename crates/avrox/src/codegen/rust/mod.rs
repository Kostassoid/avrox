//! Rust code generation.
//!
//! Named types are rendered as structs, enums and type aliases; nullable
//! unions as `Option`; other unions as generated enums with one newtype
//! variant per branch, in branch order. The generated code integrates with
//! the [`apache-avro`](https://docs.rs/apache-avro) crate through `serde`
//! derives: `bytes` fields are `serde_bytes::ByteBuf` and `fixed` types are
//! `serde_bytes::ByteArray` aliases, as required by its schema-aware serde
//! implementation.
//!
//! References that recurse through directly nested positions are boxed so
//! that the generated types have finite size; nesting inside arrays, maps and
//! generated union enums never requires boxing on its own.
//!
//! Generation is split into four stages: [`model`] indexes the schema and
//! analyzes recursion, [`decorators`] applies option-specific type mappings,
//! [`render`] renders Rust items, and [`emit`] assembles the requested file
//! layout. Adding a representation option should normally only require a new
//! type decorator and, when necessary, a generated helper.

use crate::error::Result;
use crate::schema::Document;
use crate::{GeneratedFile, Layout, RustOptions};

mod decorators;
mod derives;
mod docs;
mod emit;
mod helpers;
mod model;
mod naming;
mod render;
mod time;
mod union;

#[cfg(test)]
mod tests;

/// Generates Rust source files for the schema document.
pub(super) fn generate(
    document: &Document,
    layout: Layout,
    options: RustOptions,
) -> Result<Vec<GeneratedFile>> {
    let model = model::Model::new(document, options)?;
    emit::render(&model, layout)
}
