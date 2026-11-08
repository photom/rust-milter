# Testing decision records

Decisions about how rust-milter is tested: test environments, CI, and test tooling. Decisions about the product's design and specification are in [docs/adr/](../../adr/). Like the test scenarios, these records sit outside the Diátaxis structure of the rest of `docs/`.

| Testing ADR | Title | Status |
|-------------|-------|--------|
| [0001](0001-interop-tests-run-on-distro-mtas-in-compose-on-every-pr.md) | Interoperability tests run against distribution MTAs, in Docker Compose, on every pull request | Accepted |
| [0002](0002-ci-scans-with-cargo-deny-clippy-and-codeql.md) | CI scans dependencies with cargo-deny, and code with Clippy and CodeQL | Accepted |
| [0003](0003-interop-stack-resolves-names-with-a-local-unbound.md) | The interoperability stack resolves names with a local Unbound | Accepted |
| [0004](0004-test-server-takes-values-and-waits-for-named-states.md) | The test server takes values and waits for named states | Accepted |

Conventions are the same as for the [product ADRs](../../adr/README.md), with their own numbering from 0001. Refer to a record here as "testing ADR NNNN", so it is not confused with a product ADR of the same number.
