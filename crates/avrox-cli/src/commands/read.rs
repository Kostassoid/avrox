//! The `read` subcommand: prints every record of an Avro data file as JSON,
//! one record per line.

use std::{
    collections::HashMap,
    fmt::Write as _,
    fs::File,
    io::{self, BufReader, BufWriter, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use apache_avro::{
    BigDecimal, Decimal, Duration, Reader,
    schema::{NamesRef, ResolvedSchema, Schema},
    types::Value,
};
use chrono::{DateTime, NaiveTime, SecondsFormat, Utc};
use serde_json::{Map, Value as Json};

/// Read records from an Avro data file and print them as JSON to stdout.
#[derive(Debug, clap::Parser)]
pub(crate) struct Read {
    /// Avro data file (.avro) to read records from.
    input: PathBuf,

    /// Render dates, times, timestamps and durations as human-readable
    /// strings instead of their underlying integer representations.
    #[arg(long)]
    human_readable: bool,
}

/// Runs the `read` subcommand.
pub(crate) fn run(args: &Read) -> Result<()> {
    let file = File::open(&args.input)
        .with_context(|| format!("failed to open `{}`", args.input.display()))?;
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    write_records(BufReader::new(file), args.human_readable, &mut out)
        .with_context(|| format!("failed to read records from `{}`", args.input.display()))?;
    out.flush()
        .context("failed to write the records to stdout")?;
    Ok(())
}

/// Reads every record from an object container file and writes it as a single
/// line of JSON to `out`.
fn write_records<R: io::Read, W: io::Write>(
    reader: R,
    human_readable: bool,
    out: &mut W,
) -> Result<()> {
    let reader = Reader::new(reader).context("failed to read the Avro file header")?;
    let schema = reader.writer_schema().clone();
    let resolved =
        ResolvedSchema::try_from(&schema).context("failed to resolve schema references")?;
    let converter = JsonConverter {
        names: resolved.get_names(),
        human_readable,
    };
    for record in reader {
        let record = record.context("failed to read an Avro record")?;
        let json = converter.convert(&record, Some(&schema), None);
        serde_json::to_writer(&mut *out, &json).context("failed to serialize a record to JSON")?;
        out.write_all(b"\n")
            .context("failed to write a record to stdout")?;
    }
    Ok(())
}

/// Converts Avro values to JSON. Types JSON cannot represent are rendered as
/// the closest lossless approximation: bytes and fixed values become arrays of
/// byte values, decimal and big decimal values become strings, and union
/// branches are unwrapped because their index is not meaningful outside the
/// schema.
struct JsonConverter<'n, 's> {
    names: &'n NamesRef<'s>,
    human_readable: bool,
}

impl<'s> JsonConverter<'_, 's> {
    fn convert(&self, value: &Value, schema: Option<&'s Schema>, enclosing: Option<&str>) -> Json {
        let schema = schema.map(|schema| resolve_schema(schema, self.names, enclosing));
        let enclosing = match schema {
            Some(Schema::Record(record)) => record.name.namespace().or(enclosing),
            _ => enclosing,
        };
        match value {
            Value::Null => Json::Null,
            Value::Boolean(value) => Json::Bool(*value),
            Value::Int(value) => Json::from(*value),
            Value::Long(value) => Json::from(*value),
            Value::Float(value) => number(f64::from(*value)),
            Value::Double(value) => number(*value),
            Value::Bytes(bytes) | Value::Fixed(_, bytes) => bytes_to_json(bytes),
            Value::String(value) => Json::String(value.clone()),
            Value::Enum(_, symbol) => Json::String(symbol.clone()),
            Value::Union(index, value) => {
                let variant = match schema {
                    Some(Schema::Union(union)) => union.variants().get(*index as usize),
                    _ => None,
                };
                self.convert(value, variant, enclosing)
            }
            Value::Array(values) => self.array(values, schema, enclosing),
            Value::Map(entries) => self.map(entries, schema, enclosing),
            Value::Record(fields) => self.record(fields, schema, enclosing),
            Value::Date(days) => {
                human_readable_json(self.human_readable, date(*days), Json::from(*days))
            }
            Value::Decimal(decimal) => Json::String(decimal_to_string(decimal, schema)),
            Value::BigDecimal(decimal) => Json::String(decimal.to_string()),
            Value::TimeMillis(millis) => human_readable_json(
                self.human_readable,
                time_millis(*millis),
                Json::from(*millis),
            ),
            Value::TimeMicros(micros) => human_readable_json(
                self.human_readable,
                time_micros(*micros),
                Json::from(*micros),
            ),
            Value::TimestampMillis(millis) => human_readable_json(
                self.human_readable,
                timestamp_millis(*millis),
                Json::from(*millis),
            ),
            Value::TimestampMicros(micros) => human_readable_json(
                self.human_readable,
                timestamp_micros(*micros),
                Json::from(*micros),
            ),
            Value::TimestampNanos(nanos) => human_readable_json(
                self.human_readable,
                Some(timestamp_nanos(*nanos)),
                Json::from(*nanos),
            ),
            Value::LocalTimestampMillis(millis) => human_readable_json(
                self.human_readable,
                local_timestamp_millis(*millis),
                Json::from(*millis),
            ),
            Value::LocalTimestampMicros(micros) => human_readable_json(
                self.human_readable,
                local_timestamp_micros(*micros),
                Json::from(*micros),
            ),
            Value::LocalTimestampNanos(nanos) => human_readable_json(
                self.human_readable,
                Some(local_timestamp_nanos(*nanos)),
                Json::from(*nanos),
            ),
            Value::Duration(duration) => {
                if self.human_readable {
                    Json::String(format_duration(duration))
                } else {
                    duration_to_json(duration)
                }
            }
            Value::Uuid(uuid) => Json::String(uuid.to_string()),
        }
    }

    fn array(&self, values: &[Value], schema: Option<&'s Schema>, enclosing: Option<&str>) -> Json {
        let items = match schema {
            Some(Schema::Array(array)) => Some(&*array.items),
            _ => None,
        };
        Json::Array(
            values
                .iter()
                .map(|value| self.convert(value, items, enclosing))
                .collect(),
        )
    }

    fn map(
        &self,
        entries: &HashMap<String, Value>,
        schema: Option<&'s Schema>,
        enclosing: Option<&str>,
    ) -> Json {
        let types = match schema {
            Some(Schema::Map(map)) => Some(&*map.types),
            _ => None,
        };
        Json::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), self.convert(value, types, enclosing)))
                .collect::<Map<_, _>>(),
        )
    }

    fn record(
        &self,
        fields: &[(String, Value)],
        schema: Option<&'s Schema>,
        enclosing: Option<&str>,
    ) -> Json {
        let record = match schema {
            Some(Schema::Record(record)) => Some(record),
            _ => None,
        };
        Json::Object(
            fields
                .iter()
                .map(|(name, value)| {
                    let field_schema = record
                        .and_then(|record| {
                            record
                                .fields
                                .iter()
                                .find(|field| field.name.as_str() == name)
                        })
                        .map(|field| &field.schema);
                    (name.clone(), self.convert(value, field_schema, enclosing))
                })
                .collect::<Map<_, _>>(),
        )
    }
}

/// Resolves a schema reference against the names collected for the file.
fn resolve_schema<'s>(
    schema: &'s Schema,
    names: &NamesRef<'s>,
    enclosing: Option<&str>,
) -> &'s Schema {
    if let Schema::Ref { name } = schema {
        let full = name.fully_qualified_name(enclosing);
        if let Some(target) = names.get(full.as_ref()) {
            return target;
        }
    }
    schema
}

/// Renders a decimal value using the scale from its schema. Without a scale
/// the value is printed as its unscaled integer, which still preserves all of
/// its digits.
fn decimal_to_string(decimal: &Decimal, schema: Option<&Schema>) -> String {
    let scale = match schema {
        Some(Schema::Decimal(decimal)) => decimal.scale,
        _ => 0,
    };
    let scale = i64::try_from(scale).expect("decimal scales fit into `i64`");
    BigDecimal::new(decimal.clone().into(), scale).to_plain_string()
}

/// Returns `readable` when human-readable rendering succeeded, and the
/// underlying integer otherwise.
fn human_readable_json(human_readable: bool, readable: Option<String>, fallback: Json) -> Json {
    match (human_readable, readable) {
        (true, Some(readable)) => Json::String(readable),
        _ => fallback,
    }
}

fn number(value: f64) -> Json {
    if value.is_nan() {
        return Json::String("NaN".to_owned());
    }
    if value.is_infinite() {
        let text = if value.is_sign_negative() {
            "-Infinity"
        } else {
            "Infinity"
        };
        return Json::String(text.to_owned());
    }
    serde_json::Number::from_f64(value).map_or(Json::Null, Json::Number)
}

fn bytes_to_json(bytes: &[u8]) -> Json {
    Json::Array(bytes.iter().map(|byte| Json::from(*byte)).collect())
}

fn date(days: i32) -> Option<String> {
    let seconds = i64::from(days).checked_mul(86_400)?;
    DateTime::<Utc>::from_timestamp(seconds, 0).map(|value| value.date_naive().to_string())
}

fn time_millis(millis: i32) -> Option<String> {
    let millis = u32::try_from(millis).ok()?;
    let seconds = millis / 1_000;
    let nanos = (millis % 1_000) * 1_000_000;
    NaiveTime::from_num_seconds_from_midnight_opt(seconds, nanos)
        .map(|time| time.format("%H:%M:%S%.3f").to_string())
}

fn time_micros(micros: i64) -> Option<String> {
    let micros = u64::try_from(micros).ok()?;
    let seconds = u32::try_from(micros / 1_000_000).ok()?;
    let nanos = u32::try_from(micros % 1_000_000).ok()? * 1_000;
    NaiveTime::from_num_seconds_from_midnight_opt(seconds, nanos)
        .map(|time| time.format("%H:%M:%S%.6f").to_string())
}

fn timestamp_millis(millis: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(millis)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn timestamp_micros(micros: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_micros(micros)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Micros, true))
}

fn timestamp_nanos(nanos: i64) -> String {
    DateTime::<Utc>::from_timestamp_nanos(nanos).to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn local_timestamp_millis(millis: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(millis).map(|value| {
        value
            .naive_utc()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
            .to_string()
    })
}

fn local_timestamp_micros(micros: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_micros(micros).map(|value| {
        value
            .naive_utc()
            .format("%Y-%m-%dT%H:%M:%S%.6f")
            .to_string()
    })
}

fn local_timestamp_nanos(nanos: i64) -> String {
    DateTime::<Utc>::from_timestamp_nanos(nanos)
        .naive_utc()
        .format("%Y-%m-%dT%H:%M:%S%.9f")
        .to_string()
}

fn duration_to_json(duration: &Duration) -> Json {
    Json::Object(
        [
            (
                "months".to_owned(),
                Json::from(u32::from(duration.months())),
            ),
            ("days".to_owned(), Json::from(u32::from(duration.days()))),
            (
                "millis".to_owned(),
                Json::from(u32::from(duration.millis())),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn format_duration(duration: &Duration) -> String {
    let months = u32::from(duration.months());
    let days = u32::from(duration.days());
    let millis = u32::from(duration.millis());
    let seconds = millis / 1_000;
    let millis = millis % 1_000;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;

    let mut text = String::from("P");
    if months > 0 {
        let _ = write!(text, "{months}M");
    }
    if days > 0 {
        let _ = write!(text, "{days}D");
    }
    if hours > 0 || minutes > 0 || seconds > 0 || millis > 0 || (months == 0 && days == 0) {
        text.push('T');
        if hours > 0 {
            let _ = write!(text, "{hours}H");
        }
        if minutes > 0 {
            let _ = write!(text, "{minutes}M");
        }
        if millis > 0 {
            let _ = write!(text, "{seconds}.{millis:03}S");
        } else {
            let _ = write!(text, "{seconds}S");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use apache_avro::{Days, Millis, Months, Schema, Writer, types::Record};
    use serde_json::json;

    use super::*;

    fn json_for(value: &Value, human_readable: bool) -> Json {
        let names = NamesRef::new();
        JsonConverter {
            names: &names,
            human_readable,
        }
        .convert(value, None, None)
    }

    fn json_for_schema(schema: &Schema, value: &Value, human_readable: bool) -> Json {
        let resolved = ResolvedSchema::try_from(schema).expect("the schema resolves");
        JsonConverter {
            names: resolved.get_names(),
            human_readable,
        }
        .convert(value, Some(schema), None)
    }

    #[test]
    fn converts_primitive_values() {
        assert_eq!(json_for(&Value::Null, false), Json::Null);
        assert_eq!(json_for(&Value::Boolean(true), false), json!(true));
        assert_eq!(json_for(&Value::Int(-3), false), json!(-3));
        assert_eq!(json_for(&Value::Long(1 << 40), false), json!(1_i64 << 40));
        assert_eq!(json_for(&Value::Float(1.5), false), json!(1.5));
        assert_eq!(json_for(&Value::String("hi".into()), false), json!("hi"));
        assert_eq!(
            json_for(&Value::Bytes(vec![0, 127, 255]), false),
            json!([0, 127, 255])
        );
        assert_eq!(json_for(&Value::Fixed(2, vec![1, 2]), false), json!([1, 2]));
    }

    #[test]
    fn converts_composite_values() {
        let record = Value::Record(vec![
            ("id".into(), Value::Long(7)),
            ("tags".into(), Value::Array(vec![Value::String("a".into())])),
        ]);
        assert_eq!(json_for(&record, false), json!({"id": 7, "tags": ["a"]}));

        let map = Value::Map([("first".to_owned(), Value::Int(1))].into_iter().collect());
        assert_eq!(json_for(&map, false), json!({"first": 1}));

        assert_eq!(
            json_for(&Value::Union(1, Box::new(Value::String("x".into()))), false),
            json!("x")
        );
        assert_eq!(json_for(&Value::Enum(0, "RED".into()), false), json!("RED"));
    }

    #[test]
    fn renders_decimals_as_strings() {
        let schema = Schema::parse_str(
            r#"{"type":"bytes","logicalType":"decimal","precision":4,"scale":2}"#,
        )
        .expect("the schema parses");

        assert_eq!(
            json_for_schema(
                &schema,
                &Value::Decimal(Decimal::from(vec![0x04, 0xD2])),
                false
            ),
            json!("12.34")
        );
        assert_eq!(
            json_for_schema(
                &schema,
                &Value::Decimal(Decimal::from(vec![0xFB, 0x2E])),
                false
            ),
            json!("-12.34")
        );
        assert_eq!(
            json_for_schema(&schema, &Value::Decimal(Decimal::from(vec![0])), false),
            json!("0.00")
        );

        let coarse = Schema::parse_str(
            r#"{"type":"bytes","logicalType":"decimal","precision":4,"scale":0}"#,
        )
        .expect("the schema parses");
        assert_eq!(
            json_for_schema(
                &coarse,
                &Value::Decimal(Decimal::from(vec![0x04, 0xD2])),
                false
            ),
            json!("1234")
        );
    }

    #[test]
    fn renders_decimals_inside_composite_values() {
        let schema = Schema::parse_str(
            r#"{"type":"record","name":"Money","fields":[
                {"name":"amount","type":{"type":"bytes","logicalType":"decimal","precision":4,"scale":2}},
                {"name":"label","type":"string"}]}"#,
        )
        .expect("the schema parses");
        let value = Value::Record(vec![
            (
                "amount".into(),
                Value::Decimal(Decimal::from(vec![0x04, 0xD2])),
            ),
            ("label".into(), Value::String("price".into())),
        ]);
        assert_eq!(
            json_for_schema(&schema, &value, false),
            json!({"amount": "12.34", "label": "price"})
        );
    }

    #[test]
    fn keeps_logical_types_as_numbers_by_default() {
        assert_eq!(json_for(&Value::Date(1), false), json!(1));
        assert_eq!(json_for(&Value::TimeMillis(1_234), false), json!(1_234));
        assert_eq!(json_for(&Value::TimeMicros(1_234), false), json!(1_234));
        assert_eq!(
            json_for(&Value::TimestampMillis(1_234), false),
            json!(1_234)
        );
        assert_eq!(
            json_for(&Value::TimestampMicros(1_234), false),
            json!(1_234)
        );
        assert_eq!(json_for(&Value::TimestampNanos(1_234), false), json!(1_234));
        assert_eq!(
            json_for(&Value::LocalTimestampMillis(1_234), false),
            json!(1_234)
        );
        assert_eq!(
            json_for(&Value::LocalTimestampMicros(1_234), false),
            json!(1_234)
        );
        assert_eq!(
            json_for(&Value::LocalTimestampNanos(1_234), false),
            json!(1_234)
        );
    }

    #[test]
    fn renders_logical_types_human_readable() {
        assert_eq!(json_for(&Value::Date(1), true), json!("1970-01-02"));
        assert_eq!(
            json_for(&Value::TimeMillis(3_723_004), true),
            json!("01:02:03.004")
        );
        assert_eq!(
            json_for(&Value::TimeMicros(3_723_004_005), true),
            json!("01:02:03.004005")
        );
        assert_eq!(
            json_for(&Value::TimeMicros(86_399_999_999), true),
            json!("23:59:59.999999")
        );
        assert_eq!(
            json_for(&Value::TimestampMillis(1_600_000_000_123), true),
            json!("2020-09-13T12:26:40.123Z")
        );
        assert_eq!(
            json_for(&Value::TimestampMicros(1_600_000_000_123_456), true),
            json!("2020-09-13T12:26:40.123456Z")
        );
        assert_eq!(
            json_for(&Value::TimestampNanos(1_600_000_000_123_456_789), true),
            json!("2020-09-13T12:26:40.123456789Z")
        );
        assert_eq!(
            json_for(&Value::LocalTimestampMillis(1_600_000_000_123), true),
            json!("2020-09-13T12:26:40.123")
        );
        assert_eq!(
            json_for(&Value::LocalTimestampMicros(1_600_000_000_123_456), true),
            json!("2020-09-13T12:26:40.123456")
        );
        assert_eq!(
            json_for(&Value::LocalTimestampNanos(1_600_000_000_123_456_789), true),
            json!("2020-09-13T12:26:40.123456789")
        );
    }

    #[test]
    fn renders_durations() {
        let duration = Duration::new(Months::new(1), Days::new(2), Millis::new(3_723_004));
        assert_eq!(
            json_for(&Value::Duration(duration), false),
            json!({"months": 1, "days": 2, "millis": 3_723_004})
        );
        assert_eq!(
            json_for(&Value::Duration(duration), true),
            json!("P1M2DT1H2M3.004S")
        );
        let zero = Duration::new(Months::new(0), Days::new(0), Millis::new(0));
        assert_eq!(json_for(&Value::Duration(zero), true), json!("PT0S"));
    }

    #[test]
    fn falls_back_to_numbers_when_dates_are_out_of_range() {
        assert_eq!(json_for(&Value::Date(i32::MAX), true), json!(i32::MAX));
        assert_eq!(json_for(&Value::TimeMillis(-1), true), json!(-1));
    }

    fn sample_file() -> Vec<u8> {
        let schema = Schema::parse_str(
            r#"{"type":"record","name":"User","fields":[
                {"name":"id","type":"long"},{"name":"tag","type":"string"}]}"#,
        )
        .expect("the fixture schema parses");
        let mut writer = Writer::new(&schema, Vec::new()).expect("the writer opens");
        for (id, tag) in [(7_i64, "seven"), (8, "eight")] {
            let mut record = Record::new(&schema).expect("a record initializes");
            record.put("id", id);
            record.put("tag", tag);
            writer.append_value(record).expect("the value is appended");
        }
        writer.flush().expect("the writer flushes");
        writer.into_inner().expect("the writer finishes")
    }

    #[test]
    fn writes_one_json_record_per_line() {
        let mut out = Vec::new();
        write_records(Cursor::new(sample_file()), false, &mut out).unwrap();
        let text = String::from_utf8(out).expect("the output is UTF-8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            serde_json::from_str::<Json>(lines[0]).unwrap(),
            json!({"id": 7, "tag": "seven"})
        );
        assert_eq!(
            serde_json::from_str::<Json>(lines[1]).unwrap(),
            json!({"id": 8, "tag": "eight"})
        );
    }

    #[test]
    fn rejects_a_file_that_is_not_a_container() {
        let mut out = Vec::new();
        let error = write_records(Cursor::new(b"not avro".to_vec()), false, &mut out).unwrap_err();
        assert!(
            format!("{error:#}").contains("failed to read the Avro file header"),
            "unexpected error: {error:#}"
        );
    }
}
