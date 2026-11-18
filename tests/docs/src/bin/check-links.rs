//! Checks the links of the repository against its files and against the
//! rustdoc directory given as the only argument, as in "Link check" in
//! `docs/reference/test-environment.md`. CI runs it after `cargo doc`.

use std::path::Path;
use std::process::ExitCode;

use rust_milter_docs::links::{check_repository, repository_root};

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let (Some(doc), None) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: check-links <rustdoc directory>");
        return ExitCode::from(2);
    };
    let doc = Path::new(&doc);
    if !doc.is_dir() {
        eprintln!("{} is not a directory", doc.display());
        return ExitCode::from(2);
    }
    let report = match check_repository(&repository_root(), Some(doc)) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("cannot read the repository: {error}");
            return ExitCode::from(2);
        }
    };

    for problem in &report.problems {
        println!("{problem}");
    }
    let broken = report.problems.len();
    println!("links checked: {}, broken: {broken}", report.checked);
    if report.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
