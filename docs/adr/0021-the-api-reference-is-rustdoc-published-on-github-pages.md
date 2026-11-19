# ADR 0021: The API reference is rustdoc, published on GitHub Pages

- Status: Accepted
- Date: 2026-11-18
- Affects: the API reference, the doc comments of the `rust-milter` crate, CI, GitHub Pages, `docs/testing/`, and every document that links to the API reference

## Context

The API reference was `docs/reference/api-specification.md`, a Markdown page of tables: types, methods, fields, variants, and the contract of each. The crate's doc comments described the same items again in shorter words, so the API was written down twice, and nothing kept the two copies in step. The page named methods with their arguments in prose, such as `config(ServerConfig)`, and gave a return type only for some rows. Its thread safety table stated by hand what the compiler already knows.

rustdoc builds a reference from the crate: each item with its signature, its fields or variants, its trait implementations, and the auto traits such as `Send` and `Sync`. CI already runs `cargo doc` with `-D warnings`, which fails on a broken link between items.

rustdoc's output isn't stored in the repository, so a scenario in `docs/testing/` can't reach it with a relative link. rust-milter isn't on crates.io yet, so docs.rs doesn't build it either. The repository is public, and GitHub Pages can host a static site built by a workflow.

## Options considered

- (a) Keep the Markdown page and the doc comments as two copies.
- (b) Move the page into the doc comments, publish the rustdoc on GitHub Pages, and delete the page.
- (c) Move the page into the doc comments, delete the page, and publish nothing: readers run `cargo doc --open`.

## Decision

Option (b).

- The API reference is the rustdoc of the `rust-milter` crate. Its doc comments hold what `api-specification.md` held. Each item carries its own contract. The protocol engine rules sit on the `Handler` trait, because they say when the library calls each callback. The rules that cover the whole crate sit on the crate's root page: the Cargo features, versioning and stability, performance, and usage constraints.
- `docs/reference/api-specification.md` goes away. The other reference documents stay in Markdown, because they specify behavior, settings, wire formats, and logs, not items of the crate. `configuration.md` stays the reference for each setting, since it also covers the file format and the environment variables, and the doc comments of `ServerConfig` link to it.
- A doc comment links to a Markdown document by its GitHub URL on `main`, such as `https://github.com/photom/rust-milter/blob/main/docs/reference/configuration.md#validation-rules`.
- Doc comments hold no example code. Examples stay in the tutorials and how-to guides, which the doc comments link to.
- A workflow builds the rustdoc of the library from `main`, with the `otel` and `serde` features, and publishes it on GitHub Pages at `https://photom.github.io/rust-milter/`. The root page redirects to the crate's page. A pull request builds the site and checks its links without deploying it.
- A document links to an API item by its Pages URL and rustdoc anchor, such as `https://photom.github.io/rust-milter/rust_milter/trait.Handler.html#method.connect`. The Spec column of the test scenarios does the same.
- CI checks the links. Each Pages link in a Markdown document must name a page and an anchor in the rustdoc that the same run builds. Each relative link in a Markdown document must name a file, and a heading of it when it has an anchor. Each GitHub link to a document of the repository, in a Markdown document or in a doc comment under `src/`, must do the same.

Option (a) keeps the two copies. Option (c) leaves the scenarios with no link target, and a reader without a checkout with no reference.

## Consequences

Positive:
- The signatures, fields, and variants are written once, and the compiler checks them.
- The auto traits and trait implementations come from the code, not from a table.
- A broken link between items, from a document to an item, or from an item to a document fails CI.

Negative:
- The site shows `main`, so a Pages link in an older commit points at the current API, and can break once an item is renamed.
- Renaming an item changes its anchor, so the documents that link to it change in the same pull request.
- Tables in doc comments read less well in the source than in the HTML.
- A table row can't be linked to on its own. A scenario links to the item or to the heading of the table.

## References

- [ADR 0002](0002-library-runs-the-protocol-handlers-decide.md), the callback model that the `Handler` page describes
- [test-environment.md: Link check](../reference/test-environment.md#link-check)
- [The rustdoc book](https://doc.rust-lang.org/rustdoc/)
- [GitHub Pages: publishing with a custom GitHub Actions workflow](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages)
