# Test scenarios: Documentation

Scenarios that keep the example code in the tutorials and how-to guides compiling, and the links between the documents and the API reference working. Conventions are in [README.md](README.md).

## Example code

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| DOC-001 | doctest | Every Rust code block in `docs/tutorials/` and `docs/how-to/` | `cargo test` runs in CI | every block compiles; a block that does not fails the run | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-002 | unit | The Markdown files in `docs/tutorials/` and `docs/how-to/` | `cargo test` runs | every file is included in the documentation check; a file that is not makes the test fail | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-003 | unit | A setup comment with lines above and below its `---` line, right before a Rust block | the build script prepares the document | the block holds the lines above `---`, then its own lines, then the lines below `---`; the comment is gone | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-004 | unit | A setup comment followed by text, by a block in another language, or by the end of the file | the build script prepares the document | preparing fails and names the line of the comment | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-005 | unit | A code block without a language | the build script prepares the document | preparing fails and names the line of the block | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-006 | unit | A Rust block, and a Rust block marked `ignore` | the build script prepares the document | the first is marked `no_run`, so it is compiled and not run; the second stays ignored | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |

## Link check

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| DOC-007 | unit | A Markdown text with an inline link, an image, a reference definition, a link inside an inline code span, and a link inside a fenced code block, indented or not | the links are read | the targets of the inline link, the image, and the definition are found with their line numbers; the two in code are not | [test environment: Link check](../reference/test-environment.md#link-check) |
| DOC-008 | unit | Rust source with an inline link and a reference definition in `///` and `//!` comments, and a link in a plain `//` comment | the links are read | the two doc comment targets are found with their line numbers; the plain comment gives none | [test environment: Link check](../reference/test-environment.md#link-check) |
| DOC-009 | unit | Headings with backticks, a link, punctuation, an underscore, an HTML comment, and a repeated heading, and a `#` line inside a fenced code block | the heading anchors are computed | each anchor is GitHub's, such as `action-flags-smfif_`; the repeated heading gets `-1`; the line in code gives none | [test environment: Link check](../reference/test-environment.md#link-check) |
| DOC-010 | unit | Relative links from a Markdown document to an existing file, a missing file, a directory, a heading of another document, a missing heading, and a heading of the document itself | the links are checked | only the missing file and the missing heading are reported, each with its reason | [test environment: Link check](../reference/test-environment.md#link-check) |
| DOC-011 | unit | Repository links with `blob/main` and `tree/main` to existing and missing files and headings, from Markdown and from Rust source | the links are checked | only the missing file and the missing heading are reported | [test environment: Link check](../reference/test-environment.md#link-check) |
| DOC-012 | unit | API links to an existing page and anchor, a page by its directory, a missing page, and a missing anchor, with a rustdoc directory | the links are checked | only the missing page and the missing anchor are reported; without a rustdoc directory, none is reported | [test environment: Link check](../reference/test-environment.md#link-check) |
| DOC-013 | unit | A link to another site, a `mailto:` link, and in Rust source an intra-doc link with an anchor | the links are checked | none is reported | [test environment: Link check](../reference/test-environment.md#link-check) |
| DOC-014 | integration | The Markdown files and the doc comments of the repository | `cargo test` runs | every relative link and repository link resolves | [test environment: Link check](../reference/test-environment.md#link-check) |
| DOC-015 | integration | The rustdoc that the Rustdoc job builds | the job runs `check-links` | every link of the repository resolves, API links included; a broken link fails the job | [test environment: Link check](../reference/test-environment.md#link-check) |
