//! The style check on the Rust files of the repository. The scenarios are
//! in `docs/testing/style.md`.

use rust_milter_style::{Scan, repository_root, rust_files, scan};

/// Every Rust file of the repository with its scan.
fn scans() -> Vec<(String, Scan)> {
    let root = repository_root();
    let mut scans = Vec::new();
    for file in rust_files(&root).unwrap() {
        let source = std::fs::read_to_string(root.join(&file)).unwrap();
        match scan(&file, &source) {
            Ok(scanned) => scans.push((file, scanned)),
            Err(error) => panic!("{file}: {error}"),
        }
    }
    scans
}

/// Fails with one line per problem when there is any.
fn assert_none(problems: &[String]) {
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn sty_010_no_file_has_a_closure_fn_bound_or_function_pointer() {
    let mut problems = Vec::new();
    for (file, scanned) in scans() {
        for finding in scanned.findings {
            problems.push(format!(
                "{file}:{}:{}: {}",
                finding.line, finding.column, finding.kind
            ));
        }
    }

    assert_none(&problems);
}

#[test]
fn sty_011_no_file_allows_or_expects_disallowed_methods() {
    let mut problems = Vec::new();
    for (file, scanned) in scans() {
        if scanned.allows_disallowed_methods {
            problems.push(format!(
                "{file} allows or expects clippy::disallowed_methods; \
                 write a match or a loop instead"
            ));
        }
    }

    assert_none(&problems);
}
