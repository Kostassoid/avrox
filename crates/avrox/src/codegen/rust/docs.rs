//! Documentation attributes for generated Rust items.

use proc_macro2::TokenStream;
use quote::quote;

use crate::schema::{Fixed, LogicalType, SchemaNode};

/// Builds doc comment attributes from an Avro `doc` and extra lines.
pub(super) fn attributes(doc: Option<&str>, extra: Vec<String>) -> Vec<TokenStream> {
    let mut lines = Vec::new();
    if let Some(doc) = doc {
        lines.extend(doc.lines().map(str::to_owned));
    }
    lines.extend(extra);
    lines
        .iter()
        .map(|line| {
            let content = if line.is_empty() {
                String::new()
            } else {
                format!(" {line}")
            };
            quote! { #[doc = #content] }
        })
        .collect()
}

/// Extra doc lines describing the logical type of a field schema.
pub(super) fn logical_lines(node: &SchemaNode) -> Vec<String> {
    match node {
        SchemaNode::Logical { kind, .. } => vec![logical_line(*kind)],
        _ => Vec::new(),
    }
}

pub(super) fn logical_line(kind: LogicalType) -> String {
    let description = match kind {
        LogicalType::Decimal { precision, scale } => {
            format!("arbitrary-precision signed decimal (precision {precision}, scale {scale})")
        }
        LogicalType::Uuid => "universally unique identifier".to_owned(),
        LogicalType::Date => "days from the unix epoch".to_owned(),
        LogicalType::TimeMillis => "milliseconds after midnight".to_owned(),
        LogicalType::TimeMicros => "microseconds after midnight".to_owned(),
        LogicalType::TimestampMillis => "milliseconds from the unix epoch".to_owned(),
        LogicalType::TimestampMicros => "microseconds from the unix epoch".to_owned(),
        LogicalType::TimestampNanos => "nanoseconds from the unix epoch".to_owned(),
        LogicalType::LocalTimestampMillis => {
            "milliseconds from the unix epoch, local time".to_owned()
        }
        LogicalType::LocalTimestampMicros => {
            "microseconds from the unix epoch, local time".to_owned()
        }
        LogicalType::LocalTimestampNanos => {
            "nanoseconds from the unix epoch, local time".to_owned()
        }
        LogicalType::Duration => "months, days and milliseconds".to_owned(),
        LogicalType::BigDecimal => "arbitrary-precision decimal with variable scale".to_owned(),
    };
    format!("Logical type `{kind}`: {description}.")
}

/// Extra doc lines describing the logical type of a `fixed`.
pub(super) fn fixed_logical_lines(fixed: &Fixed) -> Vec<String> {
    match fixed.logical {
        Some(LogicalType::Decimal { precision, scale }) => {
            vec![format!(
                "Logical type `decimal` (precision {precision}, scale {scale})."
            )]
        }
        Some(LogicalType::Duration) => {
            vec!["Logical type `duration` (months, days and milliseconds).".to_owned()]
        }
        Some(LogicalType::Uuid) => {
            vec!["Logical type `uuid` (RFC 4122 byte layout).".to_owned()]
        }
        _ => Vec::new(),
    }
}
