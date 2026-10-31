# Test scenarios: Documentation

Scenarios that keep the example code in the tutorials and how-to guides compiling. Conventions are in [README.md](README.md).

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| DOC-001 | doctest | Every Rust code block in `docs/tutorials/` and `docs/how-to/` | `cargo test` runs in CI | every block compiles; a block that does not fails the run | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |
| DOC-002 | unit | The Markdown files in `docs/tutorials/` and `docs/how-to/` | `cargo test` runs | every file is included in the documentation check; a file that is not makes the test fail | [test environment: Documentation check](../reference/test-environment.md#documentation-check) |