//! The `write` subcommand: reads JSON records and writes them to an Avro data
//! file according to a schema.

use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader},
    path::PathBuf,
};

use anyhow::{Context, Result, anyhow, bail};
use apache_avro::{
    BigDecimal, Days, Decimal, Duration, Millis, Months, Schema, Uuid, Writer,
    schema::{RecordSchema, ResolvedSchema, UnionSchema},
    types::Value,
};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use serde_json::{Map, Value as Json};

/// Write JSON records to an Avro data file.
#[derive(Debug, clap::Parser)]
pub(crate) struct Write {
    /// Avro data file (.avro) to create.
    output: PathBuf,

    /// Avro schema file (.avsc) describing the records.
    schema: PathBuf,

    /// JSONL file with the records to write. Reads standard input when omitted.
    input: Option<PathBuf>,

    /// Parse dates, times, timestamps and durations from human-readable
    /// strings instead of their underlying representations.
    #[arg(long)]
    human_readable: bool,
}

/// Runs the `write` subcommand.
pub(crate) fn run(args: &Write) -> Result<()> {
    let schema_text = fs::read_to_string(&args.schema)
        .with_context(|| format!("failed to read `{}`", args.schema.display()))?;
    let schema = Schema::parse_str(&schema_text)
        .with_context(|| format!("failed to parse `{}`", args.schema.display()))?;
    let input: Box<dyn BufRead> = match &args.input {
        Some(path) => {
            Box::new(BufReader::new(File::open(path).with_context(|| {
                format!("failed to open `{}`", path.display())
            })?))
        }
        None => Box::new(BufReader::new(io::stdin())),
    };
    let output = File::create(&args.output)
        .with_context(|| format!("failed to create `{}`", args.output.display()))?;
    write_records(&schema, input, output, args.human_readable)
        .with_context(|| format!("failed to write `{}`", args.output.display()))?;
    Ok(())
}

/// Reads JSON lines from `input` and writes the corresponding Avro records to
/// `output`.
fn write_records<R: BufRead, W: io::Write>(
    schema: &Schema,
    input: R,
    output: W,
    human_readable: bool,
) -> Result<()> {
    let resolved =
        ResolvedSchema::try_from(schema).context("failed to resolve schema references")?;
    let converter = Converter {
        resolved: &resolved,
        human_readable,
    };
    let mut writer = Writer::new(schema, output).context("failed to create the Avro writer")?;
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let line = line.with_context(|| format!("failed to read line {line_number}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let json: Json = serde_json::from_str(&line)
            .with_context(|| format!("line {line_number}: invalid JSON"))?;
        let value = converter
            .convert(&json, schema, None, "$")
            .with_context(|| format!("line {line_number}: record does not match the schema"))?;
        writer
            .append_value_ref(&value)
            .with_context(|| format!("line {line_number}: failed to write the record"))?;
    }
    writer
        .into_inner()
        .context("failed to finish the Avro file")?;
    Ok(())
}

struct Converter<'a, 's> {
    resolved: &'a ResolvedSchema<'s>,
    human_readable: bool,
}

impl Converter<'_, '_> {
    fn convert(
        &self,
        json: &Json,
        schema: &Schema,
        enclosing: Option<&str>,
        path: &str,
    ) -> Result<Value> {
        self.convert_at(json, schema, enclosing, path, self.human_readable)
    }

    fn convert_at(
        &self,
        json: &Json,
        schema: &Schema,
        enclosing: Option<&str>,
        path: &str,
        human_readable: bool,
    ) -> Result<Value> {
        match schema {
            Schema::Ref { name } => {
                let full = name.fully_qualified_name(enclosing);
                let target = self
                    .resolved
                    .get_names()
                    .get(full.as_ref())
                    .copied()
                    .ok_or_else(|| {
                        anyhow!(
                            "{path}: unresolved schema reference `{}`",
                            full.fullname(None)
                        )
                    })?;
                self.convert_at(json, target, full.namespace(), path, human_readable)
            }
            Schema::Null => expect_null(json, path).map(|()| Value::Null),
            Schema::Boolean => json
                .as_bool()
                .map(Value::Boolean)
                .ok_or_else(|| type_error(path, "a boolean", json)),
            Schema::Int => Ok(Value::Int(integer_value(json, path)?)),
            Schema::Long => Ok(Value::Long(long_value(json, path)?)),
            Schema::Float => Ok(Value::Float(float_value(json, path)?)),
            Schema::Double => Ok(Value::Double(double_value(json, path)?)),
            Schema::Bytes => Ok(Value::Bytes(bytes_from_json(json, path)?)),
            Schema::String => json
                .as_str()
                .map(|text| Value::String(text.to_owned()))
                .ok_or_else(|| type_error(path, "a string", json)),
            Schema::Array(array) => self.array(json, &array.items, enclosing, path, human_readable),
            Schema::Map(map) => self.map(json, &map.types, enclosing, path, human_readable),
            Schema::Union(union) => self.union(json, union, enclosing, path, human_readable),
            Schema::Record(record) => self.record(json, record, enclosing, path, human_readable),
            Schema::Enum(enum_schema) => enum_value(json, &enum_schema.symbols, path),
            Schema::Fixed(fixed) => fixed_value(json, fixed.size, path),
            Schema::Decimal(_) => Ok(Value::Decimal(Decimal::from(bytes_from_json(json, path)?))),
            Schema::BigDecimal => big_decimal_value(json, path),
            Schema::Uuid(_) => uuid_value(json, path),
            Schema::Date => date_value(json, path, human_readable),
            Schema::TimeMillis => time_millis_value(json, path, human_readable),
            Schema::TimeMicros => time_micros_value(json, path, human_readable),
            Schema::TimestampMillis => timestamp_millis_value(json, path, human_readable),
            Schema::TimestampMicros => timestamp_micros_value(json, path, human_readable),
            Schema::TimestampNanos => timestamp_nanos_value(json, path, human_readable),
            Schema::LocalTimestampMillis => {
                local_timestamp_millis_value(json, path, human_readable)
            }
            Schema::LocalTimestampMicros => {
                local_timestamp_micros_value(json, path, human_readable)
            }
            Schema::LocalTimestampNanos => local_timestamp_nanos_value(json, path, human_readable),
            Schema::Duration(_) => duration_value(json, path, human_readable),
        }
    }

    fn record(
        &self,
        json: &Json,
        record: &RecordSchema,
        enclosing: Option<&str>,
        path: &str,
        human_readable: bool,
    ) -> Result<Value> {
        let object = json
            .as_object()
            .ok_or_else(|| type_error(path, "an object", json))?;
        for key in object.keys() {
            if !record.lookup.contains_key(key) {
                bail!("{path}.{key}: unknown field");
            }
        }
        let namespace = record.name.namespace().or(enclosing);
        let mut fields = Vec::with_capacity(record.fields.len());
        for field in &record.fields {
            let field_path = format!("{path}.{}", field.name);
            let value = if let Some(json) = object.get(&field.name) {
                self.convert_at(json, &field.schema, namespace, &field_path, human_readable)?
            } else if let Some(default) = &field.default {
                self.convert_at(default, &field.schema, namespace, &field_path, false)
                    .with_context(|| format!("{field_path}: invalid schema default"))?
            } else if let Some(index) = null_branch(&field.schema) {
                Value::Union(index, Box::new(Value::Null))
            } else {
                bail!("{field_path}: missing required field");
            };
            fields.push((field.name.clone(), value));
        }
        Ok(Value::Record(fields))
    }

    fn array(
        &self,
        json: &Json,
        schema: &Schema,
        enclosing: Option<&str>,
        path: &str,
        human_readable: bool,
    ) -> Result<Value> {
        let items = json
            .as_array()
            .ok_or_else(|| type_error(path, "an array", json))?;
        let values = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                self.convert_at(
                    item,
                    schema,
                    enclosing,
                    &format!("{path}[{index}]"),
                    human_readable,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Value::Array(values))
    }

    fn map(
        &self,
        json: &Json,
        schema: &Schema,
        enclosing: Option<&str>,
        path: &str,
        human_readable: bool,
    ) -> Result<Value> {
        let entries = json
            .as_object()
            .ok_or_else(|| type_error(path, "an object", json))?;
        let values = entries
            .iter()
            .map(|(key, value)| {
                self.convert_at(
                    value,
                    schema,
                    enclosing,
                    &format!("{path}[{key:?}]"),
                    human_readable,
                )
                .map(|value| (key.clone(), value))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Value::Map(values.into_iter().collect()))
    }

    fn union(
        &self,
        json: &Json,
        union: &UnionSchema,
        enclosing: Option<&str>,
        path: &str,
        human_readable: bool,
    ) -> Result<Value> {
        let mut errors = Vec::new();
        for (index, variant) in union.variants().iter().enumerate() {
            match self.convert_at(json, variant, enclosing, path, human_readable) {
                Ok(value) => {
                    let index = u32::try_from(index).expect("unions fit into `u32`");
                    return Ok(Value::Union(index, Box::new(value)));
                }
                Err(error) => errors.push(format!("- {error}")),
            }
        }
        bail!(
            "{path}: value does not match any union variant:\n{}",
            errors.join("\n")
        )
    }
}

fn null_branch(schema: &Schema) -> Option<u32> {
    if let Schema::Union(union) = schema {
        let index = union
            .variants()
            .iter()
            .position(|variant| matches!(variant, Schema::Null))?;
        u32::try_from(index).ok()
    } else {
        None
    }
}

fn json_type(json: &Json) -> &'static str {
    match json {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "an array",
        Json::Object(_) => "an object",
    }
}

fn type_error(path: &str, expected: &str, json: &Json) -> anyhow::Error {
    anyhow!("{path}: expected {expected}, found {}", json_type(json))
}

fn expect_null(json: &Json, path: &str) -> Result<()> {
    if json.is_null() {
        Ok(())
    } else {
        Err(type_error(path, "null", json))
    }
}

fn json_integer(json: &Json) -> Option<i64> {
    match json {
        Json::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok())),
        _ => None,
    }
}

fn integer_value(json: &Json, path: &str) -> Result<i32> {
    let value = json_integer(json).ok_or_else(|| type_error(path, "an integer", json))?;
    i32::try_from(value).map_err(|_| anyhow!("{path}: {value} does not fit into a 32-bit integer"))
}

fn long_value(json: &Json, path: &str) -> Result<i64> {
    json_integer(json).ok_or_else(|| type_error(path, "an integer", json))
}

#[allow(clippy::cast_possible_truncation)]
fn float_value(json: &Json, path: &str) -> Result<f32> {
    match json {
        Json::Number(number) => number
            .as_f64()
            .map(|value| value as f32)
            .ok_or_else(|| anyhow!("{path}: number is not representable")),
        Json::String(text) => special_float(text)
            .map(|value| value as f32)
            .ok_or_else(|| anyhow!("{path}: `{text}` is not a floating-point value")),
        _ => Err(type_error(path, "a number", json)),
    }
}

fn double_value(json: &Json, path: &str) -> Result<f64> {
    match json {
        Json::Number(number) => number
            .as_f64()
            .ok_or_else(|| anyhow!("{path}: number is not representable")),
        Json::String(text) => special_float(text)
            .ok_or_else(|| anyhow!("{path}: `{text}` is not a floating-point value")),
        _ => Err(type_error(path, "a number", json)),
    }
}

fn special_float(text: &str) -> Option<f64> {
    match text {
        "NaN" => Some(f64::NAN),
        "INF" | "Infinity" | "inf" => Some(f64::INFINITY),
        "-INF" | "-Infinity" | "-inf" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

fn bytes_from_json(json: &Json, path: &str) -> Result<Vec<u8>> {
    match json {
        Json::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let item_path = format!("{path}[{index}]");
                let value =
                    json_integer(item).ok_or_else(|| type_error(&item_path, "a byte", item))?;
                u8::try_from(value)
                    .map_err(|_| anyhow!("{item_path}: {value} is outside the byte range 0..=255"))
            })
            .collect(),
        Json::String(text) => text
            .chars()
            .enumerate()
            .map(|(index, character)| {
                u8::try_from(u32::from(character))
                    .map_err(|_| anyhow!("{path}: character at position {index} is not a byte"))
            })
            .collect(),
        _ => Err(type_error(path, "an array of bytes or a byte string", json)),
    }
}

fn fixed_value(json: &Json, size: usize, path: &str) -> Result<Value> {
    let bytes = bytes_from_json(json, path)?;
    if bytes.len() != size {
        bail!("{path}: expected {size} bytes, found {}", bytes.len());
    }
    Ok(Value::Fixed(size, bytes))
}

fn enum_value(json: &Json, symbols: &[String], path: &str) -> Result<Value> {
    let symbol = json
        .as_str()
        .ok_or_else(|| type_error(path, "an enum symbol string", json))?;
    let index = symbols
        .iter()
        .position(|candidate| candidate == symbol)
        .ok_or_else(|| {
            anyhow!(
                "{path}: `{symbol}` is not a valid symbol (expected one of: {})",
                symbols.join(", ")
            )
        })?;
    let index = u32::try_from(index).expect("enums fit into `u32`");
    Ok(Value::Enum(index, symbol.to_owned()))
}

fn big_decimal_value(json: &Json, path: &str) -> Result<Value> {
    let text = match json {
        Json::String(text) => text.clone(),
        Json::Number(number) => number.to_string(),
        _ => return Err(type_error(path, "a decimal number", json)),
    };
    let decimal = text
        .parse::<BigDecimal>()
        .map_err(|error| anyhow!("{path}: invalid decimal `{text}`: {error}"))?;
    Ok(Value::BigDecimal(decimal))
}

fn uuid_value(json: &Json, path: &str) -> Result<Value> {
    let text = json
        .as_str()
        .ok_or_else(|| type_error(path, "a UUID string", json))?;
    let uuid =
        Uuid::parse_str(text).map_err(|error| anyhow!("{path}: invalid UUID `{text}`: {error}"))?;
    Ok(Value::Uuid(uuid))
}

fn date_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        let text = json
            .as_str()
            .ok_or_else(|| type_error(path, "an ISO 8601 date string", json))?;
        let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .map_err(|error| anyhow!("{path}: invalid date `{text}`: {error}"))?;
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("the epoch is a valid date");
        let days = date.signed_duration_since(epoch).num_days();
        let days = i32::try_from(days).map_err(|_| anyhow!("{path}: date is out of range"))?;
        Ok(Value::Date(days))
    } else {
        Ok(Value::Date(integer_value(json, path)?))
    }
}

fn time_millis_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        let time = parse_time(json, path)?;
        let millis = i64::from(time.num_seconds_from_midnight()) * 1_000
            + i64::from(time.nanosecond() / 1_000_000);
        let millis = i32::try_from(millis)
            .map_err(|_| anyhow!("{path}: time is out of the millisecond range"))?;
        Ok(Value::TimeMillis(millis))
    } else {
        Ok(Value::TimeMillis(integer_value(json, path)?))
    }
}

fn time_micros_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        let time = parse_time(json, path)?;
        let micros = i64::from(time.num_seconds_from_midnight()) * 1_000_000
            + i64::from(time.nanosecond() / 1_000);
        Ok(Value::TimeMicros(micros))
    } else {
        Ok(Value::TimeMicros(long_value(json, path)?))
    }
}

fn timestamp_millis_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        Ok(Value::TimestampMillis(
            parse_timestamp(json, path)?.timestamp_millis(),
        ))
    } else {
        Ok(Value::TimestampMillis(long_value(json, path)?))
    }
}

fn timestamp_micros_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        Ok(Value::TimestampMicros(
            parse_timestamp(json, path)?.timestamp_micros(),
        ))
    } else {
        Ok(Value::TimestampMicros(long_value(json, path)?))
    }
}

fn timestamp_nanos_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        let nanos = parse_timestamp(json, path)?
            .timestamp_nanos_opt()
            .ok_or_else(|| anyhow!("{path}: timestamp is out of the nanosecond range"))?;
        Ok(Value::TimestampNanos(nanos))
    } else {
        Ok(Value::TimestampNanos(long_value(json, path)?))
    }
}

fn local_timestamp_millis_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        Ok(Value::LocalTimestampMillis(
            parse_local_timestamp(json, path)?
                .and_utc()
                .timestamp_millis(),
        ))
    } else {
        Ok(Value::LocalTimestampMillis(long_value(json, path)?))
    }
}

fn local_timestamp_micros_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        Ok(Value::LocalTimestampMicros(
            parse_local_timestamp(json, path)?
                .and_utc()
                .timestamp_micros(),
        ))
    } else {
        Ok(Value::LocalTimestampMicros(long_value(json, path)?))
    }
}

fn local_timestamp_nanos_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    if human_readable {
        let nanos = parse_local_timestamp(json, path)?
            .and_utc()
            .timestamp_nanos_opt()
            .ok_or_else(|| anyhow!("{path}: timestamp is out of the nanosecond range"))?;
        Ok(Value::LocalTimestampNanos(nanos))
    } else {
        Ok(Value::LocalTimestampNanos(long_value(json, path)?))
    }
}

fn parse_time(json: &Json, path: &str) -> Result<NaiveTime> {
    let text = json
        .as_str()
        .ok_or_else(|| type_error(path, "an ISO 8601 time string", json))?;
    NaiveTime::parse_from_str(text, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(text, "%H:%M:%S"))
        .map_err(|error| anyhow!("{path}: invalid time `{text}`: {error}"))
}

fn parse_timestamp(json: &Json, path: &str) -> Result<DateTime<Utc>> {
    let text = json
        .as_str()
        .ok_or_else(|| type_error(path, "an RFC 3339 timestamp string", json))?;
    DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| anyhow!("{path}: invalid timestamp `{text}`: {error}"))
}

fn parse_local_timestamp(json: &Json, path: &str) -> Result<NaiveDateTime> {
    let text = json
        .as_str()
        .ok_or_else(|| type_error(path, "an ISO 8601 timestamp string", json))?;
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|error| anyhow!("{path}: invalid timestamp `{text}`: {error}"))
}

fn duration_value(json: &Json, path: &str, human_readable: bool) -> Result<Value> {
    let duration = if human_readable {
        let text = json
            .as_str()
            .ok_or_else(|| type_error(path, "an ISO 8601 duration string", json))?;
        parse_iso_duration(text)
            .map_err(|error| anyhow!("{path}: invalid duration `{text}`: {error}"))?
    } else {
        match json {
            Json::Object(_) => duration_from_object(json, path)?,
            Json::Array(_) => {
                let bytes = bytes_from_json(json, path)?;
                let bytes: [u8; 12] = bytes.as_slice().try_into().map_err(|_| {
                    anyhow!(
                        "{path}: expected 12 bytes for a duration, found {}",
                        bytes.len()
                    )
                })?;
                Duration::from(bytes)
            }
            _ => {
                return Err(type_error(
                    path,
                    "a duration object or a 12-byte array",
                    json,
                ));
            }
        }
    };
    Ok(Value::Duration(duration))
}

fn duration_from_object(json: &Json, path: &str) -> Result<Duration> {
    let object = json
        .as_object()
        .ok_or_else(|| type_error(path, "a duration object", json))?;
    let months = duration_component(object, "months", path)?;
    let days = duration_component(object, "days", path)?;
    let millis = duration_component(object, "millis", path)?;
    Ok(Duration::new(
        Months::new(months),
        Days::new(days),
        Millis::new(millis),
    ))
}

fn duration_component(object: &Map<String, Json>, key: &str, path: &str) -> Result<u32> {
    let value = object
        .get(key)
        .ok_or_else(|| anyhow!("{path}: missing duration component `{key}`"))?;
    let value = json_integer(value)
        .ok_or_else(|| type_error(&format!("{path}.{key}"), "an integer", value))?;
    u32::try_from(value)
        .map_err(|_| anyhow!("{path}.{key}: {value} is not a valid duration component"))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn parse_iso_duration(text: &str) -> Result<Duration> {
    let rest = text
        .strip_prefix('P')
        .ok_or_else(|| anyhow!("expected a duration starting with `P`"))?;
    let (date_part, time_part) = rest.split_once('T').unwrap_or((rest, ""));
    let mut months: u64 = 0;
    let mut days: u64 = 0;
    let mut millis: u64 = 0;
    let mut number = String::new();

    for character in date_part.chars() {
        if character.is_ascii_digit() {
            number.push(character);
            continue;
        }
        let value: u64 = number
            .parse()
            .map_err(|_| anyhow!("expected a number before `{character}`"))?;
        number.clear();
        match character {
            'Y' => months = add(months, multiply(value, 12)?)?,
            'M' => months = add(months, value)?,
            'W' => days = add(days, multiply(value, 7)?)?,
            'D' => days = add(days, value)?,
            _ => bail!("unexpected `{character}` in the date part"),
        }
    }
    if !number.is_empty() {
        bail!("the duration date part ends with a number");
    }

    for character in time_part.chars() {
        if character.is_ascii_digit() || character == '.' {
            number.push(character);
            continue;
        }
        let value: f64 = number
            .parse()
            .map_err(|_| anyhow!("expected a number before `{character}`"))?;
        number.clear();
        let milliseconds = match character {
            'H' => value * 3_600_000.0,
            'M' => value * 60_000.0,
            'S' => value * 1_000.0,
            _ => bail!("unexpected `{character}` in the time part"),
        };
        if !milliseconds.is_finite() || milliseconds < 0.0 {
            bail!("duration is out of range");
        }
        millis = add(millis, milliseconds.round() as u64)?;
    }
    if !number.is_empty() {
        bail!("the duration time part ends with a number");
    }

    let months = u32::try_from(months).map_err(|_| anyhow!("months are out of range"))?;
    let days = u32::try_from(days).map_err(|_| anyhow!("days are out of range"))?;
    let millis = u32::try_from(millis).map_err(|_| anyhow!("milliseconds are out of range"))?;
    Ok(Duration::new(
        Months::new(months),
        Days::new(days),
        Millis::new(millis),
    ))
}

fn multiply(value: u64, factor: u64) -> Result<u64> {
    value
        .checked_mul(factor)
        .ok_or_else(|| anyhow!("duration is too large"))
}

fn add(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| anyhow!("duration is too large"))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use apache_avro::{Reader, Writer};
    use serde_json::json;

    use super::*;

    fn convert(schema: &Schema, value: &Json, human_readable: bool) -> Result<Value> {
        let resolved = ResolvedSchema::try_from(schema).expect("the schema resolves");
        let converter = Converter {
            resolved: &resolved,
            human_readable,
        };
        converter.convert(value, schema, None, "$")
    }

    fn parse_schema(text: &str) -> Schema {
        Schema::parse_str(text).expect("the fixture schema parses")
    }

    #[test]
    fn accepts_nullable_fields_when_they_are_missing() {
        let schema = parse_schema(
            r#"{"type":"record","name":"User","fields":[
                {"name":"id","type":"long"},
                {"name":"name","type":["null","string"]}]}"#,
        );
        let value = convert(&schema, &json!({"id": 7}), false).unwrap();
        assert_eq!(
            value,
            Value::Record(vec![
                ("id".into(), Value::Long(7)),
                ("name".into(), Value::Union(0, Box::new(Value::Null))),
            ])
        );
    }

    #[test]
    fn rejects_missing_required_fields() {
        let schema = parse_schema(
            r#"{"type":"record","name":"User","fields":[
                {"name":"id","type":"long"},
                {"name":"name","type":"string"}]}"#,
        );
        let error = convert(&schema, &json!({"id": 7}), false).unwrap_err();
        assert!(
            format!("{error:#}").contains("$.name: missing required field"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn rejects_fields_with_the_wrong_type() {
        let schema = parse_schema(
            r#"{"type":"record","name":"User","fields":[
                {"name":"id","type":"long"}]}"#,
        );
        let error = convert(&schema, &json!({"id": "seven"}), false).unwrap_err();
        assert!(
            format!("{error:#}").contains("$.id: expected an integer, found a string"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        let schema = parse_schema(
            r#"{"type":"record","name":"User","fields":[
                {"name":"id","type":"long"}]}"#,
        );
        let error = convert(&schema, &json!({"id": 7, "extra": 1}), false).unwrap_err();
        assert!(
            format!("{error:#}").contains("$.extra: unknown field"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn applies_schema_defaults() {
        let schema = parse_schema(
            r#"{"type":"record","name":"User","fields":[
                {"name":"id","type":"long"},
                {"name":"count","type":"int","default":42}]}"#,
        );
        let value = convert(&schema, &json!({"id": 7}), false).unwrap();
        assert_eq!(
            value,
            Value::Record(vec![
                ("id".into(), Value::Long(7)),
                ("count".into(), Value::Int(42)),
            ])
        );
    }

    #[test]
    fn selects_union_branches() {
        let schema = parse_schema(r#"["null","string"]"#);
        assert_eq!(
            convert(&schema, &json!("hello"), false).unwrap(),
            Value::Union(1, Box::new(Value::String("hello".into())))
        );
        assert_eq!(
            convert(&schema, &Json::Null, false).unwrap(),
            Value::Union(0, Box::new(Value::Null))
        );
    }

    #[test]
    fn rejects_invalid_enum_symbols() {
        let schema = parse_schema(r#"{"type":"enum","name":"Color","symbols":["RED","GREEN"]}"#);
        let error = convert(&schema, &json!("BLUE"), false).unwrap_err();
        assert!(
            format!("{error:#}").contains("`BLUE` is not a valid symbol"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn converts_bytes_and_fixed_values() {
        let schema = parse_schema(
            r#"{"type":"record","name":"Blob","fields":[
                {"name":"data","type":"bytes"},
                {"name":"hash","type":{"type":"fixed","name":"Hash","size":4}}]}"#,
        );
        let value = convert(
            &schema,
            &json!({"data": [1, 2, 3], "hash": [4, 5, 6, 7]}),
            false,
        )
        .unwrap();
        assert_eq!(
            value,
            Value::Record(vec![
                ("data".into(), Value::Bytes(vec![1, 2, 3])),
                ("hash".into(), Value::Fixed(4, vec![4, 5, 6, 7])),
            ])
        );
    }

    #[test]
    fn converts_decimal_values() {
        let schema = parse_schema(
            r#"{"type":"record","name":"Money","fields":[
                {"name":"decimal","type":{"type":"bytes","logicalType":"decimal","precision":4,"scale":2}},
                {"name":"big","type":{"type":"bytes","logicalType":"big-decimal"}}]}"#,
        );
        let value = convert(&schema, &json!({"decimal": [1, 2], "big": "123.45"}), false).unwrap();
        assert_eq!(
            value,
            Value::Record(vec![
                ("decimal".into(), Value::Decimal(Decimal::from(vec![1, 2]))),
                (
                    "big".into(),
                    Value::BigDecimal("123.45".parse().expect("valid decimal"))
                ),
            ])
        );
    }

    #[test]
    fn converts_special_floats() {
        let schema = parse_schema(r#""double""#);
        assert!(matches!(
            convert(&schema, &json!("NaN"), false).unwrap(),
            Value::Double(value) if value.is_nan()
        ));
        assert_eq!(
            convert(&schema, &json!("Infinity"), false).unwrap(),
            Value::Double(f64::INFINITY)
        );
        assert_eq!(
            convert(&schema, &json!("-Infinity"), false).unwrap(),
            Value::Double(f64::NEG_INFINITY)
        );
    }

    fn logical_schema() -> Schema {
        parse_schema(
            r#"{"type":"record","name":"Times","fields":[
                {"name":"date","type":{"type":"int","logicalType":"date"}},
                {"name":"time_ms","type":{"type":"int","logicalType":"time-millis"}},
                {"name":"time_us","type":{"type":"long","logicalType":"time-micros"}},
                {"name":"ts_ms","type":{"type":"long","logicalType":"timestamp-millis"}},
                {"name":"ts_us","type":{"type":"long","logicalType":"timestamp-micros"}},
                {"name":"ts_ns","type":{"type":"long","logicalType":"timestamp-nanos"}},
                {"name":"local_ms","type":{"type":"long","logicalType":"local-timestamp-millis"}},
                {"name":"local_us","type":{"type":"long","logicalType":"local-timestamp-micros"}},
                {"name":"local_ns","type":{"type":"long","logicalType":"local-timestamp-nanos"}},
                {"name":"duration","type":{"type":"fixed","name":"Span","size":12,"logicalType":"duration"}}
            ]}"#,
        )
    }

    fn logical_json() -> Json {
        json!({
            "date": "1970-01-02",
            "time_ms": "01:02:03.004",
            "time_us": "01:02:03.004005",
            "ts_ms": "2020-09-13T12:26:40.123Z",
            "ts_us": "2020-09-13T12:26:40.123456Z",
            "ts_ns": "2020-09-13T12:26:40.123456789Z",
            "local_ms": "2020-09-13T12:26:40.123",
            "local_us": "2020-09-13T12:26:40.123456",
            "local_ns": "2020-09-13T12:26:40.123456789",
            "duration": "P1M2DT1H2M3.004S"
        })
    }

    #[test]
    fn converts_human_readable_logical_types() {
        let value = convert(&logical_schema(), &logical_json(), true).unwrap();
        assert_eq!(
            value,
            Value::Record(vec![
                ("date".into(), Value::Date(1)),
                ("time_ms".into(), Value::TimeMillis(3_723_004)),
                ("time_us".into(), Value::TimeMicros(3_723_004_005)),
                ("ts_ms".into(), Value::TimestampMillis(1_600_000_000_123)),
                (
                    "ts_us".into(),
                    Value::TimestampMicros(1_600_000_000_123_456)
                ),
                (
                    "ts_ns".into(),
                    Value::TimestampNanos(1_600_000_000_123_456_789)
                ),
                (
                    "local_ms".into(),
                    Value::LocalTimestampMillis(1_600_000_000_123)
                ),
                (
                    "local_us".into(),
                    Value::LocalTimestampMicros(1_600_000_000_123_456)
                ),
                (
                    "local_ns".into(),
                    Value::LocalTimestampNanos(1_600_000_000_123_456_789)
                ),
                (
                    "duration".into(),
                    Value::Duration(Duration::new(
                        Months::new(1),
                        Days::new(2),
                        Millis::new(3_723_004),
                    ))
                ),
            ])
        );
    }

    #[test]
    fn writes_and_reads_human_readable_logical_types() {
        let schema = logical_schema();
        let value = convert(&schema, &logical_json(), true).unwrap();
        let mut writer = Writer::new(&schema, Vec::new()).expect("the writer opens");
        writer.append_value_ref(&value).expect("the value is valid");
        let file = writer.into_inner().expect("the writer finishes");
        let mut reader = Reader::new(Cursor::new(file)).expect("the reader opens");
        assert_eq!(reader.next().unwrap().unwrap(), value);
        assert!(reader.next().is_none());
    }

    #[test]
    fn writes_json_lines_to_an_avro_file() {
        let schema = parse_schema(
            r#"{"type":"record","name":"User","fields":[
                {"name":"id","type":"long"},
                {"name":"name","type":["null","string"]}]}"#,
        );
        let input = Cursor::new(b"{\"id\":1,\"name\":\"one\"}\n{\"id\":2}\n".to_vec());
        let mut output = Vec::new();
        write_records(&schema, input, &mut output, false).unwrap();

        let reader = Reader::new(Cursor::new(output)).expect("the reader opens");
        let records = reader
            .map(|record| record.expect("the record reads"))
            .collect::<Vec<_>>();
        assert_eq!(
            records,
            vec![
                Value::Record(vec![
                    ("id".into(), Value::Long(1)),
                    (
                        "name".into(),
                        Value::Union(1, Box::new(Value::String("one".into())))
                    ),
                ]),
                Value::Record(vec![
                    ("id".into(), Value::Long(2)),
                    ("name".into(), Value::Union(0, Box::new(Value::Null))),
                ]),
            ]
        );
    }

    #[test]
    fn fails_on_the_first_invalid_record() {
        let schema = parse_schema(
            r#"{"type":"record","name":"User","fields":[{"name":"id","type":"long"}]}"#,
        );
        let input = Cursor::new(b"{\"id\":1}\n{\"id\":\"two\"}\n".to_vec());
        let error = write_records(&schema, input, Vec::new(), false).unwrap_err();
        assert!(
            format!("{error:#}").contains("line 2: record does not match the schema"),
            "unexpected error: {error:#}"
        );
    }
}
