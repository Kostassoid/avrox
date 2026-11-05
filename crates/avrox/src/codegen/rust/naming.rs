//! Conversion of Avro names to Rust identifiers.

use proc_macro2::{Ident, Span};

/// Rust keywords and reserved words that need escaping in identifiers.
const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// Keywords that cannot be used even as raw identifiers.
const UNRAWABLE: &[&str] = &["crate", "self", "super", "Self"];

/// Builds an identifier for `name`, escaping Rust keywords.
///
/// Returns the identifier and whether it differs from `name`: keywords become
/// raw identifiers (`r#type`) and the keywords in [`UNRAWABLE`] get an
/// underscore suffix (`self_`).
pub(super) fn make_ident(name: &str) -> (Ident, bool) {
    if UNRAWABLE.contains(&name) {
        (Ident::new(&format!("{name}_"), Span::call_site()), true)
    } else if KEYWORDS.contains(&name) {
        (Ident::new_raw(name, Span::call_site()), true)
    } else {
        (Ident::new(name, Span::call_site()), false)
    }
}

/// Converts an Avro name to `snake_case`.
pub(super) fn to_snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut result = String::with_capacity(name.len() + 4);
    for (index, &c) in chars.iter().enumerate() {
        if c == '_' {
            result.push('_');
            continue;
        }
        if c.is_uppercase() {
            let boundary = index > 0
                && (chars[index - 1].is_lowercase()
                    || chars[index - 1].is_ascii_digit()
                    || (chars[index - 1].is_uppercase()
                        && chars.get(index + 1).is_some_and(|next| next.is_lowercase())));
            if boundary {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Converts an Avro name to `UpperCamelCase`.
pub(super) fn to_upper_camel_case(name: &str) -> String {
    to_snake_case(name)
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut capitalized = String::with_capacity(word.len());
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                capitalized.extend(first.to_uppercase());
                capitalized.push_str(chars.as_str());
            }
            capitalized
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_snake_case() {
        let cases = [
            ("fullName", "full_name"),
            ("FullName", "full_name"),
            ("FULL_NAME", "full_name"),
            ("HTTPServer", "http_server"),
            ("v1Data", "v1_data"),
            ("md5", "md5"),
            ("A1B", "a1_b"),
            ("already_snake", "already_snake"),
            ("_private", "_private"),
            ("id", "id"),
        ];
        for (input, expected) in cases {
            assert_eq!(to_snake_case(input), expected, "case: {input}");
        }
    }

    #[test]
    fn converts_to_upper_camel_case() {
        let cases = [
            ("SPADES", "Spades"),
            ("fullName", "FullName"),
            ("HTTPServer", "HttpServer"),
            ("user", "User"),
            ("user_profile", "UserProfile"),
            ("md5", "Md5"),
            ("a1b", "A1b"),
        ];
        for (input, expected) in cases {
            assert_eq!(to_upper_camel_case(input), expected, "case: {input}");
        }
    }

    #[test]
    fn escapes_keywords() {
        let (ident, escaped) = make_ident("type");
        assert_eq!(ident.to_string(), "r#type");
        assert!(escaped);

        let (ident, escaped) = make_ident("self");
        assert_eq!(ident.to_string(), "self_");
        assert!(escaped);

        let (ident, escaped) = make_ident("normal");
        assert_eq!(ident.to_string(), "normal");
        assert!(!escaped);
    }
}
