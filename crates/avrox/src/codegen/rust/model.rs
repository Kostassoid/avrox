//! Schema indexing and recursion analysis.
//!
//! This module deliberately contains no rendering logic. It gives the rest of
//! the generator a stable, indexed view of the document.

use std::collections::{HashMap, HashSet};

use proc_macro2::Ident;

use crate::error::{Error, Result};
use crate::schema::{Document, FullName, NamedType, Schema};
use crate::{RustOptions, RustTypes};

use super::union::{UnionInfo, UnionRendering};
use super::{naming, time, union};

/// Names used by the generated code itself; schema types mapping to them are
/// rejected as unsupported.
const RESERVED_NAMES: &[&str] = &[
    "std",
    "core",
    "serde",
    "serde_bytes",
    "Box",
    "Option",
    "Vec",
    "String",
    "HashMap",
    "Serialize",
    "Deserialize",
    "ByteBuf",
    "ByteArray",
    "bool",
    "u8",
    "i8",
    "u16",
    "i16",
    "i32",
    "u32",
    "i64",
    "u64",
    "f32",
    "f64",
    "usize",
    "isize",
    "Self",
];

/// A node of the nesting graph used to decide where `Box` is required.
///
/// An edge `A -> B` means a value of `B` is nested directly inside a value of
/// `A` without indirection: a named type referenced as a field type or union
/// branch, or a union used as a field type. Arrays and maps add indirection
/// and do not create edges.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum Node {
    /// A named type; only records have outgoing edges.
    Type(FullName),
    /// A distinct union, identified by its registration index.
    Union(usize),
}

/// An indexed schema document and its Rust-specific analysis.
pub(super) struct Model<'a> {
    pub(super) document: &'a Document,
    pub(super) options: RustOptions,
    pub(super) type_names: HashMap<FullName, String>,
    pub(super) unions: Vec<UnionInfo>,
    union_ids: HashMap<String, usize>,
    edges: HashMap<Node, Vec<Node>>,
}

impl<'a> Model<'a> {
    pub(super) fn new(document: &'a Document, options: RustOptions) -> Result<Self> {
        let mut model = Self {
            document,
            options,
            type_names: HashMap::new(),
            unions: Vec::new(),
            union_ids: HashMap::new(),
            edges: HashMap::new(),
        };
        model.collect_type_names()?;
        model.walk(&document.root, None);
        for named in &document.types {
            if let NamedType::Record(record) = named {
                for field in &record.fields {
                    model.walk(&field.schema, Some(Node::Type(record.name.clone())));
                }
            }
        }
        Ok(model)
    }

    /// Assigns a Rust name to every named type, rejecting collisions with
    /// each other and with names used by the generated code.
    fn collect_type_names(&mut self) -> Result<()> {
        let mut used: HashSet<String> = HashSet::new();
        for named in &self.document.types {
            let rust_name = naming::to_upper_camel_case(&named.name().name);
            if RESERVED_NAMES.contains(&rust_name.as_str())
                || (self.options.types == RustTypes::Std && rust_name == "Bytes")
                || time::reserved_names(self.options.time).contains(&rust_name.as_str())
            {
                return Err(Error::Unsupported(format!(
                    "type name `{rust_name}` (from type `{}`) collides with a name used by \
                     the generated code",
                    named.name()
                )));
            }
            if !used.insert(rust_name.clone()) {
                return Err(Error::Unsupported(format!(
                    "multiple types map to Rust type name `{rust_name}` (e.g. `{}`); \
                     generating types with colliding names from different namespaces is not \
                     supported yet",
                    named.name()
                )));
            }
            self.type_names.insert(named.name().clone(), rust_name);
        }
        Ok(())
    }

    /// Registers every union of the schema tree and the nesting graph edges
    /// of directly nested positions.
    fn walk(&mut self, schema: &Schema, container: Option<Node>) {
        match schema {
            Schema::Record(name) | Schema::Enum(name) | Schema::Fixed(name) => {
                if let Some(container) = container {
                    self.add_edge(container, Node::Type(name.clone()));
                }
            }
            Schema::Array(items) | Schema::Map(items) => self.walk(items, None),
            Schema::Union(branches) => {
                let id = self.register_union(branches);
                if let Some(container) = container {
                    self.add_edge(container, Node::Union(id));
                }
                for branch in branches {
                    self.walk(branch, Some(Node::Union(id)));
                }
            }
            Schema::Logical { base, .. } => self.walk(base, container),
            Schema::Null
            | Schema::Boolean
            | Schema::Int
            | Schema::Long
            | Schema::Float
            | Schema::Double
            | Schema::Bytes
            | Schema::String => {}
        }
    }

    fn add_edge(&mut self, from: Node, to: Node) {
        self.edges.entry(from).or_default().push(to);
    }

    /// Registers a union, deduplicating structurally equal ones and naming
    /// the ones rendered as generated enums.
    fn register_union(&mut self, branches: &[Schema]) -> usize {
        let key = union::key(branches);
        if let Some(id) = self.union_ids.get(&key) {
            return *id;
        }
        let name = match union::rendering(branches) {
            UnionRendering::Enum => {
                let base =
                    union::display_names(branches, &self.type_names, self.options.time).join("Or");
                Some(self.unique_name(&base))
            }
            _ => None,
        };
        let id = self.unions.len();
        self.unions.push(UnionInfo {
            name,
            branches: branches.to_vec(),
        });
        self.union_ids.insert(key, id);
        id
    }

    /// Derives a unique Rust name for a generated union enum.
    fn unique_name(&self, base: &str) -> String {
        let taken: HashSet<&str> = self
            .type_names
            .values()
            .map(String::as_str)
            .chain(self.unions.iter().filter_map(|union| union.name.as_deref()))
            .collect();
        let mut candidate = base.to_owned();
        let mut suffix = 1;
        while RESERVED_NAMES.contains(&candidate.as_str()) || taken.contains(candidate.as_str()) {
            suffix += 1;
            candidate = format!("{base}{suffix}");
        }
        candidate
    }

    /// The Rust identifier of a named type.
    pub(super) fn type_ident(&self, name: &FullName) -> (Ident, bool) {
        naming::make_ident(&self.type_names[name])
    }

    pub(super) fn union_id(&self, branches: &[Schema]) -> Option<usize> {
        self.union_ids.get(&union::key(branches)).copied()
    }

    pub(super) fn has_named_union(&self) -> bool {
        self.unions.iter().any(|union| union.name.is_some())
    }

    /// Whether a reference to `target` nested directly inside `container`
    /// recurses back into it and must be boxed.
    pub(super) fn needs_box(&self, container: &Node, target: &FullName) -> bool {
        let mut stack = vec![Node::Type(target.clone())];
        let mut visited = HashSet::new();
        while let Some(node) = stack.pop() {
            if node == *container {
                return true;
            }
            if !visited.insert(node.clone()) {
                continue;
            }
            if let Some(edges) = self.edges.get(&node) {
                stack.extend(edges.iter().cloned());
            }
        }
        false
    }
}
