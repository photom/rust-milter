//! Compiles the Rust examples in `docs/tutorials/` and `docs/how-to/` as
//! doctests. `build.rs` finds the documents, prepares a copy of each, and
//! includes the copies here. The rules are in "Documentation check" in
//! `docs/reference/test-environment.md`.

#[cfg(test)]
mod setup;

/// The test support code of `tests/`, for the examples of "Write a network
/// test".
#[cfg(feature = "check")]
#[doc(hidden)]
#[path = "../../support/mod.rs"]
pub mod support;

include!(concat!(env!("OUT_DIR"), "/documents.rs"));

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::setup::prepare;
    use super::*;

    #[test]
    fn doc_002_every_document_is_checked() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut found = Vec::new();
        for directory in ["docs/tutorials", "docs/how-to"] {
            for entry in std::fs::read_dir(root.join(directory)).unwrap() {
                let name = entry.unwrap().file_name().into_string().unwrap();
                if name.ends_with(".md") {
                    found.push(format!("{directory}/{name}"));
                }
            }
        }
        found.sort();

        assert!(!found.is_empty());
        assert_eq!(found, DOCUMENTS);
    }

    #[test]
    fn doc_003_a_setup_comment_is_merged_into_its_block() {
        let document = "Text.\n\
            \n\
            <!-- doctest-setup\n\
            fn main() {\n\
            ---\n\
            }\n\
            -->\n\
            \n\
            ```rust\n\
            let x = 1;\n\
            ```\n";

        let prepared = prepare(document).unwrap();

        assert_eq!(
            prepared,
            "Text.\n\
            \n\
            \n\
            ```rust,no_run\n\
            fn main() {\n\
            let x = 1;\n\
            }\n\
            ```\n"
        );
    }

    #[test]
    fn doc_003_setup_lines_take_the_indentation_of_the_block() {
        let document = "- Item:\n\
            \n\
            \x20 <!-- doctest-setup\n\
            \x20 fn main() {\n\
            \x20 ---\n\
            \x20 }\n\
            \x20 -->\n\
            \x20 ```rust\n\
            \x20 let x = 1;\n\
            \x20 ```\n";

        let prepared = prepare(document).unwrap();

        assert_eq!(
            prepared,
            "- Item:\n\
            \n\
            \x20 ```rust,no_run\n\
            \x20 fn main() {\n\
            \x20 let x = 1;\n\
            \x20 }\n\
            \x20 ```\n"
        );
    }

    #[test]
    fn doc_003_setup_without_a_split_line_goes_before_the_block() {
        let document = "<!-- doctest-setup\nuse std::io;\n-->\n```rust\nfn main() {}\n```\n";

        let prepared = prepare(document).unwrap();

        assert_eq!(
            prepared,
            "```rust,no_run\nuse std::io;\nfn main() {}\n```\n"
        );
    }

    #[test]
    fn doc_004_a_setup_comment_must_come_right_before_a_rust_block() {
        let comment = "Text.\n<!-- doctest-setup\nfn main() {}\n-->\n";
        for after in ["\nMore text.\n", "```toml\n[a]\n```\n", ""] {
            let document = format!("{comment}{after}");

            let error = prepare(&document).unwrap_err();

            assert!(error.contains("line 2"), "{error}");
        }
    }

    #[test]
    fn doc_004_an_unclosed_setup_comment_fails() {
        let error = prepare("<!-- doctest-setup\nfn main() {}\n").unwrap_err();

        assert!(error.contains("line 1"), "{error}");
    }

    #[test]
    fn doc_005_a_block_without_a_language_fails() {
        let error = prepare("Text.\n\n```\nlog output\n```\n").unwrap_err();

        assert!(error.contains("line 3"), "{error}");
    }

    #[test]
    fn doc_006_rust_blocks_are_compiled_and_not_run() {
        let document = "```rust\nfn main() {}\n```\n\
            ```rust,ignore\nmod support;\n```\n\
            ```sh\ncargo test\n```\n\
            ```text\nlog output\n```\n";

        let prepared = prepare(document).unwrap();

        assert_eq!(
            prepared,
            "```rust,no_run\nfn main() {}\n```\n\
            ```rust,ignore\nmod support;\n```\n\
            ```sh\ncargo test\n```\n\
            ```text\nlog output\n```\n"
        );
    }

    #[test]
    fn doc_006_a_fence_inside_a_block_is_not_a_block() {
        let document = "````text\n```\ninside\n```\n````\n";

        let prepared = prepare(document).unwrap();

        assert_eq!(prepared, document);
    }
}
