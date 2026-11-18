//! The link check: reads the links of the Markdown documents and of the
//! library's doc comments, and checks each one against the files of the
//! repository and, when given, the rustdoc output. The rules are in "Link
//! check" in `docs/reference/test-environment.md`.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// The start of every link to the API reference on GitHub Pages.
pub const PAGES: &str = "https://photom.github.io/rust-milter/";

/// The starts of the links to files of the repository on `main`.
pub const REPOSITORY: [&str; 2] = [
    "https://github.com/photom/rust-milter/blob/main/",
    "https://github.com/photom/rust-milter/tree/main/",
];

/// The directories the walk over the repository skips.
const SKIPPED: [&str; 2] = [".git", "target"];

/// The kind of file a link is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A Markdown document.
    Markdown,
    /// A Rust source file, of which only the doc comments are read.
    Rust,
}

/// A link target and the line it is on, counting from 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The line number.
    pub line: usize,
    /// The target, as written.
    pub target: String,
}

/// What a link target points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// A page of the API reference: the path under [`PAGES`].
    Api {
        /// The path of the page.
        path: &'a str,
        /// The anchor, without `#`.
        anchor: Option<&'a str>,
    },
    /// A file of the repository: the path under one of [`REPOSITORY`].
    Repository {
        /// The path from the repository root.
        path: &'a str,
        /// The anchor, without `#`.
        anchor: Option<&'a str>,
    },
    /// A target without a scheme. Empty when the target is only an anchor.
    Relative {
        /// The path from the file's directory.
        path: &'a str,
        /// The anchor, without `#`.
        anchor: Option<&'a str>,
    },
    /// Any other target, which is not checked.
    Other,
}

/// A broken link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The file, from the repository root with `/` separators.
    pub file: String,
    /// The line number.
    pub line: usize,
    /// The target, as written.
    pub target: String,
    /// Why the link is broken.
    pub reason: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: ", self.file, self.line)?;
        write!(f, "{}: {}", self.target, self.reason)
    }
}

/// What a check of the whole repository found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// How many links were read.
    pub checked: usize,
    /// The broken links.
    pub problems: Vec<Problem>,
}

/// The root of the repository.
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The link targets in `text`, a file of the given kind.
pub fn links(text: &str, kind: Kind) -> Vec<Link> {
    let mut found = Vec::new();
    let mut fence: Option<&'static str> = None;
    for (index, line) in text.lines().enumerate() {
        let content = match kind {
            Kind::Markdown => line,
            Kind::Rust => match doc_comment(line) {
                Some(content) => content,
                None => continue,
            },
        };
        if update_fence(content, &mut fence) || fence.is_some() {
            continue;
        }
        let mut targets = Vec::new();
        if let Some(target) = definition(content) {
            targets.push(target.to_owned());
        }
        inline_targets(content, &mut targets);
        for target in targets {
            found.push(Link {
                line: index + 1,
                target,
            });
        }
    }
    found
}

/// The text of a `///` or `//!` doc comment line, after its marker.
fn doc_comment(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("////") {
        return None;
    }
    if let Some(content) = trimmed.strip_prefix("///") {
        return Some(content);
    }
    trimmed.strip_prefix("//!")
}

/// Opens or closes a fenced code block at `line`. Returns whether `line` is
/// a fence line.
fn update_fence(line: &str, fence: &mut Option<&'static str>) -> bool {
    let trimmed = line.trim_start();
    let marker = if trimmed.starts_with("```") {
        "```"
    } else if trimmed.starts_with("~~~") {
        "~~~"
    } else {
        return false;
    };
    if fence.is_none() {
        *fence = Some(marker);
    } else if *fence == Some(marker) {
        *fence = None;
    }
    true
}

/// The target of a reference definition `[label]: target`, if `line` is one.
fn definition(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('[') {
        return None;
    }
    let close = trimmed.find(']')?;
    let rest = trimmed[close + 1..].strip_prefix(':')?;
    rest.split_whitespace().next()
}

/// Adds the targets of the inline links `[text](target)` of `line` that are
/// outside inline code spans.
fn inline_targets(line: &str, targets: &mut Vec<String>) {
    let bytes = line.as_bytes();
    let mut in_code = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'`' {
            in_code = !in_code;
        } else if !in_code && byte == b']' && bytes.get(index + 1) == Some(&b'(') {
            let start = index + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b')' && !bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            if end > start {
                targets.push(line[start..end].to_owned());
            }
            index = end;
            continue;
        }
        index += 1;
    }
}

/// The anchors of the headings in a Markdown text, as GitHub computes them.
pub fn anchors(text: &str) -> HashSet<String> {
    let mut anchors = HashSet::new();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut fence: Option<&'static str> = None;
    for line in text.lines() {
        if update_fence(line, &mut fence) || fence.is_some() {
            continue;
        }
        let Some(heading) = heading_text(line) else {
            continue;
        };
        let base = slug(heading);
        let seen = counts.entry(base.clone()).or_insert(0);
        if *seen == 0 {
            anchors.insert(base);
        } else {
            anchors.insert(format!("{base}-{seen}"));
        }
        *seen += 1;
    }
    anchors
}

/// The text of an ATX heading, without its `#` marks.
fn heading_text(line: &str) -> Option<&str> {
    let mut level = 0;
    for c in line.chars() {
        if c != '#' {
            break;
        }
        level += 1;
    }
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if rest.is_empty() {
        return Some(rest);
    }
    rest.strip_prefix(' ')
}

/// GitHub's anchor for the text of a heading.
pub fn slug(heading: &str) -> String {
    let text = without_link_targets(&without_html_comments(heading));
    let mut slug = String::new();
    for c in text.trim().chars() {
        if c == ' ' {
            slug.push('-');
        } else if c.is_alphanumeric() || c == '_' || c == '-' {
            for lower in c.to_lowercase() {
                slug.push(lower);
            }
        }
    }
    slug
}

/// `text` without its HTML comments.
fn without_html_comments(text: &str) -> String {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        kept.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("-->") else {
            return kept;
        };
        rest = &rest[start + end + 3..];
    }
    kept.push_str(rest);
    kept
}

/// `text` with the `(target)` part of each link removed.
fn without_link_targets(text: &str) -> String {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        kept.push_str(&rest[..start + 1]);
        let Some(end) = rest[start..].find(')') else {
            return kept;
        };
        rest = &rest[start + end + 1..];
    }
    kept.push_str(rest);
    kept
}

/// The `id` attributes of an HTML page.
pub fn html_ids(html: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut rest = html;
    while let Some(position) = rest.find("id=\"") {
        let after_space = match rest[..position].chars().next_back() {
            Some(c) => c.is_whitespace(),
            None => false,
        };
        rest = &rest[position + 4..];
        let Some(end) = rest.find('"') else {
            break;
        };
        if after_space {
            ids.insert(rest[..end].to_owned());
        }
        rest = &rest[end + 1..];
    }
    ids
}

/// What `target` points at.
pub fn classify(target: &str) -> Target<'_> {
    if let Some(rest) = target.strip_prefix(PAGES) {
        let (path, anchor) = split_anchor(rest);
        return Target::Api { path, anchor };
    }
    for prefix in REPOSITORY {
        if let Some(rest) = target.strip_prefix(prefix) {
            let (path, anchor) = split_anchor(rest);
            return Target::Repository { path, anchor };
        }
    }
    if has_scheme(target) {
        return Target::Other;
    }
    let (path, anchor) = split_anchor(target);
    Target::Relative { path, anchor }
}

/// Splits `target` at its first `#`.
fn split_anchor(target: &str) -> (&str, Option<&str>) {
    match target.split_once('#') {
        Some((path, anchor)) => (path, Some(anchor)),
        None => (target, None),
    }
}

/// Whether `target` starts with a URI scheme, such as `https:` or `mailto:`.
fn has_scheme(target: &str) -> bool {
    for (index, c) in target.char_indices() {
        if c == ':' {
            return index > 0;
        }
        let allowed = if index == 0 {
            c.is_ascii_alphabetic()
        } else {
            c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.'
        };
        if !allowed {
            return false;
        }
    }
    false
}

/// Why `target`, read from `file` of the given kind, is broken; `None` when
/// it resolves or is not checked. `file` is relative to `root`. API links
/// are checked only when `doc` names the rustdoc directory.
pub fn check(
    root: &Path,
    doc: Option<&Path>,
    file: &str,
    kind: Kind,
    target: &str,
) -> Option<String> {
    match classify(target) {
        Target::Api { path, anchor } => check_api(doc?, path, anchor),
        Target::Repository { path, anchor } => {
            let resolved = normalize(&root.join(path));
            check_file(&resolved, path, anchor, root)
        }
        Target::Relative { path, anchor } => {
            if kind == Kind::Rust {
                return None;
            }
            let document = normalize(&root.join(file));
            if path.is_empty() {
                return check_file(&document, file, anchor, root);
            }
            let directory = match document.parent() {
                Some(directory) => directory.to_path_buf(),
                None => root.to_path_buf(),
            };
            let resolved = normalize(&directory.join(path));
            check_file(&resolved, path, anchor, root)
        }
        Target::Other => None,
    }
}

/// Checks that `resolved` exists, and that a Markdown file holds a heading
/// with `anchor`. `shown` names the file in the reason.
fn check_file(resolved: &Path, shown: &str, anchor: Option<&str>, root: &Path) -> Option<String> {
    if !resolved.starts_with(normalize(root)) || !resolved.exists() {
        return Some(format!("no such file: {shown}"));
    }
    let anchor = anchor?;
    if anchor.is_empty() || !resolved.is_file() {
        return None;
    }
    if resolved.extension() != Some(std::ffi::OsStr::new("md")) {
        return None;
    }
    let text = match std::fs::read_to_string(resolved) {
        Ok(text) => text,
        Err(error) => return Some(format!("cannot read {shown}: {error}")),
    };
    if anchors(&text).contains(anchor) {
        None
    } else {
        Some(format!("no such heading: #{anchor}"))
    }
}

/// Checks that the rustdoc directory `doc` holds the page `path`, and the
/// page an element with the `id` `anchor`.
fn check_api(doc: &Path, path: &str, anchor: Option<&str>) -> Option<String> {
    let mut page = path.to_owned();
    if page.is_empty() || page.ends_with('/') {
        page.push_str("index.html");
    }
    let file = doc.join(&page);
    if !file.is_file() {
        return Some(format!("no such page in the rustdoc output: {page}"));
    }
    let anchor = anchor?;
    let html = match std::fs::read_to_string(file) {
        Ok(html) => html,
        Err(error) => return Some(format!("cannot read {page}: {error}")),
    };
    if html_ids(&html).contains(anchor) {
        None
    } else {
        Some(format!("no such anchor in {page}: {anchor}"))
    }
}

/// `path` with its `.` and `..` components resolved, without reading the
/// file system.
fn normalize(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other.as_os_str()),
        }
    }
    normal
}

/// The files the link check reads, from `root` with `/` separators, sorted:
/// every Markdown file outside `.git` and `target` directories, and every
/// `.rs` file under `src/`.
pub fn files(root: &Path) -> std::io::Result<Vec<(String, Kind)>> {
    let mut files = Vec::new();
    collect(root, "", &mut files)?;
    files.sort();
    Ok(files)
}

/// Adds the checked files under `root/directory` to `files`.
fn collect(root: &Path, directory: &str, files: &mut Vec<(String, Kind)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root.join(directory))? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let relative = if directory.is_empty() {
            name.clone()
        } else {
            format!("{directory}/{name}")
        };
        if entry.file_type()?.is_dir() {
            if !SKIPPED.contains(&name.as_str()) {
                collect(root, &relative, files)?;
            }
        } else if name.ends_with(".md") {
            files.push((relative, Kind::Markdown));
        } else if name.ends_with(".rs") && relative.starts_with("src/") {
            files.push((relative, Kind::Rust));
        }
    }
    Ok(())
}

/// Checks every link of the repository at `root`. API links are checked
/// only when `doc` names the rustdoc directory.
pub fn check_repository(root: &Path, doc: Option<&Path>) -> std::io::Result<Report> {
    let mut report = Report::default();
    for (file, kind) in files(root)? {
        let text = std::fs::read_to_string(root.join(&file))?;
        for link in links(&text, kind) {
            report.checked += 1;
            if let Some(reason) = check(root, doc, &file, kind, &link.target) {
                report.problems.push(Problem {
                    file: file.clone(),
                    line: link.line,
                    target: link.target,
                    reason,
                });
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page of DOC-012 that holds the crate's headings.
    const INDEX: &str = r#"<h2 id="performance-characteristics">Performance</h2>"#;

    /// The page of DOC-012 that holds a method, and an attribute that ends in
    /// `id` but is not one.
    const HANDLER: &str = r#"<section id="method.connect"></section><a data-id="fake"></a>"#;

    /// A directory of files for one test, removed when dropped.
    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let directory = format!("rust-milter-links-{}-{name}", std::process::id());
            let root = std::env::temp_dir().join(directory);
            let _ = std::fs::remove_dir_all(&root);
            Fixture { root }
        }

        fn write(&self, path: &str, text: &str) {
            let file = self.root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn link(line: usize, target: &str) -> Link {
        Link {
            line,
            target: target.to_owned(),
        }
    }

    #[test]
    fn doc_007_markdown_links_outside_code_are_read() {
        let text = [
            "Intro [one](a.md) and ![image](img.png).",
            "[label]: b.md#part",
            "Code `[no](c.md)` here.",
            "```text",
            "[no](d.md)",
            "```",
            "- item",
            "  ```sh",
            "  [no](e.md)",
            "  ```",
            "[two](f.md \"title\")",
        ]
        .join("\n");

        let found = links(&text, Kind::Markdown);

        let expected = [
            link(1, "a.md"),
            link(1, "img.png"),
            link(2, "b.md#part"),
            link(11, "f.md"),
        ];
        assert_eq!(found, expected);
    }

    #[test]
    fn doc_008_rust_doc_comment_links_are_read() {
        let blob = REPOSITORY[0];
        let source = [
            format!("//! Crate [docs]({blob}README.md)."),
            String::new(),
            "/// See [the guide].".to_owned(),
            "///".to_owned(),
            format!("    /// [the guide]: {blob}docs/how-to/run-the-tests.md"),
            format!("// Not [docs]({blob}missing.md)."),
            format!("//// Not [docs]({blob}missing.md)."),
            "fn f() {}".to_owned(),
        ]
        .join("\n");

        let found = links(&source, Kind::Rust);

        let readme = format!("{blob}README.md");
        let guide = format!("{blob}docs/how-to/run-the-tests.md");
        assert_eq!(found, [link(1, &readme), link(5, &guide)]);
    }

    #[test]
    fn doc_009_heading_anchors_follow_github() {
        let text = [
            "# Milter protocol specification <!-- omit from toc -->",
            "## Action flags (SMFIF_*)",
            "### `ServerConfig` reference",
            "## See [the guide](x.md) now",
            "## Repeat",
            "## Repeat",
            "```rust",
            "#[derive(Default)]",
            "# hidden",
            "```",
            "## Version 6: what's new?",
        ]
        .join("\n");

        let mut found: Vec<String> = anchors(&text).into_iter().collect();
        found.sort();

        let expected = [
            "action-flags-smfif_",
            "milter-protocol-specification",
            "repeat",
            "repeat-1",
            "see-the-guide-now",
            "serverconfig-reference",
            "version-6-whats-new",
        ];
        assert_eq!(found, expected);
    }

    #[test]
    fn doc_010_relative_links_name_files_and_headings() {
        let fixture = Fixture::new("doc-010");
        fixture.write("docs/a.md", "# Title\n\n## Part one\n");
        fixture.write("docs/sub/b.md", "# B\n");
        fixture.write("docs/dir/x.txt", "x\n");
        let root = fixture.root.as_path();
        let from = "docs/sub/b.md";
        let markdown = Kind::Markdown;

        let file = check(root, None, from, markdown, "../a.md");
        let missing = check(root, None, from, markdown, "../missing.md");
        let directory = check(root, None, from, markdown, "../dir");
        let heading = check(root, None, from, markdown, "../a.md#part-one");
        let no_heading = check(root, None, from, markdown, "../a.md#nope");
        let own = check(root, None, from, markdown, "#b");

        assert_eq!(file, None);
        assert_eq!(missing, Some("no such file: ../missing.md".to_owned()));
        assert_eq!(directory, None);
        assert_eq!(heading, None);
        assert_eq!(no_heading, Some("no such heading: #nope".to_owned()));
        assert_eq!(own, None);
    }

    #[test]
    fn doc_011_repository_links_name_files_and_headings() {
        let fixture = Fixture::new("doc-011");
        fixture.write("docs/a.md", "# Title\n\n## Part one\n");
        fixture.write("docs/dir/x.txt", "x\n");
        let root = fixture.root.as_path();
        let heading = format!("{}docs/a.md#part-one", REPOSITORY[0]);
        let directory = format!("{}docs/dir", REPOSITORY[1]);
        let missing = format!("{}docs/gone.md", REPOSITORY[0]);
        let no_heading = format!("{}docs/a.md#gone", REPOSITORY[0]);

        for kind in [Kind::Markdown, Kind::Rust] {
            let found = check(root, None, "README.md", kind, &heading);
            let tree = check(root, None, "README.md", kind, &directory);
            let gone = check(root, None, "README.md", kind, &missing);
            let no_id = check(root, None, "README.md", kind, &no_heading);

            assert_eq!(found, None);
            assert_eq!(tree, None);
            assert_eq!(gone, Some("no such file: docs/gone.md".to_owned()));
            assert_eq!(no_id, Some("no such heading: #gone".to_owned()));
        }
    }

    #[test]
    fn doc_012_api_links_name_pages_and_anchors() {
        let fixture = Fixture::new("doc-012");
        fixture.write("doc/rust_milter/index.html", INDEX);
        fixture.write("doc/rust_milter/trait.Handler.html", HANDLER);
        let root = fixture.root.as_path();
        let doc = fixture.root.join("doc");
        let doc = Some(doc.as_path());
        let page = format!("{PAGES}rust_milter/trait.Handler.html#method.connect");
        let directory = format!("{PAGES}rust_milter/");
        let missing = format!("{PAGES}rust_milter/struct.Gone.html");
        let no_anchor = format!("{PAGES}rust_milter/trait.Handler.html#fake");
        let markdown = Kind::Markdown;

        let found = check(root, doc, "README.md", markdown, &page);
        let index = check(root, doc, "README.md", markdown, &directory);
        let no_page = check(root, doc, "README.md", markdown, &missing);
        let no_id = check(root, doc, "README.md", markdown, &no_anchor);
        let unchecked = check(root, None, "README.md", markdown, &missing);

        let gone = "no such page in the rustdoc output: rust_milter/struct.Gone.html";
        let fake = "no such anchor in rust_milter/trait.Handler.html: fake";
        assert_eq!(found, None);
        assert_eq!(index, None);
        assert_eq!(no_page, Some(gone.to_owned()));
        assert_eq!(no_id, Some(fake.to_owned()));
        assert_eq!(unchecked, None);
    }

    #[test]
    fn doc_013_other_links_are_not_checked() {
        let root = repository_root();
        let root = root.as_path();
        let markdown = Kind::Markdown;
        let rust = Kind::Rust;
        let site = "https://doc.rust-lang.org/rustdoc/";
        let mail = "mailto:test@void.invalid";
        let intra_doc = "Handler#protocol-engine-behavior";

        for target in [site, mail] {
            assert_eq!(check(root, None, "README.md", markdown, target), None);
        }
        for target in [intra_doc, "crate::Error", "missing.md"] {
            assert_eq!(check(root, None, "src/lib.rs", rust, target), None);
        }
    }

    #[test]
    fn doc_014_every_relative_and_repository_link_resolves() {
        let report = check_repository(&repository_root(), None).unwrap();
        let mut problems = Vec::new();
        for problem in &report.problems {
            problems.push(problem.to_string());
        }

        assert!(report.checked > 0);
        assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
    }
}
