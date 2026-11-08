//! Reads the output of `cargo clippy --message-format=short` with
//! `clippy::disallowed_methods` turned on, from the file given as the only
//! argument, and fails on every disallowed method call that a derive macro
//! did not generate. Run as in "Run the style check" in
//! `docs/how-to/run-the-tests.md`.

use std::process::ExitCode;

use rust_milter_style::{method_problems, repository_root};

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let (Some(path), None) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: disallowed-methods <file with Clippy's output>");
        return ExitCode::from(2);
    };
    let output = match std::fs::read_to_string(&path) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            return ExitCode::from(2);
        }
    };
    let problems = match method_problems(&repository_root(), &output) {
        Ok(problems) => problems,
        Err(error) => {
            eprintln!("cannot read a reported file: {error}");
            return ExitCode::from(2);
        }
    };

    for problem in &problems {
        println!("{problem}");
    }
    if problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
