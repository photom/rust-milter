# Test scenarios

The test scenarios for rust-milter, written before the code as the Canon TDD test list. Like the [architecture decision records](../adr/), these documents sit outside the Diátaxis structure of the rest of `docs/`. How to run and write tests is in the how-to guides [Run the tests](../how-to/run-the-tests.md) and [Write a network test](../how-to/write-a-network-test.md); the test environment is specified in [test-environment.md](../reference/test-environment.md).

## Files

Ordered as the components will be built:

| File | Area | ID prefix |
|------|------|-----------|
| [frame.md](frame.md) | Frame parsing and encoding | `FRM` |
| [message.md](message.md) | `Command`, `Reply`, and `ReplyCode` | `CMD`, `RPL`, `RCD` |
| [configuration.md](configuration.md) | Configuration validation and the serde file format | `CFG` |
| [protocol-engine.md](protocol-engine.md) | Negotiation, callbacks, decisions, macros, state machine, session isolation | `ENG` |
| [documentation.md](documentation.md) | Example code in the tutorials and how-to guides compiles in CI | `DOC` |

Still to come: the transport and server (`SRV`), shutdown (`SHT`), configuration reload (`RLD`), telemetry (`TEL`), interoperability with Sendmail and Postfix (`INT`), and load and chaos tests. The milter client sample is not part of the product; CI only builds it.

## Format

Each file groups its scenarios into tables with these columns:

| Column | Content |
|--------|---------|
| ID | `<PREFIX>-<NNN>`, unique and never reused. Gaps between numbers leave room for related scenarios |
| Level | `unit`, `integration`, `property` (property-based test), `fuzz`, or `doctest` (code compiled from a document) |
| Given | The starting state |
| When | The single action under test |
| Then | The observable result to check |
| Spec | Link to the specification rule the scenario checks |

## Rules

1. Every MUST rule and every behavior table row in `docs/reference/` has at least one scenario.
2. A test names its scenario ID in its function name or doc comment, so each scenario can be traced to its test.
3. A test is written from its scenario before the code that makes it pass.
4. A case found during implementation is added here first, with a new ID, then tested.
5. When a specification changes, the scenarios that link to it are updated in the same change.
6. A scenario whose expected result depends on an open decision is marked "open" in its Then column and links to the question.
