use super::generate;
use crate::{GeneratedFile, Layout, RustOptions, RustTime, RustTypes};

fn generate_with(schema: &str, layout: Layout, options: RustOptions) -> Vec<GeneratedFile> {
    let document = crate::schema::parse(schema, crate::AvroVersion::V1_12)
        .unwrap_or_else(|error| panic!("{error}"));
    generate(&document, layout, options).unwrap_or_else(|error| panic!("{error}"))
}

fn generate_ok(schema: &str) -> Vec<GeneratedFile> {
    generate_with(schema, Layout::SingleFile, RustOptions::default())
}

fn generate_module(schema: &str) -> Vec<GeneratedFile> {
    generate_with(schema, Layout::Module, RustOptions::default())
}

fn generate_std(schema: &str) -> Vec<GeneratedFile> {
    generate_with(
        schema,
        Layout::SingleFile,
        RustOptions {
            types: RustTypes::Std,
            ..RustOptions::default()
        },
    )
}

fn generate_with_time(schema: &str, time: RustTime) -> Vec<GeneratedFile> {
    generate_with(
        schema,
        Layout::SingleFile,
        RustOptions {
            time,
            ..RustOptions::default()
        },
    )
}

fn generate_err_time(schema: &str, time: RustTime) -> String {
    let document = crate::schema::parse(schema, crate::AvroVersion::V1_12)
        .unwrap_or_else(|error| panic!("{error}"));
    generate(
        &document,
        Layout::SingleFile,
        RustOptions {
            time,
            ..RustOptions::default()
        },
    )
    .expect_err("generation should fail")
    .to_string()
}

fn contents_of_time(schema: &str, time: RustTime) -> String {
    contents(&generate_with_time(schema, time)).to_owned()
}

fn generate_err(schema: &str) -> String {
    let document = crate::schema::parse(schema, crate::AvroVersion::V1_12)
        .unwrap_or_else(|error| panic!("{error}"));
    generate(&document, Layout::SingleFile, RustOptions::default())
        .expect_err("generation should fail")
        .to_string()
}

fn contents(files: &[GeneratedFile]) -> &str {
    assert_eq!(files.len(), 1, "expected a single generated file");
    &files[0].contents
}

fn contents_of(schema: &str) -> String {
    contents(&generate_ok(schema)).to_owned()
}

fn contents_of_std(schema: &str) -> String {
    contents(&generate_std(schema)).to_owned()
}

fn generate_err_std(schema: &str) -> String {
    let document = crate::schema::parse(schema, crate::AvroVersion::V1_12)
        .unwrap_or_else(|error| panic!("{error}"));
    generate(
        &document,
        Layout::SingleFile,
        RustOptions {
            types: RustTypes::Std,
            ..RustOptions::default()
        },
    )
    .expect_err("generation should fail")
    .to_string()
}

fn generate_with_derives(schema: &str, derives: &[&str]) -> Vec<GeneratedFile> {
    generate_with(
        schema,
        Layout::SingleFile,
        RustOptions {
            derives: derives.iter().map(|derive| (*derive).to_owned()).collect(),
            ..RustOptions::default()
        },
    )
}

fn generate_err_derives(schema: &str, derives: &[&str]) -> String {
    let document = crate::schema::parse(schema, crate::AvroVersion::V1_12)
        .unwrap_or_else(|error| panic!("{error}"));
    generate(
        &document,
        Layout::SingleFile,
        RustOptions {
            derives: derives.iter().map(|derive| (*derive).to_owned()).collect(),
            ..RustOptions::default()
        },
    )
    .expect_err("generation should fail")
    .to_string()
}

fn module_file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    let file = files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("`{path}` was not generated"));
    &file.contents
}

#[test]
fn generates_a_simple_record() {
    let files = generate_ok(
        r#"{"type": "record", "name": "User", "doc": "A user.", "fields": [
            {"name": "id", "type": "long", "doc": "Unique id."},
            {"name": "email", "type": ["null", "string"]}
        ]}"#,
    );
    assert_eq!(files[0].path, "user.rs");
    let contents = contents(&files);
    assert!(contents.contains("/// A user."));
    assert!(contents.contains("/// Unique id."));
    assert!(contents.contains("pub struct User {"));
    assert!(contents.contains("pub id: i64,"));
    assert!(contents.contains("pub email: Option<String>,"));
    assert!(contents.contains("use serde::{Deserialize, Serialize};"));
}

#[test]
fn renames_fields_that_are_not_snake_case() {
    let contents = contents_of(
        r#"{"type": "record", "name": "R", "fields": [
            {"name": "fullName", "type": "string"},
            {"name": "type", "type": "string"},
            {"name": "self", "type": "long"}
        ]}"#,
    );
    assert!(contents.contains(r#"#[serde(rename = "fullName")]"#));
    assert!(contents.contains("pub full_name: String,"));
    assert!(contents.contains(r#"#[serde(rename = "type")]"#));
    assert!(contents.contains("pub r#type: String,"));
    assert!(contents.contains(r#"#[serde(rename = "self")]"#));
    assert!(contents.contains("pub self_: i64,"));
}

#[test]
fn generates_enums_with_default_and_renames() {
    let contents = contents_of(
        r#"{"type": "record", "name": "R", "fields": [
            {"name": "suit", "type": {
                "type": "enum", "name": "Suit", "doc": "Suits.",
                "symbols": ["SPADES", "HEARTS"], "default": "SPADES"
            }}
        ]}"#,
    );
    assert!(contents.contains("/// Suits."));
    assert!(contents.contains("Default"));
    assert!(contents.contains("pub enum Suit {"));
    assert!(contents.contains(r#"#[serde(rename = "SPADES")]"#));
    assert!(contents.contains("Spades,"));
    assert!(contents.contains("#[default]"));
}

#[test]
fn generates_union_enums_and_deduplicates_them() {
    let contents = contents_of(
        r#"{"type": "record", "name": "R", "fields": [
            {"name": "a", "type": ["int", "string"]},
            {"name": "b", "type": ["int", "string"]},
            {"name": "c", "type": ["null", "int", "string"]}
        ]}"#,
    );
    assert!(contents.contains("pub enum IntOrString {"));
    assert!(contents.contains("Int(i32),"));
    assert!(contents.contains("String(String),"));
    assert_eq!(contents.matches("pub enum IntOrString {").count(), 1);
    assert!(contents.contains("pub enum NullOrIntOrString {"));
    assert!(contents.contains("Null(()),"));
}

#[test]
fn boxes_recursive_references() {
    let contents = contents_of(
        r#"{"type": "record", "name": "LongList", "fields": [
            {"name": "value", "type": "long"},
            {"name": "next", "type": ["null", "LongList"]}
        ]}"#,
    );
    assert!(contents.contains("pub next: Option<Box<LongList>>,"));

    let contents = contents_of(
        r#"{"type": "record", "name": "Chain", "fields": [
            {"name": "next", "type": "Chain"}
        ]}"#,
    );
    assert!(contents.contains("pub next: Box<Chain>,"));

    let contents = contents_of(
        r#"{"type": "record", "name": "Tree", "fields": [
            {"name": "children", "type": {"type": "array", "items": "Tree"}}
        ]}"#,
    );
    assert!(contents.contains("pub children: Vec<Tree>,"));
}

#[test]
fn generates_fixed_aliases_and_maps() {
    let contents = contents_of(
        r#"{"type": "record", "name": "R", "fields": [
            {"name": "md5", "type": {"type": "fixed", "name": "Md5", "size": 16}},
            {"name": "counts", "type": {"type": "map", "values": "long"}},
            {"name": "payload", "type": "bytes"}
        ]}"#,
    );
    assert!(contents.contains("pub type Md5 = ByteArray<16>;"));
    assert!(contents.contains("pub counts: HashMap<String, i64>,"));
    assert!(contents.contains("pub payload: ByteBuf,"));
    assert!(contents.contains("use serde_bytes::{ByteArray, ByteBuf};"));
    assert!(contents.contains("use std::collections::HashMap;"));
}

#[test]
fn documents_logical_types() {
    let contents = contents_of(
        r#"{"type": "record", "name": "R", "fields": [
            {"name": "date", "type": {"type": "int", "logicalType": "date"}},
            {"name": "amount", "type": {"type": "bytes", "logicalType": "decimal",
                "precision": 4, "scale": 2}}
        ]}"#,
    );
    assert!(contents.contains("Logical type `date`: days from the unix epoch."));
    assert!(contents.contains("pub date: i32,"));
    assert!(contents.contains(
        "Logical type `decimal`: arbitrary-precision signed decimal \
                           (precision 4, scale 2)."
    ));
    assert!(contents.contains("pub amount: apache_avro::Decimal,"));
}

const LOGICAL_SCHEMA: &str = r#"{"type": "record", "name": "R", "fields": [
    {"name": "uuidString", "type": {"type": "string", "logicalType": "uuid"}},
    {"name": "uuidFixed", "type": {"type": "fixed", "name": "U", "size": 16,
        "logicalType": "uuid"}},
    {"name": "decimal", "type": {"type": "bytes", "logicalType": "decimal",
        "precision": 4, "scale": 2}},
    {"name": "decimalFixed", "type": {"type": "fixed", "name": "P", "size": 8,
        "logicalType": "decimal", "precision": 18, "scale": 6}},
    {"name": "bigDecimal", "type": {"type": "bytes", "logicalType": "big-decimal"}},
    {"name": "duration", "type": {"type": "fixed", "name": "D", "size": 12,
        "logicalType": "duration"}},
    {"name": "date", "type": {"type": "int", "logicalType": "date"}},
    {"name": "payload", "type": "bytes"},
    {"name": "digest", "type": {"type": "fixed", "name": "Digest", "size": 4}}
]}"#;

#[test]
fn avro_types_use_apache_avro_types() {
    let contents = contents_of(LOGICAL_SCHEMA);
    assert!(contents.contains("pub uuid_string: String,"));
    assert!(contents.contains("pub uuid_fixed: apache_avro::Uuid,"));
    assert!(contents.contains("pub decimal: apache_avro::Decimal,"));
    assert!(contents.contains("pub decimal_fixed: apache_avro::Decimal,"));
    assert!(contents.contains("pub big_decimal: apache_avro::Decimal,"));
    assert!(contents.contains("pub duration: apache_avro::Duration,"));
    assert!(contents.contains("pub date: i32,"));
    assert!(contents.contains("pub payload: ByteBuf,"));
    assert!(contents.contains("pub type Digest = ByteArray<4>;"));
    assert!(contents.contains("pub type U = ByteArray<16>;"));
    assert!(contents.contains("use serde_bytes::{ByteArray, ByteBuf};"));
    assert!(!contents.contains("use apache_avro"));
}

#[test]
fn std_types_use_only_std_and_generated_helpers() {
    let contents = contents_of_std(LOGICAL_SCHEMA);
    assert!(contents.contains("pub uuid_string: String,"));
    assert!(contents.contains("pub uuid_fixed: U,"));
    assert!(contents.contains("pub decimal: Bytes,"));
    assert!(contents.contains("pub decimal_fixed: P,"));
    assert!(contents.contains("pub big_decimal: Bytes,"));
    assert!(contents.contains("pub duration: D,"));
    assert!(contents.contains("pub date: i32,"));
    assert!(contents.contains("pub payload: Bytes,"));
    assert!(contents.contains("pub digest: Digest,"));
    assert!(contents.contains("pub struct Bytes(pub Vec<u8>);"));
    assert!(contents.contains("impl serde::Serialize for Bytes"));
    assert!(contents.contains("pub struct U(pub [u8; 16]);"));
    assert!(contents.contains("impl serde::Serialize for U"));
    assert!(contents.contains("pub struct Digest(pub [u8; 4]);"));
    assert!(contents.contains("pub struct D(pub [u8; 12]);"));
    assert!(!contents.contains("serde_bytes"));
    assert!(!contents.contains("apache_avro"));
    assert!(!contents.contains("pub type"));
}

#[test]
fn std_mode_rejects_the_bytes_type_name() {
    let files = generate_ok(r#"{"type": "record", "name": "Bytes", "fields": []}"#);
    assert_eq!(files.len(), 1);
    assert_eq!(
        generate_err_std(r#"{"type": "record", "name": "Bytes", "fields": []}"#),
        "unsupported schema: type name `Bytes` (from type `Bytes`) collides with a name \
         used by the generated code"
    );
}

const TIME_SCHEMA: &str = r#"{"type": "record", "name": "T", "fields": [
    {"name": "date", "type": {"type": "int", "logicalType": "date"}},
    {"name": "timeMillis", "type": {"type": "int", "logicalType": "time-millis"}},
    {"name": "timestamp", "type": {"type": "long", "logicalType": "timestamp-millis"}},
    {"name": "localNanos", "type": {"type": "long", "logicalType": "local-timestamp-nanos"}},
    {"name": "optional", "type": ["null", {"type": "long", "logicalType": "timestamp-micros"}]},
    {"name": "either", "type": [
        {"type": "long", "logicalType": "timestamp-nanos"},
        "string"
    ]}
]}"#;

#[test]
fn primitives_mode_keeps_time_integers() {
    let contents = contents_of(TIME_SCHEMA);
    assert!(contents.contains("pub date: i32,"));
    assert!(contents.contains("pub time_millis: i32,"));
    assert!(contents.contains("pub timestamp: i64,"));
    assert!(contents.contains("pub local_nanos: i64,"));
    assert!(contents.contains("pub optional: Option<i64>,"));
    assert!(!contents.contains("struct TimestampMillis"));
    assert!(!contents.contains("chrono"));
    assert!(!contents.contains("std::time"));
}

#[test]
fn std_time_mode_wraps_times_in_std_types() {
    let contents = contents_of_time(TIME_SCHEMA, RustTime::StdTime);
    assert!(contents.contains("pub date: i32,"));
    assert!(contents.contains("pub time_millis: TimeMillis,"));
    assert!(contents.contains("pub struct TimeMillis(pub std::time::Duration);"));
    assert!(contents.contains("pub timestamp: TimestampMillis,"));
    assert!(
        contents.contains("pub struct TimestampMillis(pub std::time::SystemTime);"),
        "missing wrapper:\n{contents}"
    );
    assert!(contents.contains("pub local_nanos: TimestampNanos,"));
    assert!(contents.contains("pub struct TimestampNanos(pub std::time::SystemTime);"));
    assert!(contents.contains("pub optional: Option<TimestampMicros>,"));
    assert!(contents.contains("pub enum TimestampNanosOrString {"));
    assert!(contents.contains("TimestampNanos(TimestampNanos),"));
    assert!(contents.contains("impl From<std::time::SystemTime> for TimestampMillis"));
    assert!(!contents.contains("chrono"));
}

#[test]
fn chrono_mode_wraps_times_in_chrono_types() {
    let contents = contents_of_time(TIME_SCHEMA, RustTime::Chrono);
    assert!(contents.contains("pub date: Date,"));
    assert!(contents.contains("pub struct Date(pub chrono::NaiveDate);"));
    assert!(contents.contains("pub time_millis: TimeMillis,"));
    assert!(contents.contains("pub struct TimeMillis(pub chrono::NaiveTime);"));
    assert!(
        contents.contains("pub struct TimestampMillis(pub chrono::DateTime<chrono::Utc>);"),
        "missing wrapper:\n{contents}"
    );
    assert!(contents.contains("pub local_nanos: LocalTimestampNanos,"));
    assert!(contents.contains("pub struct LocalTimestampNanos(pub chrono::NaiveDateTime);"));
    assert!(contents.contains("pub optional: Option<TimestampMicros>,"));
    assert!(contents.contains("pub enum TimestampNanosOrString {"));
    assert!(contents.contains("impl From<chrono::NaiveDate> for Date"));
    assert!(contents.contains("chrono::Timelike::hour(&self.0)"));
    assert!(!contents.contains("std::time"));
}

#[test]
fn time_modes_reject_colliding_type_names() {
    let files = generate_ok(r#"{"type": "record", "name": "TimestampMillis", "fields": []}"#);
    assert_eq!(files.len(), 1);
    for time in [RustTime::StdTime, RustTime::Chrono] {
        assert_eq!(
            generate_err_time(
                r#"{"type": "record", "name": "TimestampMillis", "fields": []}"#,
                time
            ),
            "unsupported schema: type name `TimestampMillis` (from type \
             `TimestampMillis`) collides with a name used by the generated code"
        );
    }
    assert!(
        generate_err_time(
            r#"{"type": "record", "name": "Time", "fields": []}"#,
            RustTime::StdTime
        )
        .contains("type name `Time`")
    );
    assert!(
        generate_err_time(
            r#"{"type": "record", "name": "Date", "fields": []}"#,
            RustTime::Chrono
        )
        .contains("type name `Date`")
    );
    let files = generate_ok(r#"{"type": "record", "name": "Date", "fields": []}"#);
    assert_eq!(files.len(), 1);
}

#[test]
fn chrono_module_layout_gets_a_time_file() {
    let files = generate_with(
        TIME_SCHEMA,
        Layout::Module,
        RustOptions {
            time: RustTime::Chrono,
            ..RustOptions::default()
        },
    );
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["mod.rs", "time.rs", "t.rs"]);
    let mod_rs = module_file(&files, "mod.rs");
    assert!(mod_rs.contains("pub mod time;"));
    assert!(mod_rs.contains("pub use time::{"));
    for name in [
        "Date",
        "TimeMillis",
        "TimestampMillis",
        "TimestampMicros",
        "TimestampNanos",
        "LocalTimestampNanos",
    ] {
        assert!(mod_rs.contains(name), "missing `{name}` in:\n{mod_rs}");
    }
    assert!(mod_rs.contains("pub use t::T;"));
    assert!(module_file(&files, "time.rs").contains("pub struct Date(pub chrono::NaiveDate);"));
    let record = module_file(&files, "t.rs");
    assert!(record.contains("use super::{"));
    for name in [
        "Date",
        "TimeMillis",
        "TimestampMillis",
        "TimestampNanos",
        "TimestampNanosOrString",
    ] {
        assert!(record.contains(name), "missing `{name}` in:\n{record}");
    }
    assert!(record.contains("pub date: Date,"));
    assert!(!record.contains("chrono::"));
}

#[test]
fn std_module_layout_gets_a_bytes_file() {
    let files = generate_with(
        r#"{"type": "record", "name": "R", "fields": [{"name": "b", "type": "bytes"}]}"#,
        Layout::Module,
        RustOptions {
            types: RustTypes::Std,
            ..RustOptions::default()
        },
    );
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["mod.rs", "bytes.rs", "r.rs"]);
    assert!(module_file(&files, "mod.rs").contains("pub mod bytes;"));
    assert!(module_file(&files, "mod.rs").contains("pub use bytes::Bytes;"));
    assert!(module_file(&files, "bytes.rs").contains("pub struct Bytes(pub Vec<u8>);"));
    let record = module_file(&files, "r.rs");
    assert!(record.contains("use super::Bytes;"));
    assert!(record.contains("pub b: Bytes,"));
    assert!(!record.contains("serde_bytes"));
}

#[test]
fn generates_nothing_for_unnamed_primitive_roots() {
    assert_eq!(generate_ok(r#""int""#), [] as [GeneratedFile; 0]);
    assert_eq!(
        generate_ok(r#"{"type": "array", "items": "long"}"#),
        [] as [GeneratedFile; 0]
    );
}

#[test]
fn names_files_after_the_root_type() {
    let files = generate_ok(
        r#"{"type": "record", "name": "UserProfile", "namespace": "com.example",
            "fields": []}"#,
    );
    assert_eq!(files[0].path, "user_profile.rs");

    let files = generate_ok(
        r#"[
            {"type": "record", "name": "A", "fields": []},
            "null",
            "string"
        ]"#,
    );
    assert_eq!(files[0].path, "schema.rs");
    assert!(contents(&files).contains("pub enum AOrNullOrString {"));
}

#[test]
fn generates_module_layout() {
    let files = generate_module(
        r#"{"type": "record", "name": "Order", "fields": [
            {"name": "id", "type": "long"},
            {"name": "customer", "type": {
                "type": "record", "name": "Customer",
                "fields": [{"name": "name", "type": "string"}]
            }},
            {"name": "status", "type": {
                "type": "enum", "name": "Status", "symbols": ["NEW"]
            }},
            {"name": "payment", "type": ["null", "Customer", "string"]},
            {"name": "weight", "type": "bytes"},
            {"name": "tags", "type": {"type": "map", "values": "string"}},
            {"name": "digest", "type": {"type": "fixed", "name": "Digest", "size": 4}}
        ]}"#,
    );
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "mod.rs",
            "order.rs",
            "customer.rs",
            "status.rs",
            "digest.rs"
        ]
    );

    let mod_rs = module_file(&files, "mod.rs");
    assert!(mod_rs.contains("pub mod order;"));
    assert!(mod_rs.contains("pub mod customer;"));
    assert!(mod_rs.contains("pub use order::Order;"));
    assert!(mod_rs.contains("pub use customer::Customer;"));
    assert!(mod_rs.contains("pub use status::Status;"));
    assert!(
        mod_rs.contains("pub enum NullOrCustomerOrString {"),
        "unexpected mod.rs:\n{mod_rs}"
    );
    assert!(mod_rs.contains("use serde::{Deserialize, Serialize};"));
    assert!(!mod_rs.contains("use super::"));

    let order = module_file(&files, "order.rs");
    assert!(
        order.contains("use super::{Customer, Digest, NullOrCustomerOrString, Status};"),
        "unexpected imports in order.rs:\n{order}"
    );
    assert!(order.contains("use serde_bytes::ByteBuf;"));
    assert!(order.contains("use std::collections::HashMap;"));
    assert!(order.contains("pub weight: ByteBuf,"));
    assert!(order.contains("pub digest: Digest,"));
    assert!(!order.contains("use super::Order"));

    let customer = module_file(&files, "customer.rs");
    assert!(customer.contains("pub struct Customer {"));
    assert!(!customer.contains("use super::"), "no unused imports");

    let digest = module_file(&files, "digest.rs");
    assert!(digest.contains("pub type Digest = ByteArray<4>;"));
    assert!(digest.contains("use serde_bytes::ByteArray;"));
    assert!(!digest.contains("use serde::{"));
}

#[test]
fn module_layout_with_root_union_only() {
    let files = generate_module(
        r#"[
            {"type": "record", "name": "A", "fields": []},
            "null",
            "string"
        ]"#,
    );
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["mod.rs", "a.rs"]);
    let mod_rs = module_file(&files, "mod.rs");
    assert!(mod_rs.contains("pub mod a;"));
    assert!(mod_rs.contains("pub use a::A;"));
    assert!(mod_rs.contains("pub enum AOrNullOrString {"));
}

#[test]
fn module_layout_without_named_types() {
    assert_eq!(generate_module(r#""int""#), [] as [GeneratedFile; 0]);
    assert_eq!(
        generate_module(r#"{"type": "array", "items": "long"}"#),
        [] as [GeneratedFile; 0]
    );
    let files = generate_module(r#"["int", "string"]"#);
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["mod.rs"]);
    assert!(module_file(&files, "mod.rs").contains("pub enum IntOrString {"));
}

#[test]
fn module_layout_self_reference_is_not_imported() {
    let files = generate_module(
        r#"{"type": "record", "name": "LongList", "fields": [
            {"name": "value", "type": "long"},
            {"name": "next", "type": ["null", "LongList"]}
        ]}"#,
    );
    let list = module_file(&files, "long_list.rs");
    assert!(!list.contains("use super::"));
    assert!(list.contains("pub next: Option<Box<LongList>>,"));

    let mod_rs = module_file(&files, "mod.rs");
    assert!(!mod_rs.contains("pub enum OptionOfLongList"));
}

#[test]
fn rejects_colliding_type_names() {
    assert_eq!(
        generate_err(
            r#"{"type": "record", "name": "a.Foo", "fields": [
                {"name": "x", "type": {"type": "record", "name": "b.Foo", "fields": []}}
            ]}"#
        ),
        "unsupported schema: multiple types map to Rust type name `Foo` (e.g. `b.Foo`); \
         generating types with colliding names from different namespaces is not supported yet"
    );
}

#[test]
fn rejects_reserved_type_names() {
    assert_eq!(
        generate_err(r#"{"type": "record", "name": "option", "fields": []}"#),
        "unsupported schema: type name `Option` (from type `option`) collides with a name \
         used by the generated code"
    );
}

const DERIVE_SCHEMA: &str = r#"{"type": "record", "name": "R", "fields": [
    {"name": "e", "type": {"type": "enum", "name": "E", "symbols": ["A"]}},
    {"name": "u", "type": ["int", "string"]}
]}"#;

#[test]
fn adds_requested_derives_to_generated_data_types() {
    let files = generate_with_derives(DERIVE_SCHEMA, &["Hash", "derive_more::Display"]);
    let contents = contents(&files);
    assert_eq!(
        contents
            .matches("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, derive_more::Display)]")
            .count(),
        2,
        "records and union enums should get both derives:\n{contents}"
    );
    assert_eq!(
        contents.matches("derive_more::Display").count(),
        3,
        "the enum should only get the missing derive:\n{contents}"
    );
    assert_eq!(
        contents.matches("Hash").count(),
        3,
        "the enum's built-in Hash should not be duplicated:\n{contents}"
    );
}

#[test]
fn ignores_derives_that_are_already_present() {
    let files = generate_with_derives(DERIVE_SCHEMA, &["Hash", "std::fmt::Debug"]);
    let contents = contents(&files);
    assert!(
        !contents.contains("Hash, Hash"),
        "duplicate short derive:\n{contents}"
    );
    assert!(
        !contents.contains("Debug, Debug"),
        "duplicate qualified derive:\n{contents}"
    );
}

#[test]
fn rejects_invalid_derives() {
    let error = generate_err_derives(DERIVE_SCHEMA, &["not a path"]);
    assert!(
        error.starts_with("invalid setting: invalid derive `not a path`:"),
        "unexpected error: {error}"
    );
}
