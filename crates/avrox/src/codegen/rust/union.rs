//! Union classification, identity and display naming.

use std::collections::HashMap;

use crate::schema::{FullName, SchemaNode};

use super::time::Time;
use super::uuid::Uuid as UuidSupport;

/// A union registered while walking the schema.
pub(super) struct UnionInfo {
    /// Rust type name, or `None` when the union is rendered inline as the
    /// branch itself or as an `Option`.
    pub(super) name: Option<String>,
    /// Branch schemas in declaration order.
    pub(super) branches: Vec<SchemaNode>,
}

/// How a union is rendered in Rust.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UnionRendering {
    /// A single branch: rendered as the branch itself.
    Transparent,
    /// `null` plus one other branch: rendered as an `Option`.
    Nullable {
        /// Index of the non-null branch.
        other: usize,
    },
    /// A generated enum with one variant per branch.
    Enum,
}

/// A structural key identifying a union, used to deduplicate equal unions.
pub(super) fn key(branches: &[SchemaNode]) -> String {
    branches
        .iter()
        .map(SchemaNode::union_branch_key)
        .collect::<Vec<_>>()
        .join("|")
}

/// Determines how a union is rendered in Rust.
pub(super) fn rendering(branches: &[SchemaNode]) -> UnionRendering {
    if branches.len() == 1 {
        return UnionRendering::Transparent;
    }
    if branches.len() == 2 {
        let null_first = matches!(branches[0], SchemaNode::Null);
        let null_second = matches!(branches[1], SchemaNode::Null);
        if null_first != null_second {
            return UnionRendering::Nullable {
                other: usize::from(null_first),
            };
        }
    }
    UnionRendering::Enum
}

/// The display name of a union branch, used for generated enum and variant
/// names.
pub(super) fn display_name(
    node: &SchemaNode,
    type_names: &HashMap<FullName, String>,
    time: Time,
    uuid: UuidSupport,
) -> String {
    if let Some(name) = uuid.display_name(node) {
        return name.to_owned();
    }
    match node {
        SchemaNode::Null => "Null".to_owned(),
        SchemaNode::Boolean => "Boolean".to_owned(),
        SchemaNode::Int => "Int".to_owned(),
        SchemaNode::Long => "Long".to_owned(),
        SchemaNode::Float => "Float".to_owned(),
        SchemaNode::Double => "Double".to_owned(),
        SchemaNode::Bytes => "Bytes".to_owned(),
        SchemaNode::String => "String".to_owned(),
        SchemaNode::Record(name) | SchemaNode::Enum(name) | SchemaNode::Fixed(name) => {
            type_names[name].clone()
        }
        SchemaNode::Array(_) => "Array".to_owned(),
        SchemaNode::Map(_) => "Map".to_owned(),
        SchemaNode::Union(_) => "Union".to_owned(),
        SchemaNode::Logical { kind, base, .. } => match time.display_name(*kind) {
            Some(name) => name.to_owned(),
            None => display_name(base, type_names, time, uuid),
        },
    }
}

pub(super) fn display_names(
    branches: &[SchemaNode],
    type_names: &HashMap<FullName, String>,
    time: Time,
    uuid: UuidSupport,
) -> Vec<String> {
    branches
        .iter()
        .map(|branch| display_name(branch, type_names, time, uuid))
        .collect()
}
