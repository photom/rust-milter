# Architecture decision records

Decisions that shape rust-milter's design and specification, with their context and consequences. Decisions about testing, such as test environments and CI, are in [docs/testing/adr/](../testing/adr/). These records sit outside the Diátaxis structure of the rest of `docs/`: each one is a dated record of a decision, kept as it was made.

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) | Commands in a wrong state recover like libmilter, or close the connection | Superseded by ADR 0015 |
| [0002](0002-library-runs-the-protocol-handlers-decide.md) | The library runs the Milter protocol; handlers make decisions | Accepted |
| [0003](0003-never-record-the-smtp-client-address.md) | rust-milter never records the SMTP client's address | Accepted |
| [0004](0004-metrics-come-from-opentelemetry-and-status.md) | Metrics come from OpenTelemetry and status(), with no collector trait | Accepted |
| [0005](0005-reload-closes-no-connection-and-rebinds-no-listener.md) | A reload closes no open connection and rebinds no listener | Superseded by ADR 0016 |
| [0006](0006-connection-timeouts-per-frame-and-libmilter-idle-default.md) | Connection timeouts are split per frame, and the idle default follows libmilter | Accepted |
| [0007](0007-a-reload-that-changes-a-restart-setting-fails.md) | A reload that changes a setting that requires a restart fails | Superseded by ADR 0016 |
| [0008](0008-rust-code-is-written-in-a-procedural-style.md) | Rust code is written in a procedural style | Superseded by ADR 0017 |
| [0009](0009-no-stored-closures-or-function-pointers.md) | No stored closures or function pointers | Superseded by ADR 0017 |
| [0010](0010-builder-factory-type-parameter-and-loader-trait.md) | The builder keeps the handler factory in a type parameter and takes a loader trait | Superseded by ADR 0017 |
| [0011](0011-the-disallowed-methods-run-stays-for-derive-code.md) | The disallowed-methods run stays for derive code | Superseded by ADR 0017 |
| [0012](0012-no-file-descriptor-limit-setting.md) | rust-milter has no file descriptor limit setting | Accepted |
| [0013](0013-the-first-packet-is-limited-to-1-kb.md) | The first packet of a connection is limited to 1 KB | Accepted |
| [0014](0014-stored-macros-count-against-the-connections-memory.md) | Stored macros count against the connection's memory | Superseded by ADR 0019 |
| [0015](0015-commands-in-wrong-state-follow-libmilter-or-close.md) | Commands in a wrong state follow libmilter, or close the connection | Accepted |
| [0016](0016-a-reload-closes-no-connection-and-fails-on-a-restart-setting.md) | A reload closes no open connection, and fails when it changes a setting that requires a restart | Accepted |
| [0017](0017-rust-code-is-written-in-a-procedural-style.md) | Rust code is written in a procedural style | Accepted |
| [0018](0018-panics-in-handler-code-and-the-reload-loader-are-caught.md) | Panics in handler code and the reload loader are caught | Accepted |
| [0019](0019-a-connection-counts-what-it-holds-until-each-command-ends.md) | A connection counts what it holds until each command ends | Accepted |
| [0020](0020-protocol-options-are-a-fixed-handler-method-and-close-when-missing.md) | Handlers request protocol options with a fixed method, and a missing option closes the connection | Accepted |

Conventions:
- One file per decision, named `NNNN-short-title.md`, numbered in order.
- Sections: status and date, context, options considered, decision, consequences, references.
- Status is `Proposed`, `Accepted`, `Superseded by ADR NNNN`, or `Deprecated`.
- An accepted record is not rewritten; only its status line changes.
- When a decision turns out wrong or changes, a new ADR restates the whole decision, so that it can be read alone. The old record's status becomes `Superseded by ADR NNNN`, and its body stays as it was.
- Specifications and other documents link only to records in force, and describe current behavior without its history.
