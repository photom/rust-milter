//! Reading the disallowed methods from Clippy's output. The scenarios are
//! in `docs/testing/style.md`.

use std::path::PathBuf;

use rust_milter_style::{MethodCall, disallowed_method_calls, method_problems};

/// A method call from its parts.
fn call(file: &str, line: usize, column: usize, method: &str) -> MethodCall {
    MethodCall {
        file: file.to_owned(),
        line,
        column,
        method: method.to_owned(),
    }
}

#[test]
fn sty_020_disallowed_method_calls_are_read_and_other_lines_are_not() {
    let output = "    Checking rust-milter v0.1.0\n\
                  src/config/env.rs:12:5: warning: use of a disallowed method `std::option::Option::map`\n\
                  src/config/env.rs:9:1: warning: unused import: `std::fmt`\n\
                  tests/reload.rs:40:17: error: use of a disallowed method `std::iter::Iterator::any`\n\
                  warning: `rust-milter` (lib) generated 1 warning\n";

    assert_eq!(
        disallowed_method_calls(output),
        [
            call("src/config/env.rs", 12, 5, "std::option::Option::map"),
            call("tests/reload.rs", 40, 17, "std::iter::Iterator::any"),
        ]
    );
}

#[test]
fn sty_021_a_path_through_a_parent_directory_is_resolved() {
    let output = "tests/docs/src/../../support/logs.rs:3:9: warning: use of a disallowed method `std::result::Result::map_err`\n";

    assert_eq!(
        disallowed_method_calls(output),
        [call(
            "tests/support/logs.rs",
            3,
            9,
            "std::result::Result::map_err"
        )]
    );
}

#[test]
fn sty_022_a_call_reported_for_two_targets_counts_once_in_source_order() {
    let output = "src/a.rs:9:1: warning: use of a disallowed method `bool::then`\n\
                  src/a.rs:2:4: warning: use of a disallowed method `bool::then`\n\
                  src/a.rs:9:1: warning: use of a disallowed method `bool::then`\n";

    assert_eq!(
        disallowed_method_calls(output),
        [
            call("src/a.rs", 2, 4, "bool::then"),
            call("src/a.rs", 9, 1, "bool::then"),
        ]
    );
}

#[test]
fn sty_025_a_line_with_terminal_colors_is_read() {
    let output = "\u{1b}[0msrc/a.rs:7:3: \u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[33mwarning\u{1b}[0m\u{1b}[0m: use of a disallowed method `std::iter::Iterator::any`\u{1b}[0m\n";

    assert_eq!(
        disallowed_method_calls(output),
        [call("src/a.rs", 7, 3, "std::iter::Iterator::any")]
    );
}

/// A directory under the system's temporary directory with the given
/// files, from a clean start.
fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rust-milter-style-{name}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    for (file, source) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    root
}

#[test]
fn sty_023_each_call_is_a_problem() {
    let root = tree(
        "sty-023",
        &[("src/a.rs", "fn f() {}\n"), ("src/b.rs", "fn f() {}\n")],
    );
    let output = "src/a.rs:1:1: warning: use of a disallowed method `bool::then`\n\
                  src/b.rs:2:3: warning: use of a disallowed method `std::option::Option::map`\n";

    let problems = method_problems(&root, output).unwrap();

    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[0].starts_with("src/a.rs:1:1: "), "{problems:?}");
    assert!(problems[0].contains("bool::then"), "{problems:?}");
    assert!(problems[1].starts_with("src/b.rs:2:3: "), "{problems:?}");
    assert!(
        problems[1].contains("std::option::Option::map"),
        "{problems:?}"
    );
}

#[test]
fn sty_026_a_call_that_a_derive_generates_is_not_a_problem() {
    let source = "#[derive(Debug)]\n\
                  #[cfg_attr(\n\
                  \x20   feature = \"serde\",\n\
                  \x20   derive(serde::Deserialize)\n\
                  )]\n\
                  struct A {\n\
                  \x20   b: u8,\n\
                  }\n";
    let root = tree("sty-026", &[("src/a.rs", source)]);
    let output = "src/a.rs:1:10: warning: use of a disallowed method `std::option::Option::map`\n\
                  src/a.rs:4:12: warning: use of a disallowed method `std::option::Option::map`\n\
                  src/a.rs:7:5: warning: use of a disallowed method `std::option::Option::map`\n";

    let problems = method_problems(&root, output).unwrap();

    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("src/a.rs:7:5: "), "{problems:?}");
}
