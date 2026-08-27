use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;

/// One problem found while scanning a CSV file.
#[derive(Debug, Clone)]
pub struct Finding {
    pub line: usize,
    pub rule: &'static str,
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.line, self.rule, self.message)
    }
}

/// Whether a finding should fail a normal run, or only a `--strict` one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Finding {
    /// `stray-quote` is often a false positive - an apostrophe-heavy text
    /// field, a name with an embedded quote mark - so on its own it is
    /// reported but does not fail the run unless `--strict` is passed.
    /// Everything else points at a row that is actually malformed.
    pub fn severity(&self) -> Severity {
        match self.rule {
            "stray-quote" => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

struct Record {
    line: usize,
    fields: Vec<String>,
}

/// Scans `input` and returns every finding, sorted by line number.
///
/// The first record is treated as the header; every later record is
/// compared against it for field count. Parsing itself can also produce
/// findings (an unclosed quote, a stray quote outside a quoted field, an
/// unquoted field with trailing whitespace).
pub fn lint(input: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let records = parse_records(input, &mut findings);

    if let Some(header) = records.first() {
        let expected = header.fields.len();

        let mut columns_by_name: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, name) in header.fields.iter().enumerate() {
            columns_by_name.entry(name.as_str()).or_default().push(i + 1);
        }
        let mut already_reported: HashSet<&str> = HashSet::new();
        for name in &header.fields {
            if !already_reported.insert(name.as_str()) {
                continue;
            }
            let columns = &columns_by_name[name.as_str()];
            if columns.len() > 1 {
                let column_list = columns
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                findings.push(Finding {
                    line: header.line,
                    rule: "duplicate-column",
                    message: format!(
                        "column name {:?} appears more than once (columns {})",
                        name, column_list
                    ),
                });
            }
        }

        for record in &records[1..] {
            let actual = record.fields.len();
            if actual != expected {
                findings.push(Finding {
                    line: record.line,
                    rule: "ragged-row",
                    message: format!(
                        "row has {} field{} but header has {}",
                        actual,
                        if actual == 1 { "" } else { "s" },
                        expected
                    ),
                });
            }
        }
    }

    findings.sort_by_key(|f| f.line);
    findings
}

// Parses `input` as comma-separated, double-quote-quoted records (RFC 4180
// style, with `""` as the escape for a literal quote inside a quoted
// field). A completely blank line is skipped rather than turned into a
// one-field row of empty string, since that is what every CSV export we
// have run into actually means by a blank line.
fn parse_records(input: &str, findings: &mut Vec<Finding>) -> Vec<Record> {
    let mut records = Vec::new();
    let mut chars = input.chars().peekable();
    let mut line = 1usize;

    while chars.peek().is_some() {
        let record_start_line = line;
        let mut fields: Vec<String> = Vec::new();
        let mut field = String::new();
        let mut in_quotes = false;
        let mut field_is_quoted = false;
        let mut record_has_content = false;

        loop {
            let c = match chars.next() {
                Some(c) => c,
                None => {
                    if !field_is_quoted {
                        push_trailing_whitespace_finding(findings, line, &field);
                    }
                    fields.push(field);
                    if in_quotes {
                        findings.push(Finding {
                            line: record_start_line,
                            rule: "unclosed-quote",
                            message: "quoted field is never closed before end of file"
                                .to_string(),
                        });
                    }
                    break;
                }
            };

            if in_quotes {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                } else {
                    if c == '\n' {
                        line += 1;
                    }
                    field.push(c);
                }
                continue;
            }

            match c {
                '"' if field.is_empty() => {
                    in_quotes = true;
                    field_is_quoted = true;
                    record_has_content = true;
                }
                '"' => {
                    findings.push(Finding {
                        line,
                        rule: "stray-quote",
                        message: "quote character found outside a quoted field".to_string(),
                    });
                    field.push(c);
                    record_has_content = true;
                }
                ',' => {
                    if !field_is_quoted {
                        push_trailing_whitespace_finding(findings, line, &field);
                    }
                    fields.push(std::mem::take(&mut field));
                    field_is_quoted = false;
                    record_has_content = true;
                }
                '\r' => {}
                '\n' => {
                    if !field_is_quoted {
                        push_trailing_whitespace_finding(findings, line, &field);
                    }
                    fields.push(std::mem::take(&mut field));
                    field_is_quoted = false;
                    line += 1;
                    break;
                }
                _ => {
                    field.push(c);
                    record_has_content = true;
                }
            }
        }

        let is_blank_line = !record_has_content && fields.len() == 1 && fields[0].is_empty();
        if !is_blank_line {
            records.push(Record {
                line: record_start_line,
                fields,
            });
        }
    }

    records
}

// Whitespace at the end of an unquoted field is almost always a stray space
// from hand-editing or a misconfigured export, since anyone who meant to
// keep it would have quoted the field. Quoted fields are exempt: quoting is
// exactly how CSV says "keep this whitespace on purpose".
fn push_trailing_whitespace_finding(findings: &mut Vec<Finding>, line: usize, field: &str) {
    if field.ends_with(' ') || field.ends_with('\t') {
        findings.push(Finding {
            line,
            rule: "trailing-whitespace",
            message: "unquoted field has trailing whitespace before the delimiter".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Case {
        name: &'static str,
        input: &'static str,
        expected: &'static [(usize, &'static str)],
    }

    const CASES: &[Case] = &[
        Case {
            name: "well formed file has no findings",
            input: "name,age\nAlice,30\nBob,25\n",
            expected: &[],
        },
        Case {
            name: "missing trailing newline on last row is fine",
            input: "a,b\nc,d",
            expected: &[],
        },
        Case {
            name: "short row is flagged",
            input: "a,b,c\n1,2\n",
            expected: &[(2, "ragged-row")],
        },
        Case {
            name: "long row is flagged",
            input: "a,b\n1,2,3\n",
            expected: &[(2, "ragged-row")],
        },
        Case {
            name: "comma inside quotes is not a field separator",
            input: "name,note\nAlice,\"hi, there\"\n",
            expected: &[],
        },
        Case {
            name: "quoted field can contain a newline, line number is where the row started",
            input: "a,b\n\"multi\nline\",x\nc,d\n",
            expected: &[],
        },
        Case {
            name: "doubled quote is an escaped literal quote, not a close",
            input: "a,b\n\"she said \"\"hi\"\"\",2\n",
            expected: &[],
        },
        Case {
            name: "unclosed quote at end of file",
            input: "a,b\n\"oops,2",
            expected: &[(2, "unclosed-quote"), (2, "ragged-row")],
        },
        Case {
            name: "bare quote inside an unquoted field",
            input: "a,b\nhe said \"hi\",2\n",
            expected: &[(2, "stray-quote"), (2, "stray-quote")],
        },
        Case {
            name: "blank line in the middle is ignored, not a one-field row",
            input: "a,b\n1,2\n\n3,4\n",
            expected: &[],
        },
        Case {
            name: "blank lines at end of file are ignored",
            input: "a,b\n1,2\n\n\n",
            expected: &[],
        },
        Case {
            name: "empty file has no findings",
            input: "",
            expected: &[],
        },
        Case {
            name: "header only file has no findings",
            input: "name,age\n",
            expected: &[],
        },
        Case {
            name: "crlf line endings are handled like lf",
            input: "a,b\r\n1,2\r\n",
            expected: &[],
        },
        Case {
            name: "empty quoted field is a real field, not a blank line",
            input: "a\n\"\"\n",
            expected: &[],
        },
        Case {
            name: "duplicate column name in header is flagged",
            input: "a,b,a\n1,2,3\n",
            expected: &[(1, "duplicate-column")],
        },
        Case {
            name: "column name repeated three times reports one finding",
            input: "a,a,a\n1,2,3\n",
            expected: &[(1, "duplicate-column")],
        },
        Case {
            name: "two different duplicate names each report separately",
            input: "a,b,a,b\n1,2,3,4\n",
            expected: &[(1, "duplicate-column"), (1, "duplicate-column")],
        },
        Case {
            name: "repeated empty column names count as a duplicate too",
            input: "a,,b,\n1,2,3,4\n",
            expected: &[(1, "duplicate-column")],
        },
        Case {
            name: "trailing space before a comma is flagged",
            input: "a,b\nfoo ,2\n",
            expected: &[(2, "trailing-whitespace")],
        },
        Case {
            name: "trailing tab before a newline is flagged",
            input: "a,b\n1,2\t\n",
            expected: &[(2, "trailing-whitespace")],
        },
        Case {
            name: "trailing whitespace on the last field with no trailing newline is flagged",
            input: "a,b\n1,2 ",
            expected: &[(2, "trailing-whitespace")],
        },
        Case {
            name: "trailing whitespace inside a quoted field is not flagged",
            input: "a,b\n\"foo \",2\n",
            expected: &[],
        },
        Case {
            name: "leading whitespace in an unquoted field is not flagged",
            input: "a,b\n foo,2\n",
            expected: &[],
        },
    ];

    #[test]
    fn stray_quote_is_a_warning_everything_else_is_an_error() {
        let findings = lint("a,b\nhe said \"hi\",2\n\"oops,2");
        for finding in &findings {
            let expected = if finding.rule == "stray-quote" {
                Severity::Warning
            } else {
                Severity::Error
            };
            assert_eq!(
                finding.severity(),
                expected,
                "rule {:?} had unexpected severity",
                finding.rule
            );
        }
    }

    #[test]
    fn table() {
        for case in CASES {
            let findings = lint(case.input);
            let actual: Vec<(usize, &'static str)> =
                findings.iter().map(|f| (f.line, f.rule)).collect();
            assert_eq!(
                actual.as_slice(),
                case.expected,
                "case {:?}: got {:#?}",
                case.name,
                findings
            );
        }
    }
}
