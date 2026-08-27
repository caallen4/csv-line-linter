use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

mod lint;

fn main() -> ExitCode {
    let mut strict = false;
    let mut paths: Vec<String> = Vec::new();
    for arg in env::args().skip(1) {
        if arg == "--strict" {
            strict = true;
        } else {
            paths.push(arg);
        }
    }

    // No file given, or an explicit "-", both mean "read from stdin" -
    // that way csvlint drops into a pipeline the same way grep or cat do.
    if paths.is_empty() {
        paths.push("-".to_string());
    }

    let mut should_fail = false;

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
        for finding in lint::lint(&contents) {
            println!("{}:{}", label, finding);
            if strict || finding.severity() == lint::Severity::Error {
                should_fail = true;
            }
        }
    }

    if should_fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
