//! Union classification, identity and display naming.

use std::collections::HashMap;

use crate::RustTime;
use crate::schema::{FullName, Schema};

use super::time;

/// A union registered while walking the schema.
pub(super) struct UnionInfo {
    /// Rust type name, or `None` when the union is rendered inline as the
    /// branch itself or as an `Option`.
    pub(super) name: Option<String>,
    /// Branch schemas in declaration order.
    pub(super) branches: Vec<Schema>,
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
pub(super) fn key(branches: &[Schema]) -> String {
    branches
        .iter()
        .map(Schema::union_branch_key)
        .collect::<Vec<_>>()
        .join("|")
}

/// Determines how a union is rendered in Rust.
pub(super) fn rendering(branches: &[Schema]) -> UnionRendering {
    if branches.len() == 1 {
        return UnionRendering::Transparent;
    }
    if branches.len() == 2 {
        let null_first = matches!(branches[0], Schema::Null);
        let null_second = matches!(branches[1], Schema::Null);
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
    schema: &Schema,
    type_names: &HashMap<FullName, String>,
    time_setting: RustTime,
) -> String {
    match schema {
        Schema::Null => "Null".to_owned(),
        Schema::Boolean => "Boolean".to_owned(),
        Schema::Int => "Int".to_owned(),
        Schema::Long => "Long".to_owned(),
        Schema::Float => "Float".to_owned(),
        Schema::Double => "Double".to_owned(),
        Schema::Bytes => "Bytes".to_owned(),
        Schema::String => "String".to_owned(),
        Schema::Record(name) | Schema::Enum(name) | Schema::Fixed(name) => type_names[name].clone(),
        Schema::Array(_) => "Array".to_owned(),
        Schema::Map(_) => "Map".to_owned(),
        Schema::Union(_) => "Union".to_owned(),
        Schema::Logical { kind, base, .. } => match time::wrapper(time_setting, *kind) {
            Some(wrapper) => wrapper.name.to_owned(),
            None => display_name(base, type_names, time_setting),
        },
    }
}

pub(super) fn display_names(
    branches: &[Schema],
    type_names: &HashMap<FullName, String>,
    time_setting: RustTime,
) -> Vec<String> {
    branches
        .iter()
        .map(|branch| display_name(branch, type_names, time_setting))
        .collect()
}
