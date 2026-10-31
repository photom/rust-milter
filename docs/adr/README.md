# Architecture decision records

Decisions that shape rust-milter, with their context and consequences. These records sit outside the Diátaxis structure of the rest of `docs/`: each one is a dated record of a decision, kept as it was made.

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) | Commands in a wrong state recover like libmilter, or close the connection | Accepted |
| [0002](0002-library-runs-the-protocol-handlers-decide.md) | The library runs the Milter protocol; handlers make decisions | Accepted |

Conventions:
- One file per decision, named `NNNN-short-title.md`, numbered in order.
- Sections: status and date, context, options considered, decision, consequences, references.
- Status is `Proposed`, `Accepted`, `Superseded by ADR NNNN`, or `Deprecated`.
- An accepted record is not rewritten. A changed decision gets a new ADR, and the old one is marked superseded.
