//! Rendering of strongly typed Go declarations and Avro methods.

use std::fmt::Write as _;

use crate::error::Result;
use crate::schema::{Enum, Fixed, FullName, LogicalType, NamedType, Record, SchemaNode};

use super::model::{Model, UnionRendering, rendering};
use super::naming;

pub(super) struct Renderer<'model, 'schema> {
    pub(super) model: &'model Model<'schema>,
}

impl<'model, 'schema> Renderer<'model, 'schema> {
    pub(super) fn new(model: &'model Model<'schema>) -> Self {
        Self { model }
    }

    pub(super) fn render(&self) -> Result<String> {
        let mut items = String::new();
        for named in &self.model.schema.types {
            items.push_str(&match named {
                NamedType::Record(record) => self.render_record(record)?,
                NamedType::Enum(enum_) => self.render_enum(enum_),
                NamedType::Fixed(fixed) => self.render_fixed(fixed),
            });
        }
        for (id, union) in self.model.unions.iter().enumerate() {
            if union.name.is_some() {
                items.push_str(&self.render_union(id));
            }
        }
        Ok(items)
    }

    #[allow(clippy::too_many_lines)]
    fn render_record(&self, record: &Record) -> Result<String> {
        let name = self.model.type_name(&record.name);
        let mut out = String::new();
        push_docs(
            &mut out,
            &format!("{name} is the Avro record `{}`.", record.name.full()),
            record.doc.as_deref(),
        );
        writeln!(out, "type {name} struct {{").unwrap();
        let mut fields = Vec::with_capacity(record.fields.len());
        for field in &record.fields {
            let field_name = self.model.field_name(&record.name, field);
            let mut docs = String::new();
            if field.doc.is_some() || logical_name(&field.schema).is_some() {
                let summary = format!("{field_name} is the Avro field `{}`.", field.name);
                push_docs_indented(
                    &mut docs,
                    &summary,
                    field.doc.as_deref(),
                    logical_name(&field.schema),
                    "\t",
                );
            }
            let go_type = self.go_type(&field.schema, Some(&record.name), true);
            let omit = match &field.schema {
                SchemaNode::Union(branches) => {
                    matches!(rendering(branches), UnionRendering::Nullable { .. })
                }
                _ => false,
            };
            let tag = if omit {
                format!("{},omitempty", field.name)
            } else {
                field.name.clone()
            };
            fields.push(StructField {
                name: field_name.to_owned(),
                go_type,
                tag: format!("json:{}", naming::quoted(&tag)),
                docs,
            });
        }
        write_struct_fields(&mut out, &fields);
        writeln!(out, "}}\n").unwrap();

        writeln!(
            out,
            "// New{name} constructs a {name} and applies its Avro field defaults."
        )
        .unwrap();
        writeln!(out, "func New{name}() *{name} {{").unwrap();
        let has_defaults = record.fields.iter().any(|field| field.default.is_some());
        if has_defaults {
            writeln!(out, "\treturn &{name}{{").unwrap();
            let mut defaults = Vec::new();
            for field in &record.fields {
                if let Some(default) = &field.default {
                    let field_name = self.model.field_name(&record.name, field);
                    let value =
                        self.default_value(&field.schema, default, Some(&record.name), true)?;
                    defaults.push((format!("{field_name}:"), value));
                }
            }
            let width = defaults
                .iter()
                .map(|(name, _)| name.len())
                .max()
                .unwrap_or(0);
            for (field_name, value) in defaults {
                writeln!(out, "\t\t{field_name:<width$} {value},").unwrap();
            }
            writeln!(out, "\t}}").unwrap();
        } else {
            writeln!(out, "\treturn &{name}{{}}").unwrap();
        }
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v *{name}) WriteAvro(w io.Writer) error {{").unwrap();
        writeln!(out, "\tif v == nil {{").unwrap();
        writeln!(out, "\t\treturn fmt.Errorf(\"cannot write a nil {name}\")").unwrap();
        writeln!(out, "\t}}").unwrap();
        for field in &record.fields {
            let field_name = self.model.field_name(&record.name, field);
            let writer = indent_lines(
                &self.write_fn(&field.schema, Some(&record.name), true),
                "\t",
            );
            writeln!(
                out,
                "\tif err := {writer}(w, v.{field_name}); err != nil {{"
            )
            .unwrap();
            writeln!(
                out,
                "\t\treturn fmt.Errorf({}, err)",
                naming::quoted(&format!("Avro field {}: %w", field.name))
            )
            .unwrap();
            writeln!(out, "\t}}").unwrap();
        }
        writeln!(out, "\treturn nil").unwrap();
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v *{name}) ReadAvro(r io.Reader) error {{").unwrap();
        writeln!(out, "\tif v == nil {{").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"cannot read into a nil {name}\")"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        for (index, field) in record.fields.iter().enumerate() {
            let field_name = self.model.field_name(&record.name, field);
            let reader = indent_lines(&self.read_fn(&field.schema, Some(&record.name), true), "\t");
            writeln!(out, "\tfield{index}, err := {reader}(r)").unwrap();
            writeln!(out, "\tif err != nil {{").unwrap();
            writeln!(
                out,
                "\t\treturn fmt.Errorf({}, err)",
                naming::quoted(&format!("Avro field {}: %w", field.name))
            )
            .unwrap();
            writeln!(out, "\t}}").unwrap();
            writeln!(out, "\tv.{field_name} = field{index}").unwrap();
        }
        writeln!(out, "\treturn nil").unwrap();
        writeln!(out, "}}\n").unwrap();
        out.push_str(&marshal_methods(name));
        out.push_str(&named_helpers(name));
        Ok(out)
    }

    fn render_enum(&self, enum_: &Enum) -> String {
        let name = self.model.type_name(&enum_.name);
        let symbols = self.model.symbol_names(enum_);
        let mut out = String::new();
        push_docs(
            &mut out,
            &format!("{name} is the Avro enum `{}`.", enum_.name.full()),
            enum_.doc.as_deref(),
        );
        writeln!(out, "type {name} int32\n").unwrap();
        writeln!(out, "const (").unwrap();
        for (index, symbol) in symbols.iter().enumerate() {
            if index == 0 {
                writeln!(out, "\t{symbol} {name} = iota").unwrap();
            } else {
                writeln!(out, "\t{symbol}").unwrap();
            }
        }
        writeln!(out, ")\n").unwrap();

        writeln!(out, "func (v {name}) String() string {{").unwrap();
        writeln!(out, "\tswitch v {{").unwrap();
        for (constant, symbol) in symbols.iter().zip(&enum_.symbols) {
            writeln!(out, "\tcase {constant}:").unwrap();
            writeln!(out, "\t\treturn {}", naming::quoted(symbol)).unwrap();
        }
        writeln!(out, "\tdefault:").unwrap();
        writeln!(out, "\t\treturn fmt.Sprintf(\"{name}(%d)\", int32(v))").unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v {name}) Valid() bool {{").unwrap();
        writeln!(out, "\treturn v >= 0 && int64(v) < {}", enum_.symbols.len()).unwrap();
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v {name}) MarshalText() ([]byte, error) {{").unwrap();
        writeln!(out, "\tif !v.Valid() {{").unwrap();
        writeln!(
            out,
            "\t\treturn nil, fmt.Errorf(\"invalid {name} value %d\", int32(v))"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\treturn []byte(v.String()), nil").unwrap();
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v *{name}) UnmarshalText(text []byte) error {{").unwrap();
        writeln!(out, "\tif v == nil {{").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"cannot unmarshal into a nil {name}\")"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\tswitch string(text) {{").unwrap();
        for (constant, symbol) in symbols.iter().zip(&enum_.symbols) {
            writeln!(out, "\tcase {}:", naming::quoted(symbol)).unwrap();
            writeln!(out, "\t\t*v = {constant}").unwrap();
            writeln!(out, "\t\treturn nil").unwrap();
        }
        writeln!(out, "\tdefault:").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"invalid {name} symbol %q\", string(text))"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v *{name}) WriteAvro(w io.Writer) error {{").unwrap();
        writeln!(out, "\tif v == nil || !v.Valid() {{").unwrap();
        writeln!(out, "\t\treturn fmt.Errorf(\"invalid {name} value\")").unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\treturn avroWriteInt(w, int32(*v))").unwrap();
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v *{name}) ReadAvro(r io.Reader) error {{").unwrap();
        writeln!(out, "\tif v == nil {{").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"cannot read into a nil {name}\")"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\tvalue, err := avroReadInt(r)").unwrap();
        writeln!(out, "\tif err != nil {{").unwrap();
        writeln!(out, "\t\treturn err").unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\tcandidate := {name}(value)").unwrap();
        writeln!(out, "\tif !candidate.Valid() {{").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"invalid {name} index %d\", value)"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\t*v = candidate").unwrap();
        writeln!(out, "\treturn nil").unwrap();
        writeln!(out, "}}\n").unwrap();
        out.push_str(&marshal_methods(name));
        out.push_str(&named_helpers(name));
        out
    }

    fn render_fixed(&self, fixed: &Fixed) -> String {
        let name = self.model.type_name(&fixed.name);
        let mut out = String::new();
        let logical = fixed.logical.map(crate::schema::LogicalType::avro_name);
        let mut summary = format!(
            "{name} is the {}-byte Avro fixed type `{}`.",
            fixed.size,
            fixed.name.full()
        );
        if let Some(logical) = logical {
            write!(summary, " Its logical type is `{logical}`.").unwrap();
        }
        push_docs(&mut out, &summary, fixed.doc.as_deref());
        writeln!(out, "type {name} [{}]byte\n", fixed.size).unwrap();
        writeln!(out, "func (v *{name}) WriteAvro(w io.Writer) error {{").unwrap();
        writeln!(out, "\tif v == nil {{").unwrap();
        writeln!(out, "\t\treturn fmt.Errorf(\"cannot write a nil {name}\")").unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\treturn avroWriteRaw(w, v[:])").unwrap();
        writeln!(out, "}}\n").unwrap();
        writeln!(out, "func (v *{name}) ReadAvro(r io.Reader) error {{").unwrap();
        writeln!(out, "\tif v == nil {{").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"cannot read into a nil {name}\")"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\t_, err := io.ReadFull(r, v[:])").unwrap();
        writeln!(out, "\treturn err").unwrap();
        writeln!(out, "}}\n").unwrap();
        out.push_str(&marshal_methods(name));
        out.push_str(&named_helpers(name));
        out
    }

    #[allow(clippy::too_many_lines)]
    fn render_union(&self, id: usize) -> String {
        let union = &self.model.unions[id];
        let name = union.name.as_deref().unwrap();
        let mut out = String::new();
        writeln!(
            out,
            "// {name} is an Avro union. Exactly one branch must be set."
        )
        .unwrap();
        writeln!(out, "type {name} struct {{").unwrap();
        let fields = union
            .branches
            .iter()
            .enumerate()
            .map(|(index, branch)| StructField {
                name: union.field_names[index].clone(),
                go_type: format!("*{}", self.go_type(branch, None, false)),
                tag: format!("json:\"{},omitempty\"", avro_branch_name(branch)),
                docs: String::new(),
            })
            .collect::<Vec<_>>();
        write_struct_fields(&mut out, &fields);
        writeln!(out, "}}\n").unwrap();

        for (index, branch) in union.branches.iter().enumerate() {
            let field = &union.field_names[index];
            let go_type = self.go_type(branch, None, false);
            if matches!(branch, SchemaNode::Null) {
                writeln!(out, "func New{name}From{field}() {name} {{").unwrap();
                writeln!(out, "\treturn {name}{{{field}: &struct{{}}{{}}}}").unwrap();
            } else {
                writeln!(out, "func New{name}From{field}(value {go_type}) {name} {{").unwrap();
                writeln!(out, "\treturn {name}{{{field}: &value}}").unwrap();
            }
            writeln!(out, "}}\n").unwrap();
        }

        writeln!(out, "func (v {name}) Validate() error {{").unwrap();
        writeln!(out, "\tbranches := 0").unwrap();
        for field in &union.field_names {
            writeln!(out, "\tif v.{field} != nil {{").unwrap();
            writeln!(out, "\t\tbranches++").unwrap();
            writeln!(out, "\t}}").unwrap();
        }
        writeln!(out, "\tif branches != 1 {{").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"{name} requires exactly one branch, got %d\", branches)"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\treturn nil").unwrap();
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v *{name}) WriteAvro(w io.Writer) error {{").unwrap();
        writeln!(out, "\tif v == nil {{").unwrap();
        writeln!(out, "\t\treturn fmt.Errorf(\"cannot write a nil {name}\")").unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\tif err := v.Validate(); err != nil {{").unwrap();
        writeln!(out, "\t\treturn err").unwrap();
        writeln!(out, "\t}}").unwrap();
        for (index, branch) in union.branches.iter().enumerate() {
            let field = &union.field_names[index];
            let writer = indent_lines(&self.write_fn(branch, None, false), "\t\t");
            writeln!(out, "\tif v.{field} != nil {{").unwrap();
            writeln!(
                out,
                "\t\tif err := avroWriteLong(w, {index}); err != nil {{"
            )
            .unwrap();
            writeln!(out, "\t\t\treturn err").unwrap();
            writeln!(out, "\t\t}}").unwrap();
            writeln!(out, "\t\tif err := {writer}(w, *v.{field}); err != nil {{").unwrap();
            writeln!(out, "\t\t\treturn err").unwrap();
            writeln!(out, "\t\t}}").unwrap();
            writeln!(out, "\t\treturn nil").unwrap();
            writeln!(out, "\t}}").unwrap();
        }
        writeln!(out, "\treturn fmt.Errorf(\"invalid {name}\")").unwrap();
        writeln!(out, "}}\n").unwrap();

        writeln!(out, "func (v *{name}) ReadAvro(r io.Reader) error {{").unwrap();
        writeln!(out, "\tif v == nil {{").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"cannot read into a nil {name}\")"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\tindex, err := avroReadLong(r)").unwrap();
        writeln!(out, "\tif err != nil {{").unwrap();
        writeln!(out, "\t\treturn err").unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "\t*v = {name}{{}}").unwrap();
        writeln!(out, "\tswitch index {{").unwrap();
        for (index, branch) in union.branches.iter().enumerate() {
            let field = &union.field_names[index];
            let reader = indent_lines(&self.read_fn(branch, None, false), "\t\t");
            writeln!(out, "\tcase {index}:").unwrap();
            writeln!(out, "\t\tbranch{index}, err := {reader}(r)").unwrap();
            writeln!(out, "\t\tif err != nil {{").unwrap();
            writeln!(out, "\t\t\treturn err").unwrap();
            writeln!(out, "\t\t}}").unwrap();
            writeln!(out, "\t\tv.{field} = &branch{index}").unwrap();
            writeln!(out, "\t\treturn nil").unwrap();
        }
        writeln!(out, "\tdefault:").unwrap();
        writeln!(
            out,
            "\t\treturn fmt.Errorf(\"invalid {name} union index %d\", index)"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out, "}}\n").unwrap();
        out.push_str(&marshal_methods(name));
        out.push_str(&named_helpers(name));
        out
    }

    pub(super) fn go_type(
        &self,
        node: &SchemaNode,
        owner: Option<&FullName>,
        direct: bool,
    ) -> String {
        match node {
            SchemaNode::Null => "struct{}".to_owned(),
            SchemaNode::Boolean => "bool".to_owned(),
            SchemaNode::Int => "int32".to_owned(),
            SchemaNode::Long => "int64".to_owned(),
            SchemaNode::Float => "float32".to_owned(),
            SchemaNode::Double => "float64".to_owned(),
            SchemaNode::Bytes => "[]byte".to_owned(),
            SchemaNode::String => "string".to_owned(),
            SchemaNode::Record(name) | SchemaNode::Enum(name) | SchemaNode::Fixed(name) => {
                let go_name = self.model.type_name(name);
                if direct && owner.is_some_and(|owner| self.model.needs_pointer(owner, name)) {
                    format!("*{go_name}")
                } else {
                    go_name.to_owned()
                }
            }
            SchemaNode::Array(items) => format!("[]{}", self.go_type(items, owner, false)),
            SchemaNode::Map(values) => format!("map[string]{}", self.go_type(values, owner, false)),
            SchemaNode::Union(branches) => match rendering(branches) {
                UnionRendering::Transparent => self.go_type(&branches[0], owner, direct),
                UnionRendering::Nullable { other } => {
                    format!("*{}", self.go_type(&branches[other], owner, false))
                }
                UnionRendering::Struct => {
                    let id = self.model.union_id(branches);
                    self.model.unions[id].name.clone().unwrap()
                }
            },
            SchemaNode::Logical { base, kind } => match kind {
                LogicalType::Date
                | LogicalType::TimestampMillis
                | LogicalType::TimestampMicros
                | LogicalType::TimestampNanos
                | LogicalType::LocalTimestampMillis
                | LogicalType::LocalTimestampMicros
                | LogicalType::LocalTimestampNanos => "time.Time".to_owned(),
                LogicalType::TimeMillis | LogicalType::TimeMicros => "time.Duration".to_owned(),
                _ => self.go_type(base, owner, direct),
            },
        }
    }

    fn write_fn(&self, node: &SchemaNode, owner: Option<&FullName>, direct: bool) -> String {
        match node {
            SchemaNode::Null => "avroWriteNull".to_owned(),
            SchemaNode::Boolean => "avroWriteBoolean".to_owned(),
            SchemaNode::Int => "avroWriteInt".to_owned(),
            SchemaNode::Long => "avroWriteLong".to_owned(),
            SchemaNode::Float => "avroWriteFloat".to_owned(),
            SchemaNode::Double => "avroWriteDouble".to_owned(),
            SchemaNode::Bytes => "avroWriteBytes".to_owned(),
            SchemaNode::String => "avroWriteString".to_owned(),
            SchemaNode::Record(name) | SchemaNode::Enum(name) | SchemaNode::Fixed(name) => {
                let go_name = self.model.type_name(name);
                if direct && owner.is_some_and(|owner| self.model.needs_pointer(owner, name)) {
                    format!(
                        "func(w io.Writer, value *{go_name}) error {{\n\treturn avroWritePointer(w, value, avroWriteNamed{go_name})\n}}"
                    )
                } else {
                    format!("avroWriteNamed{go_name}")
                }
            }
            SchemaNode::Array(items) => {
                let go_type = self.go_type(node, owner, direct);
                let item = indent_lines(&self.write_fn(items, owner, false), "\t");
                format!(
                    "func(w io.Writer, value {go_type}) error {{\n\treturn avroWriteArray(w, value, {item})\n}}"
                )
            }
            SchemaNode::Map(values) => {
                let go_type = self.go_type(node, owner, direct);
                let value = indent_lines(&self.write_fn(values, owner, false), "\t");
                format!(
                    "func(w io.Writer, value {go_type}) error {{\n\treturn avroWriteMap(w, value, {value})\n}}"
                )
            }
            SchemaNode::Union(branches) => match rendering(branches) {
                UnionRendering::Transparent => {
                    let go_type = self.go_type(node, owner, direct);
                    let inner = indent_lines(&self.write_fn(&branches[0], owner, direct), "\t");
                    format!(
                        "func(w io.Writer, value {go_type}) error {{\n\treturn avroWriteSingleUnion(w, value, {inner})\n}}"
                    )
                }
                UnionRendering::Nullable { other } => {
                    let go_type = self.go_type(node, owner, direct);
                    let null_index = usize::from(other == 0);
                    let inner = indent_lines(&self.write_fn(&branches[other], owner, false), "\t");
                    format!(
                        "func(w io.Writer, value {go_type}) error {{\n\treturn avroWriteNullable(w, value, {null_index}, {other}, {inner})\n}}"
                    )
                }
                UnionRendering::Struct => {
                    let id = self.model.union_id(branches);
                    let name = self.model.unions[id].name.as_deref().unwrap();
                    format!("avroWriteNamed{name}")
                }
            },
            SchemaNode::Logical { base, kind } => match kind {
                LogicalType::Date => "avroWriteDate".to_owned(),
                LogicalType::TimeMillis => "avroWriteTimeMillis".to_owned(),
                LogicalType::TimeMicros => "avroWriteTimeMicros".to_owned(),
                LogicalType::TimestampMillis => "avroWriteTimestampMillis".to_owned(),
                LogicalType::TimestampMicros => "avroWriteTimestampMicros".to_owned(),
                LogicalType::TimestampNanos => "avroWriteTimestampNanos".to_owned(),
                LogicalType::LocalTimestampMillis => "avroWriteLocalTimestampMillis".to_owned(),
                LogicalType::LocalTimestampMicros => "avroWriteLocalTimestampMicros".to_owned(),
                LogicalType::LocalTimestampNanos => "avroWriteLocalTimestampNanos".to_owned(),
                _ => self.write_fn(base, owner, direct),
            },
        }
    }

    fn read_fn(&self, node: &SchemaNode, owner: Option<&FullName>, direct: bool) -> String {
        match node {
            SchemaNode::Null => "avroReadNull".to_owned(),
            SchemaNode::Boolean => "avroReadBoolean".to_owned(),
            SchemaNode::Int => "avroReadInt".to_owned(),
            SchemaNode::Long => "avroReadLong".to_owned(),
            SchemaNode::Float => "avroReadFloat".to_owned(),
            SchemaNode::Double => "avroReadDouble".to_owned(),
            SchemaNode::Bytes => "avroReadBytes".to_owned(),
            SchemaNode::String => "avroReadString".to_owned(),
            SchemaNode::Record(name) | SchemaNode::Enum(name) | SchemaNode::Fixed(name) => {
                let go_name = self.model.type_name(name);
                if direct && owner.is_some_and(|owner| self.model.needs_pointer(owner, name)) {
                    format!(
                        "func(r io.Reader) (*{go_name}, error) {{\n\treturn avroReadPointer(r, avroReadNamed{go_name})\n}}"
                    )
                } else {
                    format!("avroReadNamed{go_name}")
                }
            }
            SchemaNode::Array(items) => {
                let go_type = self.go_type(node, owner, direct);
                let item = indent_lines(&self.read_fn(items, owner, false), "\t");
                format!(
                    "func(r io.Reader) ({go_type}, error) {{\n\treturn avroReadArray(r, {item})\n}}"
                )
            }
            SchemaNode::Map(values) => {
                let go_type = self.go_type(node, owner, direct);
                let value = indent_lines(&self.read_fn(values, owner, false), "\t");
                format!(
                    "func(r io.Reader) ({go_type}, error) {{\n\treturn avroReadMap(r, {value})\n}}"
                )
            }
            SchemaNode::Union(branches) => match rendering(branches) {
                UnionRendering::Transparent => {
                    let go_type = self.go_type(node, owner, direct);
                    let inner = indent_lines(&self.read_fn(&branches[0], owner, direct), "\t");
                    format!(
                        "func(r io.Reader) ({go_type}, error) {{\n\treturn avroReadSingleUnion(r, {inner})\n}}"
                    )
                }
                UnionRendering::Nullable { other } => {
                    let go_type = self.go_type(node, owner, direct);
                    let null_index = usize::from(other == 0);
                    let inner = indent_lines(&self.read_fn(&branches[other], owner, false), "\t");
                    format!(
                        "func(r io.Reader) ({go_type}, error) {{\n\treturn avroReadNullable(r, {null_index}, {other}, {inner})\n}}"
                    )
                }
                UnionRendering::Struct => {
                    let id = self.model.union_id(branches);
                    let name = self.model.unions[id].name.as_deref().unwrap();
                    format!("avroReadNamed{name}")
                }
            },
            SchemaNode::Logical { base, kind } => match kind {
                LogicalType::Date => "avroReadDate".to_owned(),
                LogicalType::TimeMillis => "avroReadTimeMillis".to_owned(),
                LogicalType::TimeMicros => "avroReadTimeMicros".to_owned(),
                LogicalType::TimestampMillis => "avroReadTimestampMillis".to_owned(),
                LogicalType::TimestampMicros => "avroReadTimestampMicros".to_owned(),
                LogicalType::TimestampNanos => "avroReadTimestampNanos".to_owned(),
                LogicalType::LocalTimestampMillis => "avroReadLocalTimestampMillis".to_owned(),
                LogicalType::LocalTimestampMicros => "avroReadLocalTimestampMicros".to_owned(),
                LogicalType::LocalTimestampNanos => "avroReadLocalTimestampNanos".to_owned(),
                _ => self.read_fn(base, owner, direct),
            },
        }
    }
}

fn indent_lines(value: &str, indent: &str) -> String {
    value.replace('\n', &format!("\n{indent}"))
}

struct StructField {
    name: String,
    go_type: String,
    tag: String,
    docs: String,
}

fn write_struct_fields(out: &mut String, fields: &[StructField]) {
    let mut start = 0;
    for end in 1..=fields.len() {
        if end < fields.len() && fields[end].docs.is_empty() {
            continue;
        }
        let group = &fields[start..end];
        let name_width = group
            .iter()
            .map(|field| field.name.len())
            .max()
            .unwrap_or(0);
        let type_width = group
            .iter()
            .map(|field| field.go_type.len())
            .max()
            .unwrap_or(0);
        for field in group {
            out.push_str(&field.docs);
            let name = &field.name;
            let go_type = &field.go_type;
            let tag = &field.tag;
            writeln!(out, "\t{name:<name_width$} {go_type:<type_width$} `{tag}`").unwrap();
        }
        start = end;
    }
}

fn marshal_methods(name: &str) -> String {
    format!(
        "func (v *{name}) MarshalAvro() ([]byte, error) {{\n\
         \tvar buffer bytes.Buffer\n\
         \tif err := v.WriteAvro(&buffer); err != nil {{\n\
         \t\treturn nil, err\n\
         \t}}\n\
         \treturn buffer.Bytes(), nil\n\
         }}\n\n\
         func (v *{name}) UnmarshalAvro(data []byte) error {{\n\
         \treturn v.ReadAvro(bytes.NewReader(data))\n\
         }}\n\n"
    )
}

fn named_helpers(name: &str) -> String {
    format!(
        "func avroWriteNamed{name}(w io.Writer, value {name}) error {{\n\
         \treturn value.WriteAvro(w)\n\
         }}\n\n\
         func avroReadNamed{name}(r io.Reader) ({name}, error) {{\n\
         \tvar value {name}\n\
         \tvar zero {name}\n\
         \tif err := value.ReadAvro(r); err != nil {{\n\
         \t\treturn zero, err\n\
         \t}}\n\
         \treturn value, nil\n\
         }}\n\n"
    )
}

fn logical_name(node: &SchemaNode) -> Option<&'static str> {
    match node {
        SchemaNode::Logical { kind, .. } => Some(kind.avro_name()),
        _ => None,
    }
}

fn avro_branch_name(node: &SchemaNode) -> String {
    match node {
        SchemaNode::Record(name) | SchemaNode::Enum(name) | SchemaNode::Fixed(name) => name.full(),
        SchemaNode::Array(_) => "array".to_owned(),
        SchemaNode::Map(_) => "map".to_owned(),
        SchemaNode::Union(_) => "union".to_owned(),
        SchemaNode::Logical { base, .. } => avro_branch_name(base),
        other => other.avro_type_name().to_owned(),
    }
}

fn push_docs(out: &mut String, summary: &str, doc: Option<&str>) {
    writeln!(out, "// {summary}").unwrap();
    if let Some(doc) = doc {
        writeln!(out, "//").unwrap();
        for line in doc.lines() {
            writeln!(out, "// {line}").unwrap();
        }
    }
}

fn push_docs_indented(
    out: &mut String,
    summary: &str,
    doc: Option<&str>,
    logical: Option<&str>,
    indent: &str,
) {
    writeln!(out, "{indent}// {summary}").unwrap();
    if doc.is_some() || logical.is_some() {
        writeln!(out, "{indent}//").unwrap();
    }
    if let Some(doc) = doc {
        for line in doc.lines() {
            writeln!(out, "{indent}// {line}").unwrap();
        }
    }
    if let Some(logical) = logical {
        writeln!(out, "{indent}// Avro logical type: `{logical}`.").unwrap();
    }
}
