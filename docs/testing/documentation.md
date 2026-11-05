# Test scenarios: Documentation

Scenarios that keep the example code in the tutorials and how-to guides compiling. Conventions are in [README.md](README.md).

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| DOC-001 | doctest | Every Rust code block in `docs/tutorials/` and `docs/how-to/` | `cargo test` runs in CI | every block compiles; a block that does not fails the run | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-002 | unit | The Markdown files in `docs/tutorials/` and `docs/how-to/` | `cargo test` runs | every file is included in the documentation check; a file that is not makes the test fail | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-003 | unit | A setup comment with lines above and below its `---` line, right before a Rust block | the build script prepares the document | the block holds the lines above `---`, then its own lines, then the lines below `---`; the comment is gone | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-004 | unit | A setup comment followed by text, by a block in another language, or by the end of the file | the build script prepares the document | preparing fails and names the line of the comment | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-005 | unit | A code block without a language | the build script prepares the document | preparing fails and names the line of the block | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-006 | unit | A Rust block, and a Rust block marked `ignore` | the build script prepares the document | the first is marked `no_run`, so it is compiled and not run; the second stays ignored | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
