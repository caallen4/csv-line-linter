use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

mod lint;

fn main() -> ExitCode {
    let mut paths: Vec<String> = env::args().skip(1).collect();

    // No file given, or an explicit "-", both mean "read from stdin" -
    // that way csvlint drops into a pipeline the same way grep or cat do.
    if paths.is_empty() {
        paths.push("-".to_string());
    }

    let mut found_any = false;

    for path in &paths {
        let contents = if path == "-" {
            let mut buf = String::new();
            match io::stdin().read_to_string(&mut buf) {
                Ok(_) => buf,
                Err(e) => {
                    eprintln!("stdin: {}", e);
                    found_any = true;
                    continue;
                }
            }
        } else {
            match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{}: {}", path, e);
                    found_any = true;
                    continue;
                }
            }
        };

        let label = if path == "-" { "stdin" } else { path.as_str() };
        for finding in lint::lint(&contents) {
            println!("{}:{}", label, finding);
            found_any = true;
        }
    }

    if found_any {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
