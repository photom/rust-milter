//! The style check on the Rust files of the repository. The scenarios are
//! in `docs/testing/style.md`.

use std::collections::BTreeSet;

use rust_milter_style::{
    Scan, exemptions_path, parse_exemptions, repository_root, rust_files, scan,
};

/// The files that are not converted yet, from `exemptions.txt`.
fn exemptions() -> BTreeSet<String> {
    let path = exemptions_path();
    let text = std::fs::read_to_string(path).unwrap();
    let mut exempt = BTreeSet::new();
    for file in parse_exemptions(&text) {
        exempt.insert(file);
    }
    exempt
}

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
fn sty_010_a_file_off_the_list_has_no_closure_and_no_fn_bound() {
    let exempt = exemptions();
    let mut problems = Vec::new();
    for (file, scanned) in scans() {
        if exempt.contains(&file) {
            continue;
        }
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
                 list the file in exemptions.txt instead"
            ));
        }
    }

    assert_none(&problems);
}

#[test]
fn sty_012_every_listed_file_exists_once_and_in_order() {
    let path = exemptions_path();
    let listed = parse_exemptions(&std::fs::read_to_string(path).unwrap());
    let mut problems = Vec::new();
    let mut previous: Option<&String> = None;
    for file in &listed {
        if !repository_root().join(file).is_file() {
            problems.push(format!("{file} is listed but does not exist"));
        }
        if let Some(before) = previous
            && before >= file
        {
            problems.push(format!("{file} is out of order or repeated"));
        }
        previous = Some(file);
    }

    assert_none(&problems);
}
