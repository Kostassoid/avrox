//! Black-box snapshot tests.
//!
//! Every directory under the repository-level `tests/cases` directory is a
//! test case containing:
//!
//! - `schema.avsc`: the Avro schema to generate code from,
//! - `settings.json` (optional): `language`, `avro_version` (the version the
//!   schema is parsed against), `layout` and
//!   `rust.types`/`rust.time`/`rust.derives` overrides,
//! - `expected/`: the expected generated files, or `error.txt`: the expected
//!   error message.
//!
//! Set the `AVROX_BLESS` environment variable to regenerate the expected
//! files from the generator output.
//!
//! The library version embedded in generated file headers is ignored when
//! comparing, so snapshots do not need to be regenerated for every release.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use avrox::{AvroVersion, Language, Layout, RustOptions, RustTime, RustTypes, Settings};

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("cases")
}

#[test]
fn snapshot_cases() {
    let bless = std::env::var_os("AVROX_BLESS").is_some();
    let cases = collect_cases();
    assert!(!cases.is_empty(), "no snapshot cases found");

    let mut failures = Vec::new();
    let mut passed = 0;
    for case in &cases {
        match run_case(case, bless) {
            Ok(()) => passed += 1,
            Err(failure) => failures.push(format!("{} failed: {failure}", case.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{passed} passed, {} failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

fn collect_cases() -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = fs::read_dir(cases_dir())
        .expect("failed to list snapshot cases")
        .map(|entry| entry.expect("failed to read case directory entry").path())
        .filter(|path| path.is_dir())
        .collect();
    cases.sort();
    cases
}

fn run_case(case: &Path, bless: bool) -> Result<(), String> {
    let source = fs::read_to_string(case.join("schema.avsc"))
        .map_err(|error| format!("failed to read schema.avsc: {error}"))?;
    let (avro_version, settings) = read_settings(case)?;
    let result = avrox::Schema::parse(&source, avro_version)
        .and_then(|schema| avrox::generate(&schema, &settings));

    let error_path = case.join("error.txt");
    if error_path.exists() {
        let expected = fs::read_to_string(&error_path)
            .map_err(|error| format!("failed to read error.txt: {error}"))?;
        let expected = normalize(&expected).trim_end().to_owned();
        let error = match result {
            Err(error) => error.to_string(),
            Ok(_) => return Err("case was expected to fail".to_owned()),
        };
        return if bless {
            fs::write(&error_path, format!("{error}\n")).map_err(|error| error.to_string())?;
            Ok(())
        } else if error == expected {
            Ok(())
        } else {
            Err(format!(
                "expected error:\n  {expected}\nactual error:\n  {error}"
            ))
        };
    }

    let files = result.map_err(|error| format!("generation failed unexpectedly: {error}"))?;
    let actual: BTreeMap<String, String> = files
        .into_iter()
        .map(|file| (file.path, file.contents))
        .collect();
    let expected_dir = case.join("expected");
    let expected = collect_expected(&expected_dir)?;

    if bless {
        if expected_dir.exists() {
            fs::remove_dir_all(&expected_dir).map_err(|error| error.to_string())?;
        }
        for (path, contents) in &actual {
            let target = expected_dir.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::write(&target, contents).map_err(|error| error.to_string())?;
        }
        return Ok(());
    }

    let mut failures = Vec::new();
    for (path, expected_contents) in &expected {
        match actual.get(path) {
            Some(actual_contents)
                if normalize(actual_contents).as_str() == expected_contents.as_str() => {}
            Some(actual_contents) => {
                let normalized = normalize(actual_contents);
                write_actual(case, path, &normalized);
                failures.push(format!(
                    "`{path}` does not match the expected output (actual written to \
                     target/avrox-snapshots): {}",
                    first_difference(expected_contents, &normalized)
                ));
            }
            None => failures.push(format!("`{path}` was expected but not generated")),
        }
    }
    for path in actual.keys() {
        if !expected.contains_key(path) {
            failures.push(format!("`{path}` was generated unexpectedly"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n  "))
    }
}

fn collect_expected(expected_dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut expected = BTreeMap::new();
    let Ok(entries) = fs::read_dir(expected_dir) else {
        return Ok(expected);
    };
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_file() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("unexpected non-UTF-8 file name")?
                .to_owned();
            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
            expected.insert(name, normalize(&contents));
        }
    }
    Ok(expected)
}

fn read_settings(case: &Path) -> Result<(AvroVersion, Settings), String> {
    let mut avro_version = AvroVersion::V1_12;
    let mut settings = Settings::default();
    let Ok(text) = fs::read_to_string(case.join("settings.json")) else {
        return Ok((avro_version, settings));
    };
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("invalid settings.json: {error}"))?;
    if let Some(language) = value.get("language").and_then(serde_json::Value::as_str) {
        settings.language = match language {
            "rust" => Language::Rust,
            other => return Err(format!("unsupported language `{other}`")),
        };
    }
    if let Some(version) = value
        .get("avro_version")
        .and_then(serde_json::Value::as_str)
    {
        avro_version = match version {
            "1.10" => AvroVersion::V1_10,
            "1.11" => AvroVersion::V1_11,
            "1.12" => AvroVersion::V1_12,
            other => return Err(format!("unsupported avro version `{other}`")),
        };
    }
    if let Some(layout) = value.get("layout").and_then(serde_json::Value::as_str) {
        settings.layout = match layout {
            "single" => Layout::SingleFile,
            "module" => Layout::Module,
            other => return Err(format!("unsupported layout `{other}`")),
        };
    }
    if let Some(rust) = value.get("rust") {
        if let Some(types) = rust.get("types").and_then(serde_json::Value::as_str) {
            settings.rust = RustOptions {
                types: match types {
                    "avro" => RustTypes::ApacheAvro,
                    "std" => RustTypes::Std,
                    other => return Err(format!("unsupported rust types `{other}`")),
                },
                ..settings.rust
            };
        }
        if let Some(time) = rust.get("time").and_then(serde_json::Value::as_str) {
            settings.rust = RustOptions {
                time: match time {
                    "primitives" => RustTime::Primitives,
                    "std-time" => RustTime::StdTime,
                    "chrono" => RustTime::Chrono,
                    other => return Err(format!("unsupported rust time `{other}`")),
                },
                ..settings.rust
            };
        }
        if let Some(derives) = rust.get("derives") {
            let values = match derives {
                serde_json::Value::Array(values) => values,
                serde_json::Value::String(_) => std::slice::from_ref(derives),
                _ => return Err("rust.derives must be a string or an array of strings".to_owned()),
            };
            let mut parsed = Vec::with_capacity(values.len());
            for value in values {
                let derive = value
                    .as_str()
                    .ok_or_else(|| "rust.derives entries must be strings".to_owned())?;
                parsed.push(derive.to_owned());
            }
            settings.rust = RustOptions {
                derives: parsed,
                ..settings.rust
            };
        }
    }
    Ok((avro_version, settings))
}

fn write_actual(case: &Path, path: &str, contents: &str) {
    let Ok(target) = std::env::var("CARGO_TARGET_TMPDIR") else {
        return;
    };
    let name = case
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let directory = Path::new(&target).join("avrox-snapshots").join(name);
    if fs::create_dir_all(&directory).is_ok() {
        let _ = fs::write(directory.join(path), contents);
    }
}

/// Normalizes platform line endings and the generator version embedded in the
/// file header, so snapshots do not need to be updated for every release.
fn normalize(contents: &str) -> String {
    let contents = contents.replace("\r\n", "\n");
    if !contents.contains("//! Generated by avrox ") {
        return contents;
    }
    contents
        .split('\n')
        .map(normalize_header_version)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replaces the version in a generated-file header line with a placeholder.
fn normalize_header_version(line: &str) -> String {
    const PREFIX: &str = "//! Generated by avrox ";
    const SUFFIX: &str = " from an Avro schema. DO NOT EDIT.";
    if line
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .is_some()
    {
        format!("{PREFIX}<version>{SUFFIX}")
    } else {
        line.to_owned()
    }
}

fn first_difference(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let count = expected_lines.len().max(actual_lines.len());
    for index in 0..count {
        let expected_line = expected_lines.get(index).copied().unwrap_or("<missing>");
        let actual_line = actual_lines.get(index).copied().unwrap_or("<missing>");
        if expected_line != actual_line {
            return format!(
                "first difference at line {}:\n    expected: {expected_line}\n    actual:   \
                 {actual_line}",
                index + 1
            );
        }
    }
    "outputs differ only in trailing newlines".to_owned()
}
