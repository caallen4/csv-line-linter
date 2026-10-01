use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

mod lint;

fn main() -> ExitCode {
    let mut strict = false;
    let mut json = false;
    let mut delimiter = ',';
    let mut disabled: Vec<String> = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--strict" {
            strict = true;
        } else if arg == "--json" {
            json = true;
        } else if arg == "--delimiter" {
            i += 1;
            let raw = match args.get(i) {
                Some(v) => v,
                None => {
                    eprintln!("--delimiter requires a value");
                    return ExitCode::from(2);
                }
            };
            delimiter = match parse_delimiter(raw) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("--delimiter: {}", e);
                    return ExitCode::from(2);
                }
            };
        } else if arg == "--rules" {
            i += 1;
            let raw = match args.get(i) {
                Some(v) => v,
                None => {
                    eprintln!("--rules requires a value");
                    return ExitCode::from(2);
                }
            };
            match parse_rules(raw) {
                Ok(names) => disabled.extend(names),
                Err(e) => {
                    eprintln!("--rules: {}", e);
                    return ExitCode::from(2);
                }
            }
        } else {
            paths.push(arg.clone());
        }
        i += 1;
    }

    // No file given, or an explicit "-", both mean "read from stdin" -
    // that way csvlint drops into a pipeline the same way grep or cat do.
    if paths.is_empty() {
        paths.push("-".to_string());
    }

    let mut should_fail = false;
    let mut json_findings: Vec<String> = Vec::new();

    for path in &paths {
        let contents = if path == "-" {
            let mut buf = String::new();
            match io::stdin().read_to_string(&mut buf) {
                Ok(_) => buf,
                Err(e) => {
                    eprintln!("stdin: {}", e);
                    should_fail = true;
                    continue;
                }
            }
        } else {
            match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{}: {}", path, e);
                    should_fail = true;
                    continue;
                }
            }
        };

        let label = if path == "-" { "stdin" } else { path.as_str() };
        for finding in lint::lint_with_delimiter(&contents, delimiter) {
            if disabled.iter().any(|name| name == finding.rule) {
                continue;
            }
            let is_error = finding.severity() == lint::Severity::Error;
            if strict || is_error {
                should_fail = true;
            }
            if json {
                json_findings.push(format!(
                    "{{\"file\":{},\"line\":{},\"rule\":{},\"severity\":{},\"message\":{}}}",
                    json_string(label),
                    finding.line,
                    json_string(finding.rule),
                    json_string(if is_error { "error" } else { "warning" }),
                    json_string(&finding.message),
                ));
            } else {
                println!("{}:{}", label, finding);
            }
        }
    }

    if json {
        println!("[{}]", json_findings.join(","));
    }

    if should_fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

// Takes a comma-separated list where every entry is a rule name with a
// leading "-", e.g. "-stray-quote,-trailing-whitespace", and returns the
// bare names. Requiring the "-" leaves room for a plain name to mean "only
// run this" later without changing what existing command lines do.
fn parse_rules(raw: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for entry in raw.split(',') {
        let name = entry.trim().strip_prefix('-').ok_or_else(|| {
            format!("{:?} must start with '-' to disable that rule", entry.trim())
        })?;
        if !lint::RULES.contains(&name) {
            return Err(format!(
                "unknown rule {:?} (known rules: {})",
                name,
                lint::RULES.join(", ")
            ));
        }
        names.push(name.to_string());
    }
    Ok(names)
}

// Accepts either a single literal character or the two-character escape
// "\t", since a real tab is awkward to type on a command line but tab
// separated files are common enough to deserve a shorthand.
fn parse_delimiter(raw: &str) -> Result<char, String> {
    let c = if raw == "\\t" {
        '\t'
    } else {
        let mut chars = raw.chars();
        let first = chars
            .next()
            .ok_or_else(|| "delimiter cannot be empty".to_string())?;
        if chars.next().is_some() {
            return Err(format!("delimiter must be a single character, got {:?}", raw));
        }
        first
    };

    if c == '"' {
        return Err("delimiter cannot be a double quote".to_string());
    }
    if c == '\n' || c == '\r' {
        return Err("delimiter cannot be a newline".to_string());
    }
    Ok(c)
}

// Minimal JSON string encoder - pulling in serde_json for one output mode
// would violate the no-dependencies rule, and the escaping rules for a
// string value are short enough to just write out.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_escapes_control_and_special_characters() {
        assert_eq!(json_string("plain"), "\"plain\"");
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(json_string("a\nb"), "\"a\\nb\"");
        assert_eq!(json_string("a\tb"), "\"a\\tb\"");
        assert_eq!(json_string("a\u{1}b"), "\"a\\u0001b\"");
    }

    #[test]
    fn parse_rules_returns_bare_names() {
        assert_eq!(
            parse_rules("-stray-quote, -ragged-row"),
            Ok(vec!["stray-quote".to_string(), "ragged-row".to_string()])
        );
    }

    #[test]
    fn parse_rules_rejects_unknown_names_and_missing_prefix() {
        assert!(parse_rules("-no-such-rule").is_err());
        assert!(parse_rules("stray-quote").is_err());
        assert!(parse_rules("").is_err());
        assert!(parse_rules("-stray-quote,").is_err());
    }

    #[test]
    fn parse_delimiter_accepts_a_single_character() {
        assert_eq!(parse_delimiter(";"), Ok(';'));
        assert_eq!(parse_delimiter("|"), Ok('|'));
    }

    #[test]
    fn parse_delimiter_accepts_the_tab_escape() {
        assert_eq!(parse_delimiter("\\t"), Ok('\t'));
    }

    #[test]
    fn parse_delimiter_rejects_empty_multi_character_and_reserved_values() {
        assert!(parse_delimiter("").is_err());
        assert!(parse_delimiter("::").is_err());
        assert!(parse_delimiter("\"").is_err());
        assert!(parse_delimiter("\n").is_err());
    }
}
