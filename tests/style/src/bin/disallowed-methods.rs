//! Reads the output of `cargo clippy --message-format=short` with
//! `clippy::disallowed_methods` turned on, from the file given as the only
//! argument. Fails on a disallowed method call in a file that is not on
//! `tests/style/exemptions.txt`, and on a listed file that has nothing left
//! to convert. Run as in "Run the style check" in
//! `docs/how-to/run-the-tests.md`.

use std::process::ExitCode;

use rust_milter_style::{exemptions_path, method_problems, parse_exemptions, repository_root};

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
    let list = match std::fs::read_to_string(exemptions_path()) {
        Ok(list) => list,
        Err(error) => {
            eprintln!("cannot read the exemption list: {error}");
            return ExitCode::from(2);
        }
    };
    let exempt = parse_exemptions(&list);
    let problems = match method_problems(&repository_root(), &output, &exempt) {
        Ok(problems) => problems,
        Err(error) => {
            eprintln!("cannot read a listed file: {error}");
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
