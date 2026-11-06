//! The `extract-schema` subcommand: prints the schema embedded in the header
//! of an Avro object container file without reading its data blocks.

use std::{
    fs::File,
    io::{self, BufReader, Read, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};

/// Extract the schema from an Avro data file and print it to stdout.
#[derive(Debug, clap::Parser)]
pub(crate) struct ExtractSchema {
    /// Avro data file (.avro) to read the schema from.
    input: PathBuf,
}

/// First bytes of an Avro object container file: ASCII `Obj` plus version 1.
const MAGIC: [u8; 4] = *b"Obj\x01";

/// Metadata key a container file header stores its schema under.
const SCHEMA_KEY: &[u8] = b"avro.schema";

/// Runs the `extract-schema` subcommand.
pub(crate) fn run(args: &ExtractSchema) -> Result<()> {
    let file = File::open(&args.input)
        .with_context(|| format!("failed to open `{}`", args.input.display()))?;
    let file_len = file
        .metadata()
        .with_context(|| format!("failed to read the metadata of `{}`", args.input.display()))?
        .len();
    let mut reader = BufReader::new(file);
    let schema = extract_schema(&mut reader, file_len).with_context(|| {
        format!(
            "failed to extract the schema from `{}`",
            args.input.display()
        )
    })?;
    let schema =
        std::str::from_utf8(&schema).context("the schema stored in the file is not valid UTF-8")?;
    io::stdout()
        .lock()
        .write_all(schema.as_bytes())
        .context("failed to write the schema to stdout")?;
    Ok(())
}

/// Reads the schema out of an object container file header, stopping as soon
/// as it has been found: the sync marker and the data blocks behind it are
/// never touched, so files of any size can be processed.
///
/// Every length read from the header is bounded by `file_len` so that a
/// corrupt file cannot make us allocate more than the file itself holds.
fn extract_schema<R: Read>(reader: &mut R, file_len: u64) -> Result<Vec<u8>> {
    let mut magic = [0u8; MAGIC.len()];
    reader
        .read_exact(&mut magic)
        .context("unexpected end of file while reading the file magic")?;
    if magic != MAGIC {
        bail!("not an Avro object container file");
    }
    loop {
        let count = read_long(reader).context("failed to read a metadata block")?;
        if count == 0 {
            bail!("the file header has no `avro.schema` metadata entry");
        }
        let entries = count.unsigned_abs();
        if count < 0 {
            // A negative count is followed by a block size that only exists
            // to allow skipping the block.
            read_long(reader).context("failed to read a metadata block size")?;
        }
        if entries > file_len {
            bail!("the file header claims more metadata entries than the file holds");
        }
        for _ in 0..entries {
            let key = read_bytes(reader, file_len).context("failed to read a metadata key")?;
            let value = read_bytes(reader, file_len).context("failed to read a metadata value")?;
            if key == SCHEMA_KEY {
                return Ok(value);
            }
        }
    }
}

/// Reads an Avro `long`: a variable-length zig-zag encoded integer.
fn read_long<R: Read>(reader: &mut R) -> Result<i64> {
    let mut value: u64 = 0;
    let mut shift = 0;
    loop {
        let mut byte = [0u8; 1];
        reader
            .read_exact(&mut byte)
            .context("unexpected end of file")?;
        value |= u64::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(unzigzag(value));
        }
        shift += 7;
        if shift >= 64 {
            bail!("the file header contains an oversized integer");
        }
    }
}

/// Reads an Avro `bytes`: a `long` length followed by that many bytes.
fn read_bytes<R: Read>(reader: &mut R, file_len: u64) -> Result<Vec<u8>> {
    let len = read_long(reader)?;
    let len = u64::try_from(len).context("the file header contains a negative length")?;
    if len > file_len {
        bail!("the file header contains a length larger than the file itself");
    }
    let len = usize::try_from(len).context("a header entry does not fit into memory")?;
    let mut bytes = vec![0u8; len];
    reader
        .read_exact(&mut bytes)
        .context("unexpected end of file")?;
    Ok(bytes)
}

/// Decodes a variable-length zig-zag encoded integer. The result always fits
/// into `i64`, because halving first drops the sign bit.
fn unzigzag(value: u64) -> i64 {
    let magnitude = i64::try_from(value >> 1).expect("halved values fit into `i64`");
    if value & 1 == 0 {
        magnitude
    } else {
        !magnitude
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use apache_avro::{Reader, Schema, Writer, types::Record};

    use super::*;

    /// Encodes a `long` the way the spec describes it.
    fn encode_long(value: i64) -> Vec<u8> {
        let zigzag = if value < 0 {
            (value.unsigned_abs() << 1) - 1
        } else {
            value.unsigned_abs() << 1
        };
        let mut bytes = Vec::new();
        let mut remaining = zigzag;
        loop {
            let byte = u8::try_from(remaining & 0x7f).expect("masked to seven bits");
            remaining >>= 7;
            if remaining == 0 {
                bytes.push(byte);
                return bytes;
            }
            bytes.push(byte | 0x80);
        }
    }

    /// Encodes `bytes` (or a string) the way the spec describes it.
    fn encode_bytes(data: &[u8]) -> Vec<u8> {
        let len = i64::try_from(data.len()).expect("fixture entries are small");
        let mut bytes = encode_long(len);
        bytes.extend_from_slice(data);
        bytes
    }

    /// Builds an object container file: magic, metadata map, sync marker and
    /// `trailing` bytes standing in for the data blocks.
    fn container(metadata: &[(&[u8], &[u8])], trailing: usize) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        let entries = i64::try_from(metadata.len()).expect("fixture entries are small");
        bytes.extend_from_slice(&encode_long(entries));
        for (key, value) in metadata {
            bytes.extend_from_slice(&encode_bytes(key));
            bytes.extend_from_slice(&encode_bytes(value));
        }
        bytes.push(0);
        bytes.extend(std::iter::repeat_n(0xAB, 16));
        bytes.extend(std::iter::repeat_n(0xFF, trailing));
        bytes
    }

    fn extract(bytes: &[u8]) -> Result<Vec<u8>> {
        let file_len = bytes.len() as u64;
        extract_schema(&mut Cursor::new(bytes), file_len)
    }

    #[test]
    fn extracts_the_schema_from_the_header() {
        let schema = br#"{"type":"record","name":"User","fields":[]}"#;
        let file = container(&[(SCHEMA_KEY, schema), (b"avro.codec", b"null")], 64);
        assert_eq!(extract(&file).unwrap(), schema);
    }

    #[test]
    fn stops_reading_right_after_the_schema() {
        let schema = br#"["null","string"]"#;
        let file = container(
            &[("custom.meta".as_bytes(), b"junk"), (SCHEMA_KEY, schema)],
            64,
        );
        let file_len = file.len() as u64;
        let mut reader = Cursor::new(&file);
        let extracted = extract_schema(&mut reader, file_len).unwrap();
        assert_eq!(extracted, schema);
        let bytes_read = usize::try_from(reader.position()).unwrap();
        assert!(
            bytes_read < file.len() / 2,
            "read {bytes_read} of {} bytes",
            file.len()
        );
    }

    #[test]
    fn reads_metadata_blocks_with_a_negative_count() {
        let schema = br#"{"type":"enum","name":"Card","symbols":[]}"#;
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&encode_long(-1));
        bytes.extend_from_slice(&encode_long(80));
        bytes.extend_from_slice(&encode_bytes(SCHEMA_KEY));
        bytes.extend_from_slice(&encode_bytes(schema));
        bytes.push(0);
        assert_eq!(extract(&bytes).unwrap(), schema);
    }

    #[test]
    fn rejects_a_file_that_is_not_a_container() {
        let error = extract(b"\x00\x01\x02\x03not avro at all").unwrap_err();
        assert_eq!(error.to_string(), "not an Avro object container file");
    }

    #[test]
    fn rejects_a_header_without_a_schema_entry() {
        let file = container(&[(b"avro.codec", b"null")], 64);
        let error = extract(&file).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the file header has no `avro.schema` metadata entry"
        );
    }

    #[test]
    fn rejects_a_truncated_header() {
        let schema = br#"{"type":"long"}"#;
        let file = container(&[(SCHEMA_KEY, schema)], 64);
        // Cut in the middle of the schema value itself: magic, block count,
        // key length, key and value length precede it.
        let cut = MAGIC.len() + 3 + SCHEMA_KEY.len() + schema.len() / 2;
        let error = extract(&file[..cut]).unwrap_err();
        assert!(
            format!("{error:#}").contains("unexpected end of file"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn rejects_header_lengths_beyond_the_file_size() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&encode_long(1));
        bytes.extend_from_slice(&encode_bytes(SCHEMA_KEY));
        bytes.extend_from_slice(&encode_long(1 << 40));
        let error = extract(&bytes).unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            "failed to read a metadata value: \
             the file header contains a length larger than the file itself"
        );
    }

    #[test]
    fn decodes_zig_zag_integers() {
        for value in [0, 1, -1, 2, -2, 64, -64, 127, -127] {
            let encoded = encode_long(value);
            let mut reader = Cursor::new(encoded);
            assert_eq!(read_long(&mut reader).unwrap(), value);
        }
    }

    #[test]
    fn extracts_from_a_file_written_by_apache_avro() {
        let schema = Schema::parse_str(
            r#"{"type":"record","name":"Interop","fields":[
                {"name":"id","type":"long"},{"name":"tag","type":"string"}]}"#,
        )
        .expect("the fixture schema parses");
        let mut writer = Writer::new(&schema, Vec::new()).expect("the writer opens");
        let mut record = Record::new(&schema).expect("a record initializes");
        record.put("id", 7_i64);
        record.put("tag", "seven");
        writer.append_value(record).expect("the value is appended");
        writer.flush().expect("the writer flushes");
        let file = writer.into_inner().expect("the writer finishes");

        let file_len = u64::try_from(file.len()).expect("files fit into 64 bits");
        let extracted = extract_schema(&mut Cursor::new(&file), file_len).unwrap();
        let extracted_text = std::str::from_utf8(&extracted).expect("the schema is UTF-8");
        assert_eq!(
            Schema::parse_str(extracted_text).expect("the extracted schema parses"),
            schema
        );
        assert!(
            file.windows(extracted.len())
                .any(|window| window == extracted.as_slice()),
            "the extracted schema is not a verbatim copy of the stored bytes"
        );
    }

    #[test]
    fn apache_avro_reads_our_container_files() {
        let source = r#"{"type":"record","name":"User","fields":[{"name":"id","type":"long"}]}"#;
        let file = container(
            &[(SCHEMA_KEY, source.as_bytes()), (b"avro.codec", b"null")],
            64,
        );
        let reader =
            Reader::new(Cursor::new(&file)).expect("the official reader accepts our header");
        assert_eq!(
            reader.writer_schema(),
            &Schema::parse_str(source).expect("the fixture schema parses")
        );
    }

    #[test]
    fn preserves_stored_formatting_verbatim() {
        let schema = "{\n  \"type\": \"record\",\n  \"name\": \"Pretty\",\n  \"fields\": []\n}\n";
        let file = container(&[(SCHEMA_KEY, schema.as_bytes())], 32);
        assert_eq!(extract(&file).unwrap(), schema.as_bytes());
    }
}
