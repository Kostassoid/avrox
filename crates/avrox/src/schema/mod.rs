//! An intermediate representation of Avro schemas.
//!
//! The JSON text of an `.avsc` file is parsed into a [`Document`], which holds
//! the [`Schema`] tree of the root type together with every named type
//! (`record`, `enum` and `fixed`) defined inside it. Anonymous types are
//! represented inline; references to named types are [`Schema::Record`],
//! [`Schema::Enum`] and [`Schema::Fixed`] carrying the referenced [`FullName`].

use std::fmt;

use serde_json::Value;

mod parse;

pub use parse::parse;

/// A parsed Avro schema document: a root schema plus all named types it
/// defines, in definition order.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// The root schema of the document.
    pub root: Schema,
    /// All named types defined in the document, in definition order
    /// (depth-first, left-to-right through the JSON parse tree).
    pub types: Vec<NamedType>,
}

impl Document {
    /// Looks up a named type by its fullname.
    #[must_use]
    pub fn get(&self, name: &FullName) -> Option<&NamedType> {
        self.types.iter().find(|ty| ty.name() == name)
    }
}

/// A schema type: primitives, named type references, collections, unions and
/// logical type annotations.
#[derive(Debug, Clone, PartialEq)]
pub enum Schema {
    /// The `null` type.
    Null,
    /// The `boolean` type.
    Boolean,
    /// The `int` type.
    Int,
    /// The `long` type.
    Long,
    /// The `float` type.
    Float,
    /// The `double` type.
    Double,
    /// The `bytes` type.
    Bytes,
    /// The `string` type.
    String,
    /// A reference to a `record` defined in the document.
    Record(FullName),
    /// A reference to an `enum` defined in the document.
    Enum(FullName),
    /// A reference to a `fixed` defined in the document.
    Fixed(FullName),
    /// An `array` of `items`.
    Array(Box<Schema>),
    /// A `map` with string keys and `values` values.
    Map(Box<Schema>),
    /// A `union` of the given branch schemas, in declaration order.
    Union(Vec<Schema>),
    /// A logical type annotation applied to `base`.
    Logical {
        /// The schema the logical type annotates.
        base: Box<Schema>,
        /// The logical type.
        kind: LogicalType,
    },
}

impl Schema {
    /// The primitive or complex Avro type name used for binary encoding, e.g.
    /// `int` for `Schema::Int` and `array` for `Schema::Array`.
    #[must_use]
    pub fn avro_type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Int => "int",
            Self::Long => "long",
            Self::Float => "float",
            Self::Double => "double",
            Self::Bytes => "bytes",
            Self::String => "string",
            Self::Record(_) => "record",
            Self::Enum(_) => "enum",
            Self::Fixed(_) => "fixed",
            Self::Array(_) => "array",
            Self::Map(_) => "map",
            Self::Union(_) => "union",
            Self::Logical { .. } => "logical",
        }
    }

    /// A structural key identifying this schema: primitives by type name,
    /// named types by kind and fullname, collections recursively by their
    /// item or value schemas. Used to deduplicate equal unions.
    pub(crate) fn union_branch_key(&self) -> String {
        match self {
            Self::Record(name) => format!("record:{name}"),
            Self::Enum(name) => format!("enum:{name}"),
            Self::Fixed(name) => format!("fixed:{name}"),
            Self::Array(items) => format!("array<{}>", items.union_branch_key()),
            Self::Map(values) => format!("map<{}>", values.union_branch_key()),
            Self::Logical { base, .. } => base.union_branch_key(),
            other => other.avro_type_name().to_owned(),
        }
    }
}

/// A logical type annotating an Avro primitive or complex type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalType {
    /// A `decimal` with the given precision and scale over `bytes` or `fixed`.
    Decimal {
        /// Maximum number of base-10 digits.
        precision: u32,
        /// Number of digits after the decimal point.
        scale: u32,
    },
    /// A `uuid` over `string` or a 16-byte `fixed`.
    Uuid,
    /// A `date`: days from the unix epoch, over `int`.
    Date,
    /// A `time-millis`: milliseconds after midnight, over `int`.
    TimeMillis,
    /// A `time-micros`: microseconds after midnight, over `long`.
    TimeMicros,
    /// A `timestamp-millis`: milliseconds from the unix epoch, over `long`.
    TimestampMillis,
    /// A `timestamp-micros`: microseconds from the unix epoch, over `long`.
    TimestampMicros,
    /// A `timestamp-nanos`: nanoseconds from the unix epoch, over `long`.
    TimestampNanos,
    /// A `local-timestamp-millis` over `long`.
    LocalTimestampMillis,
    /// A `local-timestamp-micros` over `long`.
    LocalTimestampMicros,
    /// A `local-timestamp-nanos` over `long`.
    LocalTimestampNanos,
    /// A `duration` of months, days and milliseconds over a 12-byte `fixed`.
    Duration,
    /// A `big-decimal` of variable scale over `bytes`.
    BigDecimal,
}

impl LogicalType {
    /// The Avro name of the logical type, as it appears in the
    /// `logicalType` attribute.
    #[must_use]
    pub fn avro_name(self) -> &'static str {
        match self {
            Self::Decimal { .. } => "decimal",
            Self::Uuid => "uuid",
            Self::Date => "date",
            Self::TimeMillis => "time-millis",
            Self::TimeMicros => "time-micros",
            Self::TimestampMillis => "timestamp-millis",
            Self::TimestampMicros => "timestamp-micros",
            Self::TimestampNanos => "timestamp-nanos",
            Self::LocalTimestampMillis => "local-timestamp-millis",
            Self::LocalTimestampMicros => "local-timestamp-micros",
            Self::LocalTimestampNanos => "local-timestamp-nanos",
            Self::Duration => "duration",
            Self::BigDecimal => "big-decimal",
        }
    }
}

impl fmt::Display for LogicalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.avro_name())
    }
}

/// A named type defined in a schema document.
#[derive(Debug, Clone, PartialEq)]
pub enum NamedType {
    /// A `record` definition.
    Record(Record),
    /// An `enum` definition.
    Enum(Enum),
    /// A `fixed` definition.
    Fixed(Fixed),
}

impl NamedType {
    /// The fullname of the type.
    #[must_use]
    pub fn name(&self) -> &FullName {
        match self {
            Self::Record(record) => &record.name,
            Self::Enum(enum_) => &enum_.name,
            Self::Fixed(fixed) => &fixed.name,
        }
    }

    /// The aliases of the type, resolved to fullnames.
    #[must_use]
    pub fn aliases(&self) -> &[FullName] {
        match self {
            Self::Record(record) => &record.aliases,
            Self::Enum(enum_) => &enum_.aliases,
            Self::Fixed(fixed) => &fixed.aliases,
        }
    }
}

/// A `record` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Fullname of the record.
    pub name: FullName,
    /// Documentation for the record, if any.
    pub doc: Option<String>,
    /// Alternate names of the record, resolved to fullnames.
    pub aliases: Vec<FullName>,
    /// Fields of the record, in declaration order.
    pub fields: Vec<Field>,
}

/// A field of a [`Record`].
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Name of the field.
    pub name: String,
    /// Documentation for the field, if any.
    pub doc: Option<String>,
    /// Schema of the field.
    pub schema: Schema,
    /// Default value of the field in JSON encoding, if any.
    pub default: Option<Value>,
    /// How the field affects sort ordering of the record.
    pub order: Order,
    /// Alternate names of the field.
    pub aliases: Vec<String>,
}

/// How a record field impacts sort ordering of the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Order {
    /// The field is sorted in ascending order (the default).
    #[default]
    Ascending,
    /// The field is sorted in descending order.
    Descending,
    /// The field is ignored when sorting.
    Ignore,
}

/// An `enum` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    /// Fullname of the enum.
    pub name: FullName,
    /// Documentation for the enum, if any.
    pub doc: Option<String>,
    /// Alternate names of the enum, resolved to fullnames.
    pub aliases: Vec<FullName>,
    /// Symbols of the enum, in declaration order.
    pub symbols: Vec<String>,
    /// Default symbol, used when a reader encounters an unknown symbol.
    pub default: Option<String>,
}

/// A `fixed` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixed {
    /// Fullname of the fixed.
    pub name: FullName,
    /// Documentation for the fixed, if any.
    pub doc: Option<String>,
    /// Alternate names of the fixed, resolved to fullnames.
    pub aliases: Vec<FullName>,
    /// Number of bytes per value.
    pub size: u32,
    /// Logical type annotating this fixed, if any.
    pub logical: Option<LogicalType>,
}

/// The fullname of a named type: a name qualified by an optional
/// dot-separated namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FullName {
    /// Namespace of the name, or `None` for the null namespace.
    pub namespace: Option<String>,
    /// Simple name of the type.
    pub name: String,
}

impl FullName {
    /// Creates a fullname from an optional namespace and a simple name.
    #[must_use]
    pub fn new(namespace: Option<&str>, name: &str) -> Self {
        Self {
            namespace: namespace.map(str::to_owned),
            name: name.to_owned(),
        }
    }

    /// The fully qualified name, e.g. `com.example.User`.
    #[must_use]
    pub fn full(&self) -> String {
        let name = &self.name;
        match &self.namespace {
            Some(namespace) => format!("{namespace}.{name}"),
            None => name.clone(),
        }
    }
}

impl fmt::Display for FullName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.full())
    }
}
