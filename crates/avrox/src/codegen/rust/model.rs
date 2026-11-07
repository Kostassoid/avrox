//! Schema indexing and recursion analysis.
//!
//! This module deliberately contains no rendering logic. It gives the rest of
//! the generator a stable, indexed view of the schema.

use std::collections::{HashMap, HashSet};

use proc_macro2::Ident;

use crate::error::{Error, Result};
use crate::schema::{FullName, NamedType, Schema, SchemaNode};
use crate::{RustOptions, RustTypes};

use super::union::{UnionInfo, UnionRendering};
use super::{derives, naming, time, union, uuid};

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

/// An indexed schema and its Rust-specific analysis.
pub(super) struct Model<'a> {
    pub(super) schema: &'a Schema,
    pub(super) options: RustOptions,
    pub(super) time: time::Time,
    pub(super) uuid: uuid::Uuid,
    pub(super) derives: Vec<derives::Derive>,
    pub(super) type_names: HashMap<FullName, String>,
    pub(super) unions: Vec<UnionInfo>,
    union_ids: HashMap<String, usize>,
    edges: HashMap<Node, Vec<Node>>,
}

impl<'a> Model<'a> {
    pub(super) fn new(schema: &'a Schema, options: RustOptions) -> Result<Self> {
        let derives = derives::parse(&options.derives)?;
        let time = time::Time::new(options.time);
        let uuid = uuid::Uuid::new(options.types);
        let mut model = Self {
            schema,
            options,
            time,
            uuid,
            derives,
            type_names: HashMap::new(),
            unions: Vec::new(),
            union_ids: HashMap::new(),
            edges: HashMap::new(),
        };
        model.collect_type_names()?;
        model.walk(&schema.root, None);
        for named in &schema.types {
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
        for named in &self.schema.types {
            let rust_name = naming::to_upper_camel_case(&named.name().name);
            if RESERVED_NAMES.contains(&rust_name.as_str())
                || (self.options.types == RustTypes::Std && rust_name == "Bytes")
                || self
                    .time
                    .reserved_names(self.schema)
                    .contains(&rust_name.as_str())
                || self
                    .uuid
                    .reserved_names(self.schema)
                    .contains(&rust_name.as_str())
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
    fn walk(&mut self, node: &SchemaNode, container: Option<Node>) {
        match node {
            SchemaNode::Record(name) | SchemaNode::Enum(name) | SchemaNode::Fixed(name) => {
                if let Some(container) = container {
                    self.add_edge(container, Node::Type(name.clone()));
                }
            }
            SchemaNode::Array(items) | SchemaNode::Map(items) => self.walk(items, None),
            SchemaNode::Union(branches) => {
                let id = self.register_union(branches);
                if let Some(container) = container {
                    self.add_edge(container, Node::Union(id));
                }
                for branch in branches {
                    self.walk(branch, Some(Node::Union(id)));
                }
            }
            SchemaNode::Logical { base, .. } => self.walk(base, container),
            SchemaNode::Null
            | SchemaNode::Boolean
            | SchemaNode::Int
            | SchemaNode::Long
            | SchemaNode::Float
            | SchemaNode::Double
            | SchemaNode::Bytes
            | SchemaNode::String => {}
        }
    }

    fn add_edge(&mut self, from: Node, to: Node) {
        self.edges.entry(from).or_default().push(to);
    }

    /// Registers a union, deduplicating structurally equal ones and naming
    /// the ones rendered as generated enums.
    fn register_union(&mut self, branches: &[SchemaNode]) -> usize {
        let key = union::key(branches);
        if let Some(id) = self.union_ids.get(&key) {
            return *id;
        }
        let name = match union::rendering(branches) {
            UnionRendering::Enum => {
                let base = union::display_names(branches, &self.type_names, self.time, self.uuid)
                    .join("Or");
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

    pub(super) fn union_id(&self, branches: &[SchemaNode]) -> Option<usize> {
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
