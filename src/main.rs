use std::env;
use std::fs;
use std::process::ExitCode;

mod lint;

fn main() -> ExitCode {
    let paths: Vec<String> = env::args().skip(1).collect();

    if paths.is_empty() {
        eprintln!("usage: csvlint <file.csv> [more.csv ...]");
        return ExitCode::from(2);
    }

    let mut found_any = false;

    for path in &paths {
        let contents = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{}: {}", path, e);
                found_any = true;
                continue;
            }
        };

        for finding in lint::lint(&contents) {
            println!("{}:{}", path, finding);
            found_any = true;
        }
    }

    if found_any {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
