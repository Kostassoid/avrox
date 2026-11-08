//! Compile-time validation of the generated Go snapshots.
//!
//! These tests require `go` and `gofmt` on `PATH`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use avrox::{Language, Schema, Settings};

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("cases")
}

#[test]
fn go_snapshots_are_formatted_and_compile() {
    let mut expected = fs::read_dir(cases_dir())
        .expect("failed to list snapshot cases")
        .map(|entry| entry.expect("failed to read case directory entry").path())
        .map(|path| path.join("expected_go"))
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    expected.sort();
    assert!(!expected.is_empty(), "no Go snapshot cases found");

    for directory in expected {
        let files = fs::read_dir(&directory)
            .expect("failed to list Go snapshot")
            .map(|entry| entry.expect("failed to read Go snapshot entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "go"))
            .collect::<Vec<_>>();
        assert!(
            !files.is_empty(),
            "empty Go snapshot: {}",
            directory.display()
        );

        let formatting = Command::new("gofmt")
            .arg("-d")
            .args(&files)
            .output()
            .unwrap_or_else(|error| panic!("failed to run gofmt (is Go installed?): {error}"));
        assert!(
            formatting.status.success() && formatting.stdout.is_empty(),
            "Go snapshot {} is not gofmt-clean:\n{}{}",
            directory.display(),
            String::from_utf8_lossy(&formatting.stdout),
            String::from_utf8_lossy(&formatting.stderr)
        );

        let compilation = Command::new("go")
            .arg("test")
            .current_dir(&directory)
            .env("GO111MODULE", "off")
            .output()
            .unwrap_or_else(|error| panic!("failed to run go test (is Go installed?): {error}"));
        assert!(
            compilation.status.success(),
            "Go snapshot {} failed to compile:\n{}{}",
            directory.display(),
            String::from_utf8_lossy(&compilation.stdout),
            String::from_utf8_lossy(&compilation.stderr)
        );
    }
}

#[test]
fn generated_go_runtime_handles_union_and_length_edges() {
    let schema: Schema = r#"{
        "type": "record",
        "name": "Edge",
        "fields": [
            {"name": "single", "type": ["double"]},
            {"name": "logicalA", "type": [
                {"type": "long", "logicalType": "timestamp-millis"},
                "string"
            ]},
            {"name": "logicalB", "type": ["long", "string"]},
            {"name": "choice", "type": ["null", "int", "string"], "default": 1},
            {"name": "overflow", "type": "float", "default": 1e39}
        ]
    }"#
    .parse()
    .unwrap();
    let settings = Settings::new().language(Language::Go).go_package("edge");
    let files = avrox::generate(&schema, &settings).unwrap();
    assert_eq!(files.len(), 1);

    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("go-edge-runtime");
    if directory.exists() {
        fs::remove_dir_all(&directory).unwrap();
    }
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("go.mod"), "module edge\n\ngo 1.22\n").unwrap();
    fs::write(directory.join("edge.go"), &files[0].contents).unwrap();
    fs::write(
        directory.join("edge_test.go"),
        r#"package edge

import (
	"bytes"
	"io"
	"math"
	"testing"
	"time"
)

func TestGeneratedRuntime(t *testing.T) {
	value := NewEdge()
	value.Single = 1.5
	value.LogicalA = NewTimestampMillisOrStringFromString("logical")
	value.LogicalB = NewLongOrStringFromLong(7)
	if value.Choice.Int == nil || *value.Choice.Int != 1 || value.Choice.Null != nil {
		t.Fatalf("non-first union default selected the wrong branch: %#v", value.Choice)
	}
	encoded, err := value.MarshalAvro()
	if err != nil {
		t.Fatal(err)
	}
	expectedPrefix := []byte{0, 0, 0, 0, 0, 0, 0, 0xf8, 0x3f}
	if len(encoded) < len(expectedPrefix) || !bytes.Equal(encoded[:len(expectedPrefix)], expectedPrefix) {
		t.Fatalf("single union is missing its zero index: %x", encoded)
	}
	var decoded Edge
	if err := decoded.UnmarshalAvro(encoded); err != nil {
		t.Fatal(err)
	}
	if decoded.Single != 1.5 || decoded.Choice.Int == nil || *decoded.Choice.Int != 1 {
		t.Fatalf("unexpected round trip: %#v", decoded)
	}
	if !math.IsInf(float64(value.Overflow), 1) {
		t.Fatalf("float default did not narrow to positive infinity: %v", value.Overflow)
	}
	if err := avroWriteTimestampNanos(io.Discard, time.Time{}); err == nil {
		t.Fatal("zero time escaped the Avro nanosecond range check")
	}
	if err := avroWriteTimestampMicros(io.Discard, time.Unix(math.MaxInt64/1_000_000+1, 0)); err == nil {
		t.Fatal("out-of-range microsecond timestamp was accepted")
	}
	var malformed bytes.Buffer
	if err := avroWriteLong(&malformed, math.MaxInt64); err != nil {
		t.Fatal(err)
	}
	if _, err := avroReadBytes(&malformed); err == nil {
		t.Fatal("truncated byte string was accepted")
	}
}
"#,
    )
    .unwrap();
    let output = Command::new("go")
        .arg("test")
        .current_dir(&directory)
        .output()
        .unwrap_or_else(|error| panic!("failed to run go test (is Go installed?): {error}"));
    assert!(
        output.status.success(),
        "generated Go runtime test failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
