//! Finds the Markdown files of `docs/tutorials/` and `docs/how-to/`,
//! prepares a copy of each in `OUT_DIR` with `setup::prepare`, and writes
//! `documents.rs`, which includes every copy as a doctest.

#[path = "src/setup.rs"]
mod setup;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The directories whose documents are checked, from the repository root.
const DIRECTORIES: [&str; 2] = ["docs/tutorials", "docs/how-to"];

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let mut documents = Vec::new();
    for directory in DIRECTORIES {
        let path = root.join(directory);
        println!("cargo::rerun-if-changed={}", path.display());
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => panic!("cannot read {}: {error}", path.display()),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => panic!("cannot read {directory}: {error}"),
            };
            let Ok(name) = entry.file_name().into_string() else {
                panic!("{directory} has a file name that is not UTF-8");
            };
            if name.ends_with(".md") {
                documents.push(format!("{directory}/{name}"));
            }
        }
    }
    documents.sort();

    let mut generated = format!("#[cfg(test)]\nconst DOCUMENTS: &[&str] = &{documents:?};\n");
    for document in &documents {
        let text = match std::fs::read_to_string(root.join(document)) {
            Ok(text) => text,
            Err(error) => panic!("cannot read {document}: {error}"),
        };
        let prepared = match setup::prepare(&text) {
            Ok(prepared) => prepared,
            Err(error) => panic!("{document}: {error}"),
        };
        let copy = out.join(document);
        let parent = copy.parent().expect("a document is in a directory");
        if let Err(error) = std::fs::create_dir_all(parent) {
            panic!("cannot create {}: {error}", parent.display());
        }
        if let Err(error) = std::fs::write(&copy, prepared) {
            panic!("cannot write {}: {error}", copy.display());
        }
        let name = document
            .trim_start_matches("docs/")
            .trim_end_matches(".md")
            .replace(['/', '-'], "_");
        let path = copy.to_str().expect("OUT_DIR is UTF-8");
        writeln!(
            generated,
            "/// `{document}`.\n#[cfg(all(doctest, feature = \"check\"))]\n#[doc = include_str!({path:?})]\npub mod {name} {{}}"
        )
        .expect("writing to a String does not fail");
    }
    let target = out.join("documents.rs");
    if let Err(error) = std::fs::write(&target, generated) {
        panic!("cannot write {}: {error}", target.display());
    }
}
