//! Conversion of Avro names to exported Go identifiers.

use std::collections::HashSet;

const GO_KEYWORDS: &[&str] = &[
    "break",
    "default",
    "func",
    "interface",
    "select",
    "case",
    "defer",
    "go",
    "map",
    "struct",
    "chan",
    "else",
    "goto",
    "package",
    "switch",
    "const",
    "fallthrough",
    "if",
    "range",
    "type",
    "continue",
    "for",
    "import",
    "return",
    "var",
];

const INITIALISMS: &[&str] = &[
    "api", "ascii", "cpu", "css", "dns", "eof", "guid", "html", "http", "https", "id", "ip",
    "json", "qps", "ram", "rpc", "sla", "smtp", "sql", "ssh", "tcp", "tls", "ttl", "udp", "ui",
    "uid", "uri", "url", "urn", "utf8", "uuid", "vm", "xml", "xmpp", "xsrf", "xss",
];

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

pub(super) fn exported(name: &str) -> String {
    let result = to_snake_case(name)
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            if INITIALISMS.contains(&word) {
                return word.to_uppercase();
            }
            let mut result = String::with_capacity(word.len());
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                result.extend(first.to_uppercase());
                result.push_str(chars.as_str());
            }
            result
        })
        .collect::<String>();
    if result.is_empty() {
        "X".to_owned()
    } else if result.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("X{result}")
    } else {
        result
    }
}

pub(super) fn unique(candidate: &str, used: &mut HashSet<String>) -> String {
    if used.insert(candidate.to_owned()) {
        return candidate.to_owned();
    }
    let mut suffix = 2;
    loop {
        let name = format!("{candidate}{suffix}");
        if used.insert(name.clone()) {
            return name;
        }
        suffix += 1;
    }
}

pub(super) fn default_package(namespace: Option<&str>) -> String {
    let Some(component) = namespace.and_then(|value| value.rsplit('.').next()) else {
        return "avro".to_owned();
    };
    let mut package: String = component
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_lowercase();
    if package.is_empty() {
        return "avro".to_owned();
    }
    if package.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        package.insert_str(0, "avro");
    }
    if GO_KEYWORDS.contains(&package.as_str()) {
        package.push_str("pkg");
    }
    package
}

pub(super) fn valid_package(package: &str) -> bool {
    let mut chars = package.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        && !GO_KEYWORDS.contains(&package)
}

pub(super) fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_go_names() {
        assert_eq!(exported("fullName"), "FullName");
        assert_eq!(exported("deviceId"), "DeviceID");
        assert_eq!(exported("HTTPServer"), "HTTPServer");
        assert_eq!(exported("uuid"), "UUID");
        assert_eq!(exported("_private"), "Private");
        assert_eq!(exported("_1value"), "X1value");
        assert_eq!(exported("_"), "X");
    }

    #[test]
    fn derives_safe_package_names() {
        assert_eq!(
            default_package(Some("com.example.user_profiles")),
            "userprofiles"
        );
        assert_eq!(default_package(Some("com.example.type")), "typepkg");
        assert_eq!(
            default_package(Some("com.example._1profile")),
            "avro1profile"
        );
        assert_eq!(default_package(None), "avro");
        assert!(valid_package("profile"));
        assert!(!valid_package("package"));
        assert!(!valid_package("bad-name"));
    }
}
