# Architecture decision records

Decisions that shape rust-milter's design and specification, with their context and consequences. Decisions about testing, such as test environments and CI, are in [docs/testing/adr/](../testing/adr/). These records sit outside the Diátaxis structure of the rest of `docs/`: each one is a dated record of a decision, kept as it was made.

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) | Commands in a wrong state recover like libmilter, or close the connection | Accepted |
| [0002](0002-library-runs-the-protocol-handlers-decide.md) | The library runs the Milter protocol; handlers make decisions | Accepted |
| [0003](0003-never-record-the-smtp-client-address.md) | rust-milter never records the SMTP client's address | Accepted |
| [0004](0004-metrics-come-from-opentelemetry-and-status.md) | Metrics come from OpenTelemetry and status(), with no collector trait | Accepted |
| [0005](0005-reload-closes-no-connection-and-rebinds-no-listener.md) | A reload closes no open connection and rebinds no listener | Accepted; the `backlog` decision is superseded by ADR 0007 |
| [0006](0006-connection-timeouts-per-frame-and-libmilter-idle-default.md) | Connection timeouts are split per frame, and the idle default follows libmilter | Accepted |
| [0007](0007-a-reload-that-changes-a-restart-setting-fails.md) | A reload that changes a setting that requires a restart fails | Accepted |
| [0008](0008-rust-code-is-written-in-a-procedural-style.md) | Rust code is written in a procedural style | Accepted; the `Factory` and `Loader` exception and the allowance of function pointers are superseded by ADR 0009, the `reload_with` exceptions by ADR 0010, and the end of the transition by ADR 0011 |
| [0009](0009-no-stored-closures-or-function-pointers.md) | No stored closures or function pointers | Accepted; its list of exceptions is extended by ADR 0010 |
| [0010](0010-builder-factory-type-parameter-and-loader-trait.md) | The builder keeps the handler factory in a type parameter and takes a loader trait | Accepted |
| [0011](0011-the-disallowed-methods-run-stays-for-derive-code.md) | The disallowed-methods run stays for derive code | Accepted |

Conventions:
- One file per decision, named `NNNN-short-title.md`, numbered in order.
- Sections: status and date, context, options considered, decision, consequences, references.
- Status is `Proposed`, `Accepted`, `Superseded by ADR NNNN`, or `Deprecated`.
- An accepted record is not rewritten. A changed decision gets a new ADR, and the old one is marked superseded.
