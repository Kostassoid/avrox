//! Parsing of Avro schema JSON into the intermediate representation.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::{
    Enum, Field, Fixed, FullName, LogicalType, NamedType, Order, Record, Schema, SchemaNode,
};
use crate::AvroVersion;
use crate::error::{Error, Result, schema_error};

/// Names of the eight Avro primitive types.
const PRIMITIVE_NAMES: [&str; 8] = [
    "null", "boolean", "int", "long", "float", "double", "bytes", "string",
];

/// Names of the Avro complex types that must be declared as type objects.
const COMPLEX_NAMES: [&str; 5] = ["record", "enum", "array", "map", "fixed"];

/// Kind of a named type, used to resolve references.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NamedKind {
    Record,
    Enum,
    Fixed,
}

/// Parses the JSON text of an Avro schema (`.avsc`) into a [`Schema`].
///
/// Every named type defined in the schema is collected in definition order.
/// References are resolved while parsing, so all of them point at types
/// defined in the schema.
pub(super) fn parse(source: &str, version: AvroVersion) -> Result<Schema> {
    let value: Value = serde_json::from_str(source)?;
    if let Some(object) = value.as_object()
        && object.contains_key("protocol")
        && !object.contains_key("type")
    {
        return schema_error(
            "$",
            "Avro protocols are not supported; expected a schema definition (.avsc)",
        );
    }
    let mut parser = Parser {
        version,
        types: Vec::new(),
        index: HashMap::new(),
    };
    let root = parser.parse_schema(&value, None, "$")?;
    Ok(Schema {
        root,
        types: parser.types,
        version,
    })
}

/// Recursive-descent parser building the schema tree and the named type table.
struct Parser {
    version: AvroVersion,
    types: Vec<NamedType>,
    index: HashMap<FullName, NamedKind>,
}

impl Parser {
    fn parse_schema(
        &mut self,
        value: &Value,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        match value {
            Value::String(name) => self.parse_named_or_primitive(name, enclosing, path),
            Value::Array(branches) => self.parse_union(branches, enclosing, path),
            Value::Object(_) => self.parse_object_schema(value, enclosing, path),
            _ => schema_error(
                path,
                "expected a type name string, a type object or a union array",
            ),
        }
    }

    /// Parses a schema given as a bare JSON string: a primitive type name or a
    /// reference to a named type.
    fn parse_named_or_primitive(
        &mut self,
        name: &str,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        if let Some(primitive) = primitive_schema(name) {
            return Ok(primitive);
        }
        if COMPLEX_NAMES.contains(&name) {
            return schema_error(
                path,
                format!("complex type `{name}` must be declared as a type object"),
            );
        }
        self.resolve_reference(name, enclosing, path)
    }

    fn parse_object_schema(
        &mut self,
        value: &Value,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        let Some(object) = value.as_object() else {
            return schema_error(path, "expected a type object");
        };
        let type_attr = required_attribute(object, "type", path)?;
        let Some(type_name) = type_attr.as_str() else {
            return schema_error(&format!("{path}.type"), "`type` attribute must be a string");
        };
        match type_name {
            "record" => self.parse_record(object, enclosing, path),
            "enum" => self.parse_enum(object, enclosing, path),
            "fixed" => self.parse_fixed(object, enclosing, path),
            "array" => self.parse_array(object, enclosing, path),
            "map" => self.parse_map(object, enclosing, path),
            _ => {
                let Some(base) = primitive_schema(type_name) else {
                    return self.parse_named_or_primitive(type_name, enclosing, path);
                };
                match object.get("logicalType").and_then(Value::as_str) {
                    Some(name) => match logical_type(name, &base, object, self.version) {
                        Some(kind) => Ok(SchemaNode::Logical {
                            base: Box::new(base),
                            kind,
                        }),
                        None => Ok(base),
                    },
                    None => Ok(base),
                }
            }
        }
    }

    fn parse_array(
        &mut self,
        object: &Map<String, Value>,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        let items = required_attribute(object, "items", path)?;
        let items = self.parse_schema(items, enclosing, &format!("{path}.items"))?;
        Ok(SchemaNode::Array(Box::new(items)))
    }

    fn parse_map(
        &mut self,
        object: &Map<String, Value>,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        let values = required_attribute(object, "values", path)?;
        let values = self.parse_schema(values, enclosing, &format!("{path}.values"))?;
        Ok(SchemaNode::Map(Box::new(values)))
    }

    fn parse_record(
        &mut self,
        object: &Map<String, Value>,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        let (namespace, name) = parse_name(object, enclosing, path)?;
        let fullname = FullName::new(namespace.as_deref(), &name);
        self.define(&fullname, NamedKind::Record, path)?;
        let doc = optional_string(object, "doc", path)?;
        let aliases = type_aliases(object, &fullname, path)?;
        let slot = self.types.len();
        self.types.push(NamedType::Record(Record {
            name: fullname.clone(),
            doc,
            aliases,
            fields: Vec::new(),
        }));
        let fields = self.parse_fields(object, namespace.as_deref(), path)?;
        let NamedType::Record(record) = &mut self.types[slot] else {
            return Err(Error::Internal("record slot was overwritten".to_owned()));
        };
        record.fields = fields;
        Ok(SchemaNode::Record(fullname))
    }

    fn parse_fields(
        &mut self,
        object: &Map<String, Value>,
        namespace: Option<&str>,
        path: &str,
    ) -> Result<Vec<Field>> {
        let fields_attr = required_attribute(object, "fields", path)?;
        let Some(fields) = fields_attr.as_array() else {
            return schema_error(
                &format!("{path}.fields"),
                "`fields` attribute must be an array",
            );
        };
        let mut names = HashSet::new();
        let mut parsed = Vec::with_capacity(fields.len());
        for (index, value) in fields.iter().enumerate() {
            let field_path = format!("{path}.fields[{index}]");
            let field = self.parse_field(value, namespace, &field_path)?;
            if !names.insert(field.name.clone()) {
                return schema_error(
                    &format!("{field_path}.name"),
                    format!("duplicate field name `{}`", field.name),
                );
            }
            parsed.push(field);
        }
        Ok(parsed)
    }

    fn parse_field(&mut self, value: &Value, namespace: Option<&str>, path: &str) -> Result<Field> {
        let Some(object) = value.as_object() else {
            return schema_error(path, "field must be an object");
        };
        let name = required_string(object, "name", path)?;
        validate_name(&name, &format!("{path}.name"))?;
        let type_attr = required_attribute(object, "type", path)?;
        let node = self.parse_schema(type_attr, namespace, &format!("{path}.type"))?;
        let doc = optional_string(object, "doc", path)?;
        let default = self.parse_default(object, &node, path)?;
        let order = parse_order(object, path)?;
        let aliases = string_array(object, "aliases", path)?;
        Ok(Field {
            name,
            doc,
            schema: node,
            default,
            order,
            aliases,
        })
    }

    fn parse_default(
        &self,
        object: &Map<String, Value>,
        node: &SchemaNode,
        path: &str,
    ) -> Result<Option<Value>> {
        match object.get("default") {
            None => Ok(None),
            Some(value) => {
                if self.default_matches(node, value) {
                    Ok(Some(value.clone()))
                } else {
                    schema_error(
                        &format!("{path}.default"),
                        format!(
                            "default value does not match type `{}`",
                            node.avro_type_name()
                        ),
                    )
                }
            }
        }
    }

    fn parse_enum(
        &mut self,
        object: &Map<String, Value>,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        let (namespace, name) = parse_name(object, enclosing, path)?;
        let fullname = FullName::new(namespace.as_deref(), &name);
        self.define(&fullname, NamedKind::Enum, path)?;
        let doc = optional_string(object, "doc", path)?;
        let aliases = type_aliases(object, &fullname, path)?;
        let symbols = parse_symbols(object, path)?;
        let default = match object.get("default") {
            None => None,
            Some(value) => {
                let Some(symbol) = value.as_str() else {
                    return schema_error(
                        &format!("{path}.default"),
                        "enum default must be a string",
                    );
                };
                if !symbols.iter().any(|candidate| candidate == symbol) {
                    return schema_error(
                        &format!("{path}.default"),
                        format!("enum default `{symbol}` is not a symbol of `{fullname}`"),
                    );
                }
                Some(symbol.to_owned())
            }
        };
        self.types.push(NamedType::Enum(Enum {
            name: fullname.clone(),
            doc,
            aliases,
            symbols,
            default,
        }));
        Ok(SchemaNode::Enum(fullname))
    }

    fn parse_fixed(
        &mut self,
        object: &Map<String, Value>,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        let (namespace, name) = parse_name(object, enclosing, path)?;
        let fullname = FullName::new(namespace.as_deref(), &name);
        self.define(&fullname, NamedKind::Fixed, path)?;
        let doc = optional_string(object, "doc", path)?;
        let aliases = type_aliases(object, &fullname, path)?;
        let size = required_positive_integer(object, "size", path)?;
        let logical = match object.get("logicalType").and_then(Value::as_str) {
            Some(name) => fixed_logical_type(name, size, object, self.version),
            None => None,
        };
        self.types.push(NamedType::Fixed(Fixed {
            name: fullname.clone(),
            doc,
            aliases,
            size,
            logical,
        }));
        Ok(SchemaNode::Fixed(fullname))
    }

    fn parse_union(
        &mut self,
        branches: &[Value],
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        if branches.is_empty() {
            return schema_error(path, "union must have at least one branch");
        }
        let mut schemas = Vec::with_capacity(branches.len());
        for (index, value) in branches.iter().enumerate() {
            let branch = self.parse_schema(value, enclosing, &format!("{path}[{index}]"))?;
            if matches!(branch, SchemaNode::Union(_)) {
                return schema_error(
                    &format!("{path}[{index}]"),
                    "unions may not immediately contain other unions",
                );
            }
            schemas.push(branch);
        }
        let mut seen = HashSet::new();
        for (index, branch) in schemas.iter().enumerate() {
            let key = union_branch_type_key(branch);
            if !seen.insert(key.clone()) {
                return schema_error(
                    &format!("{path}[{index}]"),
                    format!("union may not contain more than one schema of type `{key}`"),
                );
            }
        }
        Ok(SchemaNode::Union(schemas))
    }

    /// Resolves a reference to a named type: a dotted name is a fullname, a
    /// simple name is looked up in the enclosing namespace first and then in
    /// the null namespace.
    fn resolve_reference(
        &mut self,
        name: &str,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<SchemaNode> {
        let candidates: Vec<FullName> = if let Some((namespace, name)) = split_fullname(name) {
            vec![FullName::new(Some(&namespace), &name)]
        } else {
            let mut candidates = Vec::new();
            if let Some(namespace) = enclosing {
                candidates.push(FullName::new(Some(namespace), name));
            }
            candidates.push(FullName::new(None, name));
            candidates
        };
        for candidate in &candidates {
            if let Some(kind) = self.index.get(candidate) {
                return Ok(match kind {
                    NamedKind::Record => SchemaNode::Record(candidate.clone()),
                    NamedKind::Enum => SchemaNode::Enum(candidate.clone()),
                    NamedKind::Fixed => SchemaNode::Fixed(candidate.clone()),
                });
            }
        }
        schema_error(path, format!("reference to undefined type `{name}`"))
    }

    /// Registers a named type definition, rejecting primitives and duplicates.
    fn define(&mut self, fullname: &FullName, kind: NamedKind, path: &str) -> Result<()> {
        if PRIMITIVE_NAMES.contains(&fullname.name.as_str()) {
            return schema_error(
                path,
                format!(
                    "primitive type name `{}` may not be redefined",
                    fullname.name
                ),
            );
        }
        if self.index.insert(fullname.clone(), kind).is_some() {
            return schema_error(path, format!("redefinition of type `{fullname}`"));
        }
        Ok(())
    }

    /// Looks up a defined record by fullname.
    fn record(&self, name: &FullName) -> Option<&Record> {
        self.types.iter().find_map(|ty| match ty {
            NamedType::Record(record) if &record.name == name => Some(record),
            _ => None,
        })
    }

    /// Looks up a defined enum by fullname.
    fn enum_type(&self, name: &FullName) -> Option<&Enum> {
        self.types.iter().find_map(|ty| match ty {
            NamedType::Enum(enum_) if &enum_.name == name => Some(enum_),
            _ => None,
        })
    }

    /// Looks up a defined fixed by fullname.
    fn fixed(&self, name: &FullName) -> Option<&Fixed> {
        self.types.iter().find_map(|ty| match ty {
            NamedType::Fixed(fixed) if &fixed.name == name => Some(fixed),
            _ => None,
        })
    }

    /// Checks a JSON value against the default value encoding of a schema.
    fn default_matches(&self, node: &SchemaNode, value: &Value) -> bool {
        match node {
            SchemaNode::Null => value.is_null(),
            SchemaNode::Boolean => value.is_boolean(),
            SchemaNode::Int => value.as_i64().is_some_and(|number| {
                (i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&number)
            }),
            SchemaNode::Long => {
                value.as_i64().is_some()
                    || value
                        .as_u64()
                        .is_some_and(|number| i128::from(number) <= i128::from(i64::MAX))
            }
            SchemaNode::Float | SchemaNode::Double => value.is_number(),
            SchemaNode::Bytes => value.as_str().is_some_and(is_byte_string),
            SchemaNode::String => value.is_string(),
            SchemaNode::Fixed(name) => self.fixed(name).is_some_and(|fixed| {
                value.as_str().is_some_and(|string| {
                    is_byte_string(string) && string.chars().count() == fixed.size as usize
                })
            }),
            SchemaNode::Enum(name) => self.enum_type(name).is_some_and(|enum_| {
                value
                    .as_str()
                    .is_some_and(|symbol| enum_.symbols.iter().any(|s| s == symbol))
            }),
            SchemaNode::Record(name) => self.record(name).is_some_and(|record| {
                value.as_object().is_some_and(|object| {
                    object.iter().all(|(field_name, field_value)| {
                        record
                            .fields
                            .iter()
                            .find(|field| &field.name == field_name)
                            .is_some_and(|field| self.default_matches(&field.schema, field_value))
                    }) && record
                        .fields
                        .iter()
                        .all(|field| object.contains_key(&field.name) || field.default.is_some())
                })
            }),
            SchemaNode::Array(items) => value.as_array().is_some_and(|items_json| {
                items_json.iter().all(|v| self.default_matches(items, v))
            }),
            SchemaNode::Map(values) => value
                .as_object()
                .is_some_and(|object| object.values().all(|v| self.default_matches(values, v))),
            SchemaNode::Union(branches) => branches
                .iter()
                .any(|branch| self.default_matches(branch, value)),
            SchemaNode::Logical { base, .. } => self.default_matches(base, value),
        }
    }
}

/// Returns the primitive schema for a primitive type name.
fn primitive_schema(name: &str) -> Option<SchemaNode> {
    match name {
        "null" => Some(SchemaNode::Null),
        "boolean" => Some(SchemaNode::Boolean),
        "int" => Some(SchemaNode::Int),
        "long" => Some(SchemaNode::Long),
        "float" => Some(SchemaNode::Float),
        "double" => Some(SchemaNode::Double),
        "bytes" => Some(SchemaNode::Bytes),
        "string" => Some(SchemaNode::String),
        _ => None,
    }
}

/// Splits a fullname into namespace and simple name, unless it is simple.
fn split_fullname(name: &str) -> Option<(String, String)> {
    name.rfind('.')
        .map(|index| (name[..index].to_owned(), name[index + 1..].to_owned()))
}

/// Checks a name against the Avro name grammar `[A-Za-z_][A-Za-z0-9_]*`.
fn is_valid_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn validate_name(name: &str, path: &str) -> Result<()> {
    if is_valid_name(name) {
        Ok(())
    } else {
        schema_error(
            path,
            format!("invalid name `{name}`: names must match `[A-Za-z_][A-Za-z0-9_]*`"),
        )
    }
}

fn validate_namespace(namespace: &str, path: &str) -> Result<()> {
    if !namespace.is_empty() && namespace.split('.').all(is_valid_name) {
        Ok(())
    } else {
        schema_error(
            path,
            format!(
                "invalid namespace `{namespace}`: namespaces must be dot-separated \
                 names matching `[A-Za-z_][A-Za-z0-9_]*`"
            ),
        )
    }
}

/// Determines the namespace and simple name of a named type definition from
/// its `name` and `namespace` attributes.
fn parse_name(
    object: &Map<String, Value>,
    enclosing: Option<&str>,
    path: &str,
) -> Result<(Option<String>, String)> {
    let name_attr = required_string(object, "name", path)?;
    if let Some((namespace, name)) = split_fullname(&name_attr) {
        validate_namespace(&namespace, &format!("{path}.name"))?;
        validate_name(&name, &format!("{path}.name"))?;
        return Ok((Some(namespace), name));
    }
    validate_name(&name_attr, &format!("{path}.name"))?;
    let namespace = match object.get("namespace") {
        None => enclosing.map(str::to_owned),
        Some(value) => {
            let Some(namespace) = value.as_str() else {
                return schema_error(
                    &format!("{path}.namespace"),
                    "`namespace` attribute must be a string",
                );
            };
            if namespace.is_empty() {
                None
            } else {
                validate_namespace(namespace, &format!("{path}.namespace"))?;
                Some(namespace.to_owned())
            }
        }
    };
    Ok((namespace, name_attr))
}

fn parse_order(object: &Map<String, Value>, path: &str) -> Result<Order> {
    match object.get("order") {
        None => Ok(Order::Ascending),
        Some(value) => {
            let Some(order) = value.as_str() else {
                return schema_error(
                    &format!("{path}.order"),
                    "`order` attribute must be a string",
                );
            };
            match order {
                "ascending" => Ok(Order::Ascending),
                "descending" => Ok(Order::Descending),
                "ignore" => Ok(Order::Ignore),
                other => schema_error(
                    &format!("{path}.order"),
                    format!(
                        "invalid order `{other}`: expected `ascending`, `descending` or `ignore`"
                    ),
                ),
            }
        }
    }
}

fn parse_symbols(object: &Map<String, Value>, path: &str) -> Result<Vec<String>> {
    let symbols_attr = required_attribute(object, "symbols", path)?;
    let Some(symbols) = symbols_attr.as_array() else {
        return schema_error(
            &format!("{path}.symbols"),
            "`symbols` attribute must be an array",
        );
    };
    if symbols.is_empty() {
        return schema_error(
            &format!("{path}.symbols"),
            "enum must have at least one symbol",
        );
    }
    let mut seen = HashSet::new();
    let mut parsed = Vec::with_capacity(symbols.len());
    for (index, symbol) in symbols.iter().enumerate() {
        let Some(symbol) = symbol.as_str() else {
            return schema_error(
                &format!("{path}.symbols[{index}]"),
                "enum symbol must be a string",
            );
        };
        validate_name(symbol, &format!("{path}.symbols[{index}]"))?;
        if !seen.insert(symbol) {
            return schema_error(
                &format!("{path}.symbols[{index}]"),
                format!("duplicate enum symbol `{symbol}`"),
            );
        }
        parsed.push(symbol.to_owned());
    }
    Ok(parsed)
}

/// Resolves type aliases relative to the namespace of the aliased type.
fn type_aliases(object: &Map<String, Value>, of: &FullName, path: &str) -> Result<Vec<FullName>> {
    Ok(string_array(object, "aliases", path)?
        .into_iter()
        .map(|alias| match split_fullname(&alias) {
            Some((namespace, name)) => FullName::new(Some(&namespace), &name),
            None => FullName::new(of.namespace.as_deref(), &alias),
        })
        .collect())
}

fn required_attribute<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a Value> {
    object.get(key).ok_or_else(|| {
        crate::error::invalid_schema(path, format!("missing required attribute `{key}`"))
    })
}

fn required_string(object: &Map<String, Value>, key: &str, path: &str) -> Result<String> {
    let value = required_attribute(object, key, path)?;
    let Some(string) = value.as_str() else {
        return schema_error(
            &format!("{path}.{key}"),
            format!("`{key}` attribute must be a string"),
        );
    };
    Ok(string.to_owned())
}

fn optional_string(object: &Map<String, Value>, key: &str, path: &str) -> Result<Option<String>> {
    match object.get(key) {
        None => Ok(None),
        Some(value) => {
            let Some(string) = value.as_str() else {
                return schema_error(
                    &format!("{path}.{key}"),
                    format!("`{key}` attribute must be a string"),
                );
            };
            Ok(Some(string.to_owned()))
        }
    }
}

fn required_positive_integer(object: &Map<String, Value>, key: &str, path: &str) -> Result<u32> {
    let value = required_attribute(object, key, path)?;
    let Some(number) = value.as_u64() else {
        return schema_error(
            &format!("{path}.{key}"),
            format!("`{key}` attribute must be a positive integer"),
        );
    };
    if number == 0 {
        return schema_error(
            &format!("{path}.{key}"),
            format!("`{key}` attribute must be a positive integer"),
        );
    }
    let Ok(parsed) = u32::try_from(number) else {
        return schema_error(
            &format!("{path}.{key}"),
            format!(
                "`{key}` attribute must be a positive integer not larger than {}",
                u32::MAX
            ),
        );
    };
    Ok(parsed)
}

fn positive_integer(object: &Map<String, Value>, key: &str) -> Option<u32> {
    let parsed = u32::try_from(object.get(key)?.as_u64()?).ok()?;
    if parsed == 0 { None } else { Some(parsed) }
}

fn non_negative_integer(object: &Map<String, Value>, key: &str) -> Option<u32> {
    u32::try_from(object.get(key)?.as_u64()?).ok()
}

fn string_array(object: &Map<String, Value>, key: &str, path: &str) -> Result<Vec<String>> {
    match object.get(key) {
        None => Ok(Vec::new()),
        Some(value) => {
            let Some(array) = value.as_array() else {
                return schema_error(
                    &format!("{path}.{key}"),
                    format!("`{key}` attribute must be an array of strings"),
                );
            };
            let mut strings = Vec::with_capacity(array.len());
            for (index, value) in array.iter().enumerate() {
                let Some(string) = value.as_str() else {
                    return schema_error(
                        &format!("{path}.{key}[{index}]"),
                        format!("`{key}` attribute must only contain strings"),
                    );
                };
                strings.push(string.to_owned());
            }
            Ok(strings)
        }
    }
}

/// Checks that a JSON string only contains code points 0-255, as required for
/// `bytes` and `fixed` default values.
fn is_byte_string(string: &str) -> bool {
    string.chars().all(|c| u32::from(c) <= 0xFF)
}

/// A key for the union branch duplicate rule: unions may not contain more
/// than one schema of the same type, except for named types, which are
/// distinguished by fullname. Collections count as their type regardless of
/// their item or value schemas.
fn union_branch_type_key(branch: &SchemaNode) -> String {
    match branch {
        SchemaNode::Record(name) => format!("record:{name}"),
        SchemaNode::Enum(name) => format!("enum:{name}"),
        SchemaNode::Fixed(name) => format!("fixed:{name}"),
        SchemaNode::Array(_) => "array".to_owned(),
        SchemaNode::Map(_) => "map".to_owned(),
        SchemaNode::Logical { base, .. } => union_branch_type_key(base),
        other => other.avro_type_name().to_owned(),
    }
}

/// Resolves a logical type annotating a primitive. Unknown or invalid logical
/// types (and ones unsupported by the target Avro version) return `None`, in
/// which case the underlying type is used as required by the specification.
fn logical_type(
    name: &str,
    base: &SchemaNode,
    object: &Map<String, Value>,
    version: AvroVersion,
) -> Option<LogicalType> {
    match (name, base) {
        ("decimal", SchemaNode::Bytes) => decimal_from(object),
        ("big-decimal", SchemaNode::Bytes) if version >= AvroVersion::V1_12 => {
            Some(LogicalType::BigDecimal)
        }
        ("uuid", SchemaNode::String) => Some(LogicalType::Uuid),
        ("date", SchemaNode::Int) => Some(LogicalType::Date),
        ("time-millis", SchemaNode::Int) => Some(LogicalType::TimeMillis),
        ("time-micros", SchemaNode::Long) => Some(LogicalType::TimeMicros),
        ("timestamp-millis", SchemaNode::Long) => Some(LogicalType::TimestampMillis),
        ("timestamp-micros", SchemaNode::Long) => Some(LogicalType::TimestampMicros),
        ("timestamp-nanos", SchemaNode::Long) if version >= AvroVersion::V1_12 => {
            Some(LogicalType::TimestampNanos)
        }
        ("local-timestamp-millis", SchemaNode::Long) => Some(LogicalType::LocalTimestampMillis),
        ("local-timestamp-micros", SchemaNode::Long) => Some(LogicalType::LocalTimestampMicros),
        ("local-timestamp-nanos", SchemaNode::Long) if version >= AvroVersion::V1_12 => {
            Some(LogicalType::LocalTimestampNanos)
        }
        _ => None,
    }
}

/// Resolves a logical type annotating a `fixed`.
fn fixed_logical_type(
    name: &str,
    size: u32,
    object: &Map<String, Value>,
    version: AvroVersion,
) -> Option<LogicalType> {
    match name {
        "decimal" => {
            let LogicalType::Decimal { precision, scale } = decimal_from(object)? else {
                return None;
            };
            if precision > max_decimal_precision(size) {
                return None;
            }
            Some(LogicalType::Decimal { precision, scale })
        }
        "uuid" if version >= AvroVersion::V1_12 && size == 16 => Some(LogicalType::Uuid),
        "duration" if size == 12 => Some(LogicalType::Duration),
        _ => None,
    }
}

/// Parses the `precision` and `scale` attributes of a `decimal`.
fn decimal_from(object: &Map<String, Value>) -> Option<LogicalType> {
    let precision = positive_integer(object, "precision")?;
    let scale = non_negative_integer(object, "scale").unwrap_or(0);
    if scale > precision {
        return None;
    }
    Some(LogicalType::Decimal { precision, scale })
}

/// Maximum base-10 digit precision representable by a two's complement
/// big-endian `fixed` of the given size: `floor(log10(2^(8n-1) - 1))`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn max_decimal_precision(size: u32) -> u32 {
    let bits = 8.0 * f64::from(size) - 1.0;
    (bits * std::f64::consts::LOG10_2).floor() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> Schema {
        parse(source, AvroVersion::V1_12).unwrap_or_else(|error| panic!("{error}"))
    }

    fn parse_err(source: &str) -> String {
        parse(source, AvroVersion::V1_12)
            .expect_err("schema should be rejected")
            .to_string()
    }

    fn fullnames(schema: &Schema) -> Vec<String> {
        schema.types.iter().map(|ty| ty.name().full()).collect()
    }

    fn record_of<'a>(schema: &'a Schema, fullname: &str) -> &'a Record {
        schema
            .types
            .iter()
            .find_map(|ty| match ty {
                NamedType::Record(record) if record.name.full() == fullname => Some(record),
                _ => None,
            })
            .unwrap_or_else(|| panic!("record `{fullname}` not defined"))
    }

    #[test]
    fn parses_primitive_strings_and_objects() {
        assert_eq!(parse_ok(r#""null""#).root, SchemaNode::Null);
        assert_eq!(parse_ok(r#"{"type": "int"}"#).root, SchemaNode::Int);
        assert_eq!(parse_ok(r#"{"type": "string"}"#).root, SchemaNode::String);
    }

    #[test]
    fn rejects_non_schema_json() {
        assert!(parse_err("42").starts_with("invalid schema at `$`:"));
        assert!(parse_err("[]").starts_with("invalid schema at `$`:"));
        assert!(parse_err("{}").contains("missing required attribute `type`"));
        assert!(parse_err("not json").starts_with("invalid JSON:"));
    }

    #[test]
    fn rejects_protocols() {
        assert_eq!(
            parse_err(r#"{"protocol": "Hello", "messages": {}}"#),
            "invalid schema at `$`: Avro protocols are not supported; expected a schema \
             definition (.avsc)"
        );
    }

    #[test]
    fn determines_fullnames_per_specification() {
        let schema = parse_ok(
            r#"{
                "type": "record",
                "name": "Example",
                "doc": "A simple name with no namespace: the fullname is 'Example'.",
                "fields": [
                    {"name": "inheritNull", "type": {
                        "type": "enum", "name": "Simple",
                        "symbols": ["a", "b"]
                    }},
                    {"name": "explicitNamespace", "type": {
                        "type": "fixed", "name": "Simple", "namespace": "explicit", "size": 12
                    }},
                    {"name": "fullName", "type": {
                        "type": "record", "name": "a.full.Name", "namespace": "ignored",
                        "fields": [
                            {"name": "inheritNamespace", "type": {
                                "type": "enum", "name": "Understanding",
                                "symbols": ["d", "e"]
                            }}
                        ]
                    }}
                ]
            }"#,
        );
        assert_eq!(
            fullnames(&schema),
            [
                "Example",
                "Simple",
                "explicit.Simple",
                "a.full.Name",
                "a.full.Understanding"
            ]
        );
    }

    #[test]
    fn explicit_empty_namespace_is_the_null_namespace() {
        let schema =
            parse_ok(r#"{"type": "record", "name": "Plain", "namespace": "", "fields": []}"#);
        assert_eq!(fullnames(&schema), ["Plain"]);
        assert_eq!(schema.types[0].name().namespace, None);
    }

    #[test]
    fn resolves_references_in_enclosing_then_null_namespace() {
        let schema = parse_ok(
            r#"{
                "type": "record", "name": "com.example.Outer", "fields": [
                    {"name": "a", "type": {
                        "type": "record", "name": "Inner", "fields": []
                    }},
                    {"name": "b", "type": {
                        "type": "record", "name": "Plain", "namespace": "", "fields": []
                    }},
                    {"name": "c", "type": {
                        "type": "record", "name": "Inner", "namespace": "other", "fields": []
                    }},
                    {"name": "d", "type": "Inner"},
                    {"name": "e", "type": "com.example.Inner"},
                    {"name": "f", "type": "other.Inner"},
                    {"name": "g", "type": "Plain"}
                ]
            }"#,
        );
        let outer = record_of(&schema, "com.example.Outer");
        let resolved = |index: usize| match &outer.fields[index].schema {
            SchemaNode::Record(name) | SchemaNode::Enum(name) | SchemaNode::Fixed(name) => {
                name.full()
            }
            other => panic!("unexpected field schema: {other:?}"),
        };
        assert_eq!(resolved(3), "com.example.Inner");
        assert_eq!(resolved(4), "com.example.Inner");
        assert_eq!(resolved(5), "other.Inner");
        assert_eq!(resolved(6), "Plain");
    }

    #[test]
    fn rejects_undefined_and_forward_references() {
        assert_eq!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [{"name": "x", "type": "Missing"}]}"#
            ),
            "invalid schema at `$.fields[0].type`: reference to undefined type `Missing`"
        );
        assert_eq!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [
                    {"name": "x", "type": "B"},
                    {"name": "y", "type": {"type": "record", "name": "B", "fields": []}}
                ]}"#
            ),
            "invalid schema at `$.fields[0].type`: reference to undefined type `B`"
        );
    }

    #[test]
    fn rejects_redefinition() {
        assert_eq!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [
                    {"name": "x", "type": {"type": "record", "name": "N", "fields": []}},
                    {"name": "y", "type": {"type": "record", "name": "N", "fields": []}}
                ]}"#
            ),
            "invalid schema at `$.fields[1].type`: redefinition of type `N`"
        );
    }

    #[test]
    fn rejects_primitive_and_invalid_names() {
        assert!(
            parse_err(r#"{"type": "record", "name": "string", "fields": []}"#)
                .contains("primitive type name `string` may not be redefined")
        );
        assert!(
            parse_err(r#"{"type": "record", "name": "9bad", "fields": []}"#)
                .contains("invalid name `9bad`")
        );
        assert!(
            parse_err(r#"{"type": "record", "name": "R", "namespace": "a..b", "fields": []}"#)
                .contains("invalid namespace `a..b`")
        );
        assert!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [{"name": "b@d", "type": "int"}]}"#
            )
            .contains("invalid name `b@d`")
        );
        assert!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [{"name": "x", "type": "array"}]}"#
            )
            .contains("complex type `array` must be declared as a type object")
        );
    }

    #[test]
    fn rejects_duplicate_field_names() {
        assert_eq!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [
                    {"name": "x", "type": "int"},
                    {"name": "x", "type": "long"}
                ]}"#
            ),
            "invalid schema at `$.fields[1].name`: duplicate field name `x`"
        );
    }

    #[test]
    fn parses_enums() {
        let schema = parse_ok(
            r#"{"type": "enum", "name": "Suit", "doc": "Suits.",
                "symbols": ["SPADES", "HEARTS"], "default": "SPADES"}"#,
        );
        let NamedType::Enum(enum_) = &schema.types[0] else {
            panic!("expected an enum");
        };
        assert_eq!(enum_.symbols, ["SPADES", "HEARTS"]);
        assert_eq!(enum_.default.as_deref(), Some("SPADES"));
        assert_eq!(enum_.doc.as_deref(), Some("Suits."));
    }

    #[test]
    fn rejects_invalid_enums() {
        assert!(
            parse_err(r#"{"type": "enum", "name": "E", "symbols": []}"#)
                .contains("enum must have at least one symbol")
        );
        assert!(
            parse_err(r#"{"type": "enum", "name": "E", "symbols": ["A", "A"]}"#)
                .contains("duplicate enum symbol `A`")
        );
        assert!(
            parse_err(r#"{"type": "enum", "name": "E", "symbols": ["A", 3]}"#)
                .contains("enum symbol must be a string")
        );
        assert!(
            parse_err(r#"{"type": "enum", "name": "E", "symbols": ["A"], "default": "B"}"#)
                .contains("enum default `B` is not a symbol")
        );
        assert!(
            parse_err(r#"{"type": "enum", "name": "E", "symbols": ["a-b"]}"#)
                .contains("invalid name `a-b`")
        );
    }

    #[test]
    fn parses_fixed_with_size() {
        let schema = parse_ok(
            r#"{"type": "record", "name": "R", "fields": [
                {"name": "md5", "type": {"type": "fixed", "name": "Md5", "size": 16}},
                {"name": "copy", "type": "Md5"}
            ]}"#,
        );
        let NamedType::Fixed(fixed) = &schema.types[1] else {
            panic!("expected a fixed");
        };
        assert_eq!(fixed.size, 16);
        assert!(matches!(schema.root, SchemaNode::Record(_)));
    }

    #[test]
    fn rejects_invalid_fixed() {
        assert!(
            parse_err(r#"{"type": "fixed", "name": "F"}"#)
                .contains("missing required attribute `size`")
        );
        assert!(
            parse_err(r#"{"type": "fixed", "name": "F", "size": 0}"#)
                .contains("`size` attribute must be a positive integer")
        );
        assert!(
            parse_err(r#"{"type": "fixed", "name": "F", "size": -4}"#)
                .contains("`size` attribute must be a positive integer")
        );
    }

    #[test]
    fn parses_collections() {
        let schema = parse_ok(
            r#"{"type": "record", "name": "R", "fields": [
                {"name": "ids", "type": {"type": "array", "items": "long"}},
                {"name": "labels", "type": {"type": "map", "values": "string"}}
            ]}"#,
        );
        let record = record_of(&schema, "R");
        assert!(
            matches!(&record.fields[0].schema, SchemaNode::Array(items) if **items == SchemaNode::Long)
        );
        assert!(
            matches!(&record.fields[1].schema, SchemaNode::Map(values) if **values == SchemaNode::String)
        );
    }

    #[test]
    fn parses_self_referencing_unions() {
        let schema = parse_ok(
            r#"{"type": "record", "name": "LongList", "fields": [
                {"name": "value", "type": "long"},
                {"name": "next", "type": ["null", "LongList"]}
            ]}"#,
        );
        let record = record_of(&schema, "LongList");
        assert!(
            matches!(&record.fields[1].schema, SchemaNode::Union(branches) if branches.len() == 2)
        );
    }

    #[test]
    fn rejects_invalid_unions() {
        assert_eq!(
            parse_err("[]"),
            "invalid schema at `$`: union must have at least one branch"
        );
        assert_eq!(
            parse_err(r#"[["null", "string"]]"#),
            "invalid schema at `$[0]`: unions may not immediately contain other unions"
        );
        assert_eq!(
            parse_err(r#"["int", "int"]"#),
            "invalid schema at `$[1]`: union may not contain more than one schema of type `int`"
        );
        assert_eq!(
            parse_err(
                r#"[
                    {"type": "array", "items": "int"},
                    {"type": "array", "items": "long"}
                ]"#
            ),
            "invalid schema at `$[1]`: union may not contain more than one schema of type `array`"
        );
    }

    #[test]
    fn unions_may_repeat_named_types() {
        let schema = parse_ok(
            r#"{"type": "record", "name": "R", "fields": [
                {"name": "x", "type": [
                    {"type": "record", "name": "A", "fields": []},
                    {"type": "record", "name": "B", "fields": []}
                ]}
            ]}"#,
        );
        assert_eq!(fullnames(&schema), ["R", "A", "B"]);
    }

    #[test]
    fn parses_field_attributes() {
        let schema = parse_ok(
            r#"{"type": "record", "name": "R", "fields": [
                {"name": "x", "type": "int", "doc": "An x.", "order": "descending",
                 "aliases": ["y", "z"], "default": 3}
            ]}"#,
        );
        let field = &record_of(&schema, "R").fields[0];
        assert_eq!(field.doc.as_deref(), Some("An x."));
        assert_eq!(field.order, Order::Descending);
        assert_eq!(field.aliases, ["y", "z"]);
        assert_eq!(field.default.as_ref().and_then(Value::as_i64), Some(3));
    }

    #[test]
    fn rejects_invalid_order() {
        assert!(parse_err(
            r#"{"type": "record", "name": "R", "fields": [{"name": "x", "type": "int", "order": "sideways"}]}"#
        )
        .contains("invalid order `sideways`"));
    }

    #[test]
    fn validates_field_defaults() {
        parse_ok(
            r#"{"type": "record", "name": "R", "fields": [
                {"name": "i", "type": "int", "default": 42},
                {"name": "s", "type": "string", "default": "foo"},
                {"name": "b", "type": "bytes", "default": "\u00FF"},
                {"name": "n", "type": ["null", "long"], "default": null},
                {"name": "u", "type": ["int", "string"], "default": 7},
                {"name": "a", "type": {"type": "array", "items": "int"}, "default": [1, 2]},
                {"name": "m", "type": {"type": "map", "values": "double"}, "default": {"x": 1.5}},
                {"name": "f", "type": {"type": "fixed", "name": "F", "size": 2}, "default": "ab"},
                {"name": "e", "type": {"type": "enum", "name": "E", "symbols": ["A"]}, "default": "A"},
                {"name": "r", "type": {"type": "record", "name": "N",
                    "fields": [{"name": "x", "type": "long", "default": 3}, {"name": "y", "type": "string"}]},
                 "default": {"x": 9, "y": "complete"}},
                {"name": "p", "type": {"type": "record", "name": "M",
                    "fields": [{"name": "x", "type": "long", "default": 3}]},
                 "default": {}}
            ]}"#,
        );
    }

    #[test]
    fn record_defaults_must_be_complete() {
        assert!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [
                {"name": "x", "type": {"type": "record", "name": "N",
                    "fields": [{"name": "a", "type": "int"}, {"name": "b", "type": "int"}]},
                 "default": {"a": 1}}
            ]}"#
            )
            .contains("default value does not match type `record`")
        );
    }

    #[test]
    fn rejects_invalid_field_defaults() {
        assert!(parse_err(
            r#"{"type": "record", "name": "R", "fields": [{"name": "x", "type": "int", "default": "1"}]}"#
        )
        .contains("default value does not match type `int`"));
        assert!(parse_err(
            r#"{"type": "record", "name": "R", "fields": [{"name": "x", "type": "int", "default": 3000000000}]}"#
        )
        .contains("default value does not match type `int`"));
        assert!(parse_err(
            r#"{"type": "record", "name": "R", "fields": [{"name": "x", "type": "bytes", "default": "€"}]}"#
        )
        .contains("default value does not match type `bytes`"));
        assert!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [
                {"name": "x", "type": {"type": "fixed", "name": "F", "size": 4}, "default": "ab"}
            ]}"#
            )
            .contains("default value does not match type `fixed`")
        );
        assert!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [
                {"name": "x", "type": ["string", "int"], "default": true}
            ]}"#
            )
            .contains("default value does not match type `union`")
        );
        assert!(
            parse_err(
                r#"{"type": "record", "name": "R", "fields": [
                {"name": "x", "type": {"type": "record", "name": "N",
                    "fields": [{"name": "a", "type": "int"}]},
                 "default": {"b": 1}}
            ]}"#
            )
            .contains("default value does not match type `record`")
        );
    }

    #[test]
    fn parses_logical_types() {
        let schema = parse_ok(
            r#"{"type": "record", "name": "L", "fields": [
                {"name": "date", "type": {"type": "int", "logicalType": "date"}},
                {"name": "timeMillis", "type": {"type": "int", "logicalType": "time-millis"}},
                {"name": "timeMicros", "type": {"type": "long", "logicalType": "time-micros"}},
                {"name": "tsMillis", "type": {"type": "long", "logicalType": "timestamp-millis"}},
                {"name": "tsMicros", "type": {"type": "long", "logicalType": "timestamp-micros"}},
                {"name": "tsNanos", "type": {"type": "long", "logicalType": "timestamp-nanos"}},
                {"name": "ltsMillis", "type": {"type": "long", "logicalType": "local-timestamp-millis"}},
                {"name": "ltsMicros", "type": {"type": "long", "logicalType": "local-timestamp-micros"}},
                {"name": "ltsNanos", "type": {"type": "long", "logicalType": "local-timestamp-nanos"}},
                {"name": "uuid", "type": {"type": "string", "logicalType": "uuid"}},
                {"name": "decimal", "type": {"type": "bytes", "logicalType": "decimal",
                    "precision": 4, "scale": 2}},
                {"name": "bigDecimal", "type": {"type": "bytes", "logicalType": "big-decimal"}},
                {"name": "duration", "type": {"type": "fixed", "name": "D", "size": 12,
                    "logicalType": "duration"}},
                {"name": "uuidFixed", "type": {"type": "fixed", "name": "U", "size": 16,
                    "logicalType": "uuid"}},
                {"name": "decimalFixed", "type": {"type": "fixed", "name": "P", "size": 8,
                    "logicalType": "decimal", "precision": 18, "scale": 6}}
            ]}"#,
        );
        let record = record_of(&schema, "L");
        let kinds: Vec<LogicalType> = record
            .fields
            .iter()
            .filter_map(|field| match &field.schema {
                SchemaNode::Logical { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            [
                LogicalType::Date,
                LogicalType::TimeMillis,
                LogicalType::TimeMicros,
                LogicalType::TimestampMillis,
                LogicalType::TimestampMicros,
                LogicalType::TimestampNanos,
                LogicalType::LocalTimestampMillis,
                LogicalType::LocalTimestampMicros,
                LogicalType::LocalTimestampNanos,
                LogicalType::Uuid,
                LogicalType::Decimal {
                    precision: 4,
                    scale: 2
                },
                LogicalType::BigDecimal,
            ]
        );
        let fixed_logical = |name: &str| match schema.get(&FullName::new(None, name)) {
            Some(NamedType::Fixed(fixed)) => fixed.logical,
            _ => panic!("fixed `{name}` not defined"),
        };
        assert_eq!(fixed_logical("D"), Some(LogicalType::Duration));
        assert_eq!(fixed_logical("U"), Some(LogicalType::Uuid));
        assert_eq!(
            fixed_logical("P"),
            Some(LogicalType::Decimal {
                precision: 18,
                scale: 6
            })
        );
    }

    #[test]
    fn nanos_and_big_decimal_are_gated_by_version() {
        let nanos = r#"{"type": "long", "logicalType": "timestamp-nanos"}"#;
        assert!(matches!(
            parse(nanos, AvroVersion::V1_12).unwrap().root,
            SchemaNode::Logical {
                kind: LogicalType::TimestampNanos,
                ..
            }
        ));
        assert_eq!(
            parse(nanos, AvroVersion::V1_11).unwrap().root,
            SchemaNode::Long
        );

        let big = r#"{"type": "bytes", "logicalType": "big-decimal"}"#;
        assert!(matches!(
            parse(big, AvroVersion::V1_12).unwrap().root,
            SchemaNode::Logical {
                kind: LogicalType::BigDecimal,
                ..
            }
        ));
        assert_eq!(
            parse(big, AvroVersion::V1_11).unwrap().root,
            SchemaNode::Bytes
        );

        let uuid_fixed = r#"{"type": "fixed", "name": "U", "size": 16, "logicalType": "uuid"}"#;
        let v12 = parse(uuid_fixed, AvroVersion::V1_12).unwrap();
        let NamedType::Fixed(fixed) = &v12.types[0] else {
            panic!("expected a fixed");
        };
        assert_eq!(fixed.logical, Some(LogicalType::Uuid));
        let v11 = parse(uuid_fixed, AvroVersion::V1_11).unwrap();
        let NamedType::Fixed(fixed) = &v11.types[0] else {
            panic!("expected a fixed");
        };
        assert_eq!(fixed.logical, None);
    }

    #[test]
    fn invalid_logical_types_fall_back_to_the_base_type() {
        let cases = [
            (
                r#"{"type": "int", "logicalType": "flavor"}"#,
                SchemaNode::Int,
            ),
            (
                r#"{"type": "long", "logicalType": "date"}"#,
                SchemaNode::Long,
            ),
            (
                r#"{"type": "bytes", "logicalType": "decimal", "precision": 4, "scale": 9}"#,
                SchemaNode::Bytes,
            ),
            (
                r#"{"type": "bytes", "logicalType": "decimal", "scale": 2}"#,
                SchemaNode::Bytes,
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(parse_ok(source).root, expected, "case: {source}");
        }
        let fixed = parse_ok(
            r#"{"type": "fixed", "name": "F", "size": 4, "logicalType": "decimal",
                "precision": 18}"#,
        );
        let NamedType::Fixed(fixed) = &fixed.types[0] else {
            panic!("expected a fixed");
        };
        assert_eq!(fixed.logical, None);
    }

    #[test]
    fn resolves_aliases_relative_to_own_namespace() {
        let schema =
            parse_ok(r#"{"type": "record", "name": "a.b", "aliases": ["c", "x.y"], "fields": []}"#);
        let aliases: Vec<String> = schema.types[0]
            .aliases()
            .iter()
            .map(FullName::full)
            .collect();
        assert_eq!(aliases, ["a.c", "x.y"]);
    }
}
