# ADR 0002: The library runs the Milter protocol; handlers make decisions

- Status: Accepted
- Date: 2026-10-31
- Affects: handler API, server, documentation of the handler contract

## Context

A rust-milter handler can be designed in two ways:

- (A) The handler receives raw protocol frames on its connection and implements the protocol itself: option negotiation, state checks, which commands get a reply, which replies each stage allows, encoding replies, and clearing state at the end of messages and sessions. The library provides only transport (sockets, timeouts, frame parsing) and the `SMFIC_QUIT_NC` check from [ADR 0001](0001-commands-in-wrong-state-recover-like-libmilter-or-close.md).
- (B) The library runs the protocol conversation, and the handler implements one callback per event (connect, HELO, MAIL, RCPT, header, body, end of message, and so on), each returning a decision.

Sendmail's libmilter, the reference implementation of the Milter protocol, uses model B. The Sendmail 8.19.0.2 source shows:

- `mi_engine` (`libmilter/engine.c`) reads every command from the MTA and checks it against libmilter's state table (`next_states`, `trans_ok`). The application never sees raw frames.
- The application registers callbacks (`xxfi_connect`, `xxfi_helo`, `xxfi_envfrom`, `xxfi_envrcpt`, `xxfi_data`, `xxfi_header`, `xxfi_eoh`, `xxfi_body`, `xxfi_eom`, `xxfi_abort`, `xxfi_close`, `xxfi_unknown`, `xxfi_negotiate`). Each returns a status code such as `SMFIS_CONTINUE`.
- libmilter performs option negotiation (`st_optionneg`). It derives the protocol flags from the callbacks the application registered: no `xxfi_header` means `SMFIP_NOHDRS` (`libmilter/listener.c`).
- `sendreply` turns the status code into the protocol reply, and leaves out replies that negotiated no-reply flags forbid.
- Message modifications (`smfi_addheader`, `smfi_chgheader`, and the others) go through `mi_sendok`, which refuses them unless the state is end of message and the action was negotiated.
- Reply codes are set with `smfi_setreply` and `smfi_setmlreply`, which check their format.

Milter developers know this model from libmilter, and existing milters are written against it.

## Decision

Model B, following libmilter's division of work.

- The library runs the protocol: option negotiation, the protocol state machine, macro storage, reply encoding, the no-reply rules, and the stage rules for replies and modifications.
- The handler implements callbacks that correspond to libmilter's `xxfi_*` callbacks and return decisions corresponding to libmilter's `SMFIS_*` codes.
- The protocol flags requested from the MTA follow from the events the handler subscribes to, as libmilter derives them from the registered callbacks.
- Modifications are available only at end of message, as libmilter's `mi_sendok` requires.

Raw frames are not part of the handler API. A lower-level frame API for applications would need a new ADR. The protocol types (`Frame`, `Command`, `Reply`) stay public for the milter client and for tests.

## Consequences

Positive:
- Developers who know libmilter find the same model: one callback per event, a decision as the return value, modifications at end of message.
- The protocol is implemented once, in the library, and tested there for every application.
- Handlers cannot send a reply that is invalid for the current stage, forget a no-reply rule, or skip negotiation.

Beyond libmilter, the Rust design closes gaps that libmilter leaves to the application:
- Per-session and per-message state are types the handler declares. The library creates a fresh value at each `SMFIC_CONNECT` and each `SMFIC_MAIL`, and drops it when the session or message ends. libmilter instead stores one private pointer (`smfi_setpriv`) that the application must free itself, and only logs a warning when it does not.
- Decision types differ per stage, so a stage-invalid decision such as `Discard` at connect does not compile.
- Modifications are methods of the end-of-message context, so calling them at another stage does not compile. libmilter checks this at run time.
- The library escapes `%` in reply text, so handler authors write the text as it should appear.

Negative:
- Applications that need raw protocol access, such as protocol experiments or transparent proxies, cannot get it from the handler API.
- The library takes on more code: the full protocol engine, not only transport.

Effect on earlier decisions:
- [ADR 0001](0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) still applies. The `SMFIC_QUIT_NC` rule is now enforced by the library's protocol engine, which owns the protocol state.

## References

- [api-specification.md, Handler trait](../reference/api-specification.md#handler-trait)
- [Library architecture](../explanation/architecture.md)
- [milter-protocol.md](../reference/milter-protocol.md)
- Sendmail 8.19.0.2 source: `libmilter/engine.c` (`mi_engine`, `st_optionneg`, `sendreply`, `mi_sendok`), `libmilter/listener.c`, `libmilter/smfi.c`, `include/libmilter/mfapi.h`, `libmilter/docs/api.html`
