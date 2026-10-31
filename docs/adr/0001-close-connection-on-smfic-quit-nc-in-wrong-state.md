# ADR 0001: Close the connection on SMFIC_QUIT_NC in a wrong state

- Status: Accepted
- Date: 2026-10-30
- Affects: server (`Connection`), milter client, handler contract

## Context

`SMFIC_QUIT_NC` (command `'K'`, protocol version 6) ends an SMTP session but keeps the milter connection open for the next session, which then starts with `SMFIC_CONNECT` without a new option negotiation.

What the sources show:

- libmilter (Sendmail 8.19.0.2, `libmilter/engine.c`) accepts `SMFIC_QUIT_NC` only in two states: `ST_ENDM`, after end of message, and `ST_UNKN`, after an unknown command.
- libmilter leaves those states on common replies. `SMFIS_ACCEPT` to any command resets the state to `ST_HELO`. `SMFIS_REJECT`, `SMFIS_DISCARD`, and `SMFIS_TEMPFAIL` do the same, except after `SMFIC_RCPT` and `SMFIC_UNKNOWN`.
- From `ST_HELO`, libmilter treats `SMFIC_QUIT_NC` as an invalid transition and ignores it without a reply. It then also ignores the following `SMFIC_CONNECT`. The MTA waits for a reply until its timeout expires.
- Neither Sendmail 8.19 nor Postfix 3.12 sends `SMFIC_QUIT_NC`. Both open one milter connection per SMTP session.
- The rust-milter milter client (`docs/reference/milter-client.md`) is designed to reuse connections with `SMFIC_QUIT_NC`.
- rust-milter handlers drive the protocol frame by frame. Any state rule left to handlers has to be implemented correctly in every application.

The question: what does the rust-milter server do when `SMFIC_QUIT_NC` arrives in a state where libmilter does not accept it?

## Options considered

### (a) Copy libmilter: ignore the command

- Matches libmilter exactly.
- Reproduces the hang: the MTA's next `SMFIC_CONNECT` also goes unanswered until a timeout.

### (b) Treat it as a protocol error and close the connection

- `SMFIC_QUIT_NC` stays valid exactly where libmilter accepts it.
- Elsewhere the connection closes at once, so the MTA reconnects instead of waiting for a timeout.
- Handlers meet `SMFIC_QUIT_NC` only in valid states.

### (c) Accept it in any state after option negotiation, and reset the session

- Allows the most reuse.
- Every handler must drop its per-session state correctly from any point in the conversation, including halfway through a message. This state handling is hard to get right in application code, and a mistake leaks one SMTP session's data into the next.
- Diverges from libmilter, so a client could not use one rule for libmilter and rust-milter servers.

## Decision

Option (b).

- `SMFIC_QUIT_NC` is accepted in `ST_ENDM` and `ST_UNKN`, as libmilter defines them.
- In any other state it is a protocol error, and the connection closes.
- The library enforces the rule, not the handler. `Connection` tracks the protocol state from the command codes it reads and the reply codes it writes, with libmilter's transition rules. On `SMFIC_QUIT_NC` in a wrong state, `read_frame` returns `Error::Protocol` and the connection closes; the handler never receives that command.
- In a valid state, the command reaches the handler, which must drop all per-session state and keep only the negotiated options (session isolation).

## Consequences

Positive:
- No hang. An MTA that sends `SMFIC_QUIT_NC` too early gets a closed connection and can reconnect at once.
- Handler authors deal with `SMFIC_QUIT_NC` only where it is valid, and do not have to implement libmilter's state table.
- The milter client uses one reuse rule for libmilter and rust-milter servers. The `any_state` reuse policy was dropped; `reuse_policy` is `libmilter_compatible` (default) or `never`.

Negative:
- Reuse stays limited. A connection is reusable only when the milter ended the last message with `SMFIR_CONTINUE`, or after an unknown command. A milter that ends messages with `SMFIR_ACCEPT` gets a new connection per session.
- `Connection` must decode the command code of every frame it reads and the reply code of every frame it writes, which ties the transport component to protocol knowledge.
- An MTA that relies on rust-milter accepting `SMFIC_QUIT_NC` in more states than libmilter does will see closed connections.

Follow-up work:
- Test `SMFIC_QUIT_NC` in every protocol state, after every reply that resets the state, and check the close reason (`error`, with `error.type` `Protocol`) in telemetry.
- Keep the client's state mirror and the server's state tracking on one shared implementation of libmilter's transition rules.

## References

- [milter-protocol.md, rust-milter protocol support](../reference/milter-protocol.md#rust-milter-protocol-support)
- [milter-protocol.md, protocol states](../reference/milter-protocol.md#protocol-states)
- [api-specification.md, Connection](../reference/api-specification.md#connection)
- [milter-client.md, reuse rules](../reference/milter-client.md#reuse-rules)
- [Connection reuse and session isolation](../explanation/connection-reuse.md)
- Sendmail 8.19.0.2 source: `libmilter/engine.c` (`next_states`, `trans_ok`, `mi_engine`)
- Postfix 3.12 source: `src/milter/milter8.c`
