//! Conversion of validated Avro JSON defaults to Go expressions.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::schema::{FullName, LogicalType, NamedType, SchemaNode};

use super::model::{UnionRendering, rendering};
use super::naming;
use super::render::Renderer;

impl Renderer<'_, '_> {
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    pub(super) fn default_value(
        &self,
        node: &SchemaNode,
        value: &Value,
        owner: Option<&FullName>,
        direct: bool,
    ) -> Result<String> {
        match node {
            SchemaNode::Null => Ok("struct{}{}".to_owned()),
            SchemaNode::Boolean => Ok(value
                .as_bool()
                .map(|value| value.to_string())
                .ok_or_else(|| invalid_default("boolean", value))?),
            SchemaNode::Int => Ok(format!(
                "int32({})",
                value
                    .as_i64()
                    .ok_or_else(|| invalid_default("int", value))?
            )),
            SchemaNode::Long => Ok(format!(
                "int64({})",
                value
                    .as_i64()
                    .ok_or_else(|| invalid_default("long", value))?
            )),
            SchemaNode::Float => {
                let number = value
                    .as_f64()
                    .ok_or_else(|| invalid_default("float", value))?;
                if (number as f32).is_infinite() {
                    Ok(format!("avroFloat32({value})"))
                } else {
                    Ok(format!("float32({value})"))
                }
            }
            SchemaNode::Double => Ok(format!("float64({value})")),
            SchemaNode::Bytes => Ok(byte_string("[]byte", value)?),
            SchemaNode::String => value
                .as_str()
                .map(naming::quoted)
                .ok_or_else(|| invalid_default("string", value)),
            SchemaNode::Record(name) => {
                let record = self.model.record(name);
                let object = value
                    .as_object()
                    .ok_or_else(|| invalid_default("record", value))?;
                let go_name = self.model.type_name(name);
                let mut fields = Vec::new();
                for field in &record.fields {
                    let default = object.get(&field.name).or(field.default.as_ref());
                    if let Some(default) = default {
                        let field_name = self.model.field_name(name, field);
                        let expression =
                            self.default_value(&field.schema, default, Some(name), true)?;
                        fields.push(format!("{field_name}: {expression}"));
                    }
                }
                let literal = format!("{go_name}{{{}}}", fields.join(", "));
                if direct && owner.is_some_and(|owner| self.model.needs_pointer(owner, name)) {
                    Ok(format!("avroPtr({literal})"))
                } else {
                    Ok(literal)
                }
            }
            SchemaNode::Enum(name) => {
                let enum_ = self.model.enum_(name);
                let symbol = value
                    .as_str()
                    .ok_or_else(|| invalid_default("enum", value))?;
                let index = enum_
                    .symbols
                    .iter()
                    .position(|candidate| candidate == symbol)
                    .ok_or_else(|| invalid_default("enum", value))?;
                Ok(self.model.symbol_names(enum_)[index].clone())
            }
            SchemaNode::Fixed(name) => byte_string(self.model.type_name(name), value),
            SchemaNode::Array(items) => {
                let values = value
                    .as_array()
                    .ok_or_else(|| invalid_default("array", value))?;
                let mut rendered = Vec::with_capacity(values.len());
                for value in values {
                    rendered.push(self.default_value(items, value, owner, false)?);
                }
                Ok(format!(
                    "{}{{{}}}",
                    self.go_type(node, owner, direct),
                    rendered.join(", ")
                ))
            }
            SchemaNode::Map(values) => {
                let object = value
                    .as_object()
                    .ok_or_else(|| invalid_default("map", value))?;
                let sorted: BTreeMap<_, _> = object.iter().collect();
                let mut rendered = Vec::with_capacity(sorted.len());
                for (key, value) in sorted {
                    rendered.push(format!(
                        "{}: {}",
                        naming::quoted(key),
                        self.default_value(values, value, owner, false)?
                    ));
                }
                Ok(format!(
                    "{}{{{}}}",
                    self.go_type(node, owner, direct),
                    rendered.join(", ")
                ))
            }
            SchemaNode::Union(branches) => match rendering(branches) {
                UnionRendering::Transparent => {
                    self.default_value(&branches[0], value, owner, direct)
                }
                UnionRendering::Nullable { other } => {
                    if value.is_null() {
                        Ok("nil".to_owned())
                    } else {
                        Ok(format!(
                            "avroPtr({})",
                            self.default_value(&branches[other], value, owner, false)?
                        ))
                    }
                }
                UnionRendering::Struct => {
                    let index = branches
                        .iter()
                        .position(|branch| self.matches_default(branch, value))
                        .ok_or_else(|| invalid_default("union", value))?;
                    let id = self.model.union_id(branches);
                    let union = &self.model.unions[id];
                    let name = union.name.as_deref().unwrap();
                    let field = &union.field_names[index];
                    if matches!(branches[index], SchemaNode::Null) {
                        Ok(format!("New{name}From{field}()"))
                    } else {
                        Ok(format!(
                            "New{name}From{field}({})",
                            self.default_value(&branches[index], value, None, false)?
                        ))
                    }
                }
            },
            SchemaNode::Logical { base, kind } => match kind {
                LogicalType::Date => Ok(format!(
                    "time.Unix(int64({}) * 86_400, 0).UTC()",
                    integer(value)?
                )),
                LogicalType::TimeMillis => Ok(format!(
                    "time.Duration({}) * time.Millisecond",
                    integer(value)?
                )),
                LogicalType::TimeMicros => Ok(format!(
                    "time.Duration({}) * time.Microsecond",
                    integer(value)?
                )),
                LogicalType::TimestampMillis | LogicalType::LocalTimestampMillis => {
                    Ok(format!("avroTimestamp({}, 1_000)", integer(value)?))
                }
                LogicalType::TimestampMicros | LogicalType::LocalTimestampMicros => {
                    Ok(format!("avroTimestamp({}, 1_000_000)", integer(value)?))
                }
                LogicalType::TimestampNanos | LogicalType::LocalTimestampNanos => {
                    Ok(format!("avroTimestamp({}, 1_000_000_000)", integer(value)?))
                }
                LogicalType::Decimal { .. }
                | LogicalType::Uuid
                | LogicalType::Duration
                | LogicalType::BigDecimal => self.default_value(base, value, owner, direct),
            },
        }
    }

    fn matches_default(&self, node: &SchemaNode, value: &Value) -> bool {
        match node {
            SchemaNode::Null => value.is_null(),
            SchemaNode::Boolean => value.is_boolean(),
            SchemaNode::Int => value
                .as_i64()
                .is_some_and(|number| i32::try_from(number).is_ok()),
            SchemaNode::Long => value.as_i64().is_some(),
            SchemaNode::Float | SchemaNode::Double => value.is_number(),
            SchemaNode::Bytes => value.as_str().is_some_and(|text| {
                text.chars()
                    .all(|character| u8::try_from(character).is_ok())
            }),
            SchemaNode::String => value.is_string(),
            SchemaNode::Record(name) => {
                let record = self.model.record(name);
                value.as_object().is_some_and(|object| {
                    object.iter().all(|(field_name, field_value)| {
                        record
                            .fields
                            .iter()
                            .find(|field| &field.name == field_name)
                            .is_some_and(|field| self.matches_default(&field.schema, field_value))
                    }) && record
                        .fields
                        .iter()
                        .all(|field| object.contains_key(&field.name) || field.default.is_some())
                })
            }
            SchemaNode::Enum(name) => {
                let enum_ = self.model.enum_(name);
                value
                    .as_str()
                    .is_some_and(|symbol| enum_.symbols.iter().any(|candidate| candidate == symbol))
            }
            SchemaNode::Fixed(name) => {
                let Some(NamedType::Fixed(fixed)) = self.model.schema.get(name) else {
                    return false;
                };
                value.as_str().is_some_and(|text| {
                    text.chars().count() == fixed.size as usize
                        && text
                            .chars()
                            .all(|character| u8::try_from(character).is_ok())
                })
            }
            SchemaNode::Array(items) => value.as_array().is_some_and(|values| {
                values
                    .iter()
                    .all(|value| self.matches_default(items, value))
            }),
            SchemaNode::Map(values) => value.as_object().is_some_and(|object| {
                object
                    .values()
                    .all(|value| self.matches_default(values, value))
            }),
            SchemaNode::Union(branches) => branches
                .iter()
                .any(|branch| self.matches_default(branch, value)),
            SchemaNode::Logical { base, .. } => self.matches_default(base, value),
        }
    }
}

fn integer(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| invalid_default("integer logical type", value))
}

fn byte_string(go_type: &str, value: &Value) -> Result<String> {
    let value = value
        .as_str()
        .ok_or_else(|| invalid_default("bytes", value))?;
    let bytes = value
        .chars()
        .map(|character| format!("0x{:02x}", character as u32))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!("{go_type}{{{bytes}}}"))
}

fn invalid_default(expected: &str, value: &Value) -> Error {
    Error::Internal(format!(
        "schema parser accepted invalid {expected} default `{value}`"
    ))
}
