//! Schema indexing, Go naming and recursion analysis.

use std::collections::{HashMap, HashSet};

use crate::GoOptions;
use crate::error::{Error, Result};
use crate::schema::{Enum, Field, FullName, LogicalType, NamedType, Record, Schema, SchemaNode};

use super::naming;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UnionRendering {
    Transparent,
    Nullable { other: usize },
    Struct,
}

pub(super) struct UnionInfo {
    pub(super) name: Option<String>,
    pub(super) field_names: Vec<String>,
    pub(super) branches: Vec<SchemaNode>,
}

pub(super) struct Model<'a> {
    pub(super) schema: &'a Schema,
    pub(super) package: String,
    pub(super) type_names: HashMap<FullName, String>,
    field_names: HashMap<(FullName, String), String>,
    pub(super) unions: Vec<UnionInfo>,
    union_ids: HashMap<String, usize>,
}

impl<'a> Model<'a> {
    pub(super) fn new(schema: &'a Schema, options: &GoOptions) -> Result<Self> {
        let root_namespace = match &schema.root {
            SchemaNode::Record(name) | SchemaNode::Enum(name) | SchemaNode::Fixed(name) => {
                name.namespace.as_deref()
            }
            _ => None,
        };
        let package = options
            .package
            .clone()
            .unwrap_or_else(|| naming::default_package(root_namespace));
        if !naming::valid_package(&package) {
            return Err(Error::Setting(format!(
                "`{package}` is not a valid Go package name"
            )));
        }
        let mut model = Self {
            schema,
            package,
            type_names: HashMap::new(),
            field_names: HashMap::new(),
            unions: Vec::new(),
            union_ids: HashMap::new(),
        };
        model.collect_names()?;
        model.walk(&schema.root);
        for named in &schema.types {
            if let NamedType::Record(record) = named {
                for field in &record.fields {
                    model.walk(&field.schema);
                }
            }
        }
        model.validate_declarations()?;
        Ok(model)
    }

    fn collect_names(&mut self) -> Result<()> {
        let mut used = HashSet::new();
        for named in &self.schema.types {
            let go_name = naming::exported(&named.name().name);
            if !used.insert(go_name.clone()) {
                return Err(Error::Unsupported(format!(
                    "multiple types map to Go type name `{go_name}` (e.g. `{}`); generating \
                     types with colliding names from different namespaces is not supported yet",
                    named.name()
                )));
            }
            self.type_names.insert(named.name().clone(), go_name);
        }
        for named in &self.schema.types {
            if let NamedType::Record(record) = named {
                let mut fields = HashSet::new();
                for field in &record.fields {
                    let go_name = naming::exported(&field.name);
                    if ["WriteAvro", "ReadAvro", "MarshalAvro", "UnmarshalAvro"]
                        .contains(&go_name.as_str())
                    {
                        return Err(Error::Unsupported(format!(
                            "field `{}` of record `{}` maps to generated Go method `{go_name}`",
                            field.name, record.name
                        )));
                    }
                    if !fields.insert(go_name.clone()) {
                        return Err(Error::Unsupported(format!(
                            "multiple fields of record `{}` map to Go field name `{go_name}`",
                            record.name
                        )));
                    }
                    self.field_names
                        .insert((record.name.clone(), field.name.clone()), go_name);
                }
            }
        }
        Ok(())
    }

    fn walk(&mut self, node: &SchemaNode) {
        match node {
            SchemaNode::Array(items)
            | SchemaNode::Map(items)
            | SchemaNode::Logical { base: items, .. } => {
                self.walk(items);
            }
            SchemaNode::Union(branches) => {
                self.register_union(branches);
                for branch in branches {
                    self.walk(branch);
                }
            }
            SchemaNode::Null
            | SchemaNode::Boolean
            | SchemaNode::Int
            | SchemaNode::Long
            | SchemaNode::Float
            | SchemaNode::Double
            | SchemaNode::Bytes
            | SchemaNode::String
            | SchemaNode::Record(_)
            | SchemaNode::Enum(_)
            | SchemaNode::Fixed(_) => {}
        }
    }

    fn register_union(&mut self, branches: &[SchemaNode]) {
        let key = union_key(branches);
        if self.union_ids.contains_key(&key) {
            return;
        }
        let name = if rendering(branches) == UnionRendering::Struct {
            let base = branches
                .iter()
                .map(|branch| display_name(branch, &self.type_names))
                .collect::<Vec<_>>()
                .join("Or");
            let mut taken: HashSet<String> = self.type_names.values().cloned().collect();
            taken.extend(self.unions.iter().filter_map(|union| union.name.clone()));
            Some(naming::unique(&base, &mut taken))
        } else {
            None
        };
        let mut used: HashSet<String> = [
            "Validate",
            "WriteAvro",
            "ReadAvro",
            "MarshalAvro",
            "UnmarshalAvro",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let field_names = branches
            .iter()
            .map(|branch| naming::unique(&display_name(branch, &self.type_names), &mut used))
            .collect();
        let id = self.unions.len();
        self.unions.push(UnionInfo {
            name,
            field_names,
            branches: branches.to_vec(),
        });
        self.union_ids.insert(key, id);
    }

    pub(super) fn type_name(&self, name: &FullName) -> &str {
        &self.type_names[name]
    }

    pub(super) fn field_name(&self, record: &FullName, field: &Field) -> &str {
        &self.field_names[&(record.clone(), field.name.clone())]
    }

    pub(super) fn union_id(&self, branches: &[SchemaNode]) -> usize {
        self.union_ids[&union_key(branches)]
    }

    pub(super) fn has_output(&self) -> bool {
        !self.schema.types.is_empty() || self.unions.iter().any(|union| union.name.is_some())
    }

    pub(super) fn record(&self, name: &FullName) -> &Record {
        match self.schema.get(name) {
            Some(NamedType::Record(record)) => record,
            _ => unreachable!("record references are resolved by the schema parser"),
        }
    }

    pub(super) fn enum_(&self, name: &FullName) -> &Enum {
        match self.schema.get(name) {
            Some(NamedType::Enum(enum_)) => enum_,
            _ => unreachable!("enum references are resolved by the schema parser"),
        }
    }

    pub(super) fn symbol_names(&self, enum_: &Enum) -> Vec<String> {
        let type_name = self.type_name(&enum_.name);
        let mut used = HashSet::new();
        enum_
            .symbols
            .iter()
            .map(|symbol| {
                naming::unique(
                    &format!("{type_name}{}", naming::exported(symbol)),
                    &mut used,
                )
            })
            .collect()
    }

    fn validate_declarations(&self) -> Result<()> {
        let mut used: HashSet<String> = self.type_names.values().cloned().collect();
        used.extend(self.unions.iter().filter_map(|union| union.name.clone()));
        for named in &self.schema.types {
            match named {
                NamedType::Record(record) => reserve_declaration(
                    &mut used,
                    &format!("New{}", self.type_name(&record.name)),
                    &record.name.full(),
                )?,
                NamedType::Enum(enum_) => {
                    for symbol in self.symbol_names(enum_) {
                        reserve_declaration(&mut used, &symbol, &enum_.name.full())?;
                    }
                }
                NamedType::Fixed(_) => {}
            }
        }
        for union in &self.unions {
            let Some(name) = &union.name else {
                continue;
            };
            for field in &union.field_names {
                reserve_declaration(
                    &mut used,
                    &format!("New{name}From{field}"),
                    "generated union constructors",
                )?;
            }
        }
        Ok(())
    }

    /// Whether a direct field of `owner` needs a pointer to break a record
    /// cycle. Arrays, maps and all generated union structs supply indirection.
    pub(super) fn needs_pointer(&self, owner: &FullName, target: &FullName) -> bool {
        let mut stack = vec![target.clone()];
        let mut visited = HashSet::new();
        while let Some(current) = stack.pop() {
            if current == *owner {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            let Some(NamedType::Record(record)) = self.schema.get(&current) else {
                continue;
            };
            for field in &record.fields {
                collect_direct_references(&field.schema, &mut stack);
            }
        }
        false
    }
}

fn reserve_declaration(used: &mut HashSet<String>, name: &str, owner: &str) -> Result<()> {
    if used.insert(name.to_owned()) {
        Ok(())
    } else {
        Err(Error::Unsupported(format!(
            "Go declaration `{name}` generated for `{owner}` collides with another declaration"
        )))
    }
}

fn collect_direct_references(node: &SchemaNode, output: &mut Vec<FullName>) {
    match node {
        SchemaNode::Record(name) => output.push(name.clone()),
        SchemaNode::Logical { base, .. } => collect_direct_references(base, output),
        SchemaNode::Union(branches) if branches.len() == 1 => {
            collect_direct_references(&branches[0], output);
        }
        _ => {}
    }
}

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
    UnionRendering::Struct
}

fn union_key(branches: &[SchemaNode]) -> String {
    branches
        .iter()
        .map(go_schema_key)
        .collect::<Vec<_>>()
        .join("|")
}

fn go_schema_key(node: &SchemaNode) -> String {
    match node {
        SchemaNode::Record(name) => format!("record:{name}"),
        SchemaNode::Enum(name) => format!("enum:{name}"),
        SchemaNode::Fixed(name) => format!("fixed:{name}"),
        SchemaNode::Array(items) => format!("array<{}>", go_schema_key(items)),
        SchemaNode::Map(values) => format!("map<{}>", go_schema_key(values)),
        SchemaNode::Union(branches) => format!("union<{}>", union_key(branches)),
        SchemaNode::Logical { base, kind } => {
            format!("logical:{}<{}>", kind.avro_name(), go_schema_key(base))
        }
        other => other.avro_type_name().to_owned(),
    }
}

pub(super) fn display_name(node: &SchemaNode, type_names: &HashMap<FullName, String>) -> String {
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
        SchemaNode::Logical { base, kind } => match kind {
            LogicalType::Date => "Date".to_owned(),
            LogicalType::TimeMillis => "TimeMillis".to_owned(),
            LogicalType::TimeMicros => "TimeMicros".to_owned(),
            LogicalType::TimestampMillis => "TimestampMillis".to_owned(),
            LogicalType::TimestampMicros => "TimestampMicros".to_owned(),
            LogicalType::TimestampNanos => "TimestampNanos".to_owned(),
            LogicalType::LocalTimestampMillis => "LocalTimestampMillis".to_owned(),
            LogicalType::LocalTimestampMicros => "LocalTimestampMicros".to_owned(),
            LogicalType::LocalTimestampNanos => "LocalTimestampNanos".to_owned(),
            _ => display_name(base, type_names),
        },
    }
}
