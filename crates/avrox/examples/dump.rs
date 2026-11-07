//! Prints generated code for a schema file given as the first argument.

use std::{env, fs};

fn main() {
    let path = env::args().nth(1).expect("usage: dump <schema.avsc>");
    let source = fs::read_to_string(path).expect("failed to read schema");
    let schema =
        avrox::Schema::parse(&source, avrox::AvroVersion::V1_12).expect("failed to parse schema");
    for file in avrox::generate(&schema, &avrox::Settings::default()).expect("generation failed") {
        println!("=== {} ===\n{}", file.path, file.contents);
    }
}
