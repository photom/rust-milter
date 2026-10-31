# ADR 0001: Commands in a wrong state recover like libmilter, or close the connection

- Status: Accepted
- Date: 2026-10-30 (decision on `SMFIC_QUIT_NC`); extended 2026-10-31 to all other commands
- Affects: protocol engine, milter client, handler contract
- Related: [ADR 0002](0002-library-runs-the-protocol-handlers-decide.md)

## Context

The protocol engine must decide what to do when the MTA sends a command that does not fit the current protocol state. The most important case is `SMFIC_QUIT_NC`; the same question applies to every other command.

### SMFIC_QUIT_NC

`SMFIC_QUIT_NC` (command `'K'`, protocol version 6) ends an SMTP session but keeps the milter connection open for the next session, which then starts with `SMFIC_CONNECT` without a new option negotiation.

- libmilter (Sendmail 8.19.0.2, `libmilter/engine.c`) accepts `SMFIC_QUIT_NC` only in two states: `ST_ENDM`, after end of message, and `ST_UNKN`, after an unknown command.
- libmilter leaves those states on common replies. `SMFIS_ACCEPT` to any command resets the state to `ST_HELO`. `SMFIS_REJECT`, `SMFIS_DISCARD`, and `SMFIS_TEMPFAIL` do the same, except after `SMFIC_RCPT` and `SMFIC_UNKNOWN`.
- From `ST_HELO`, libmilter ignores `SMFIC_QUIT_NC` without a reply, then also ignores the following `SMFIC_CONNECT`. The MTA waits for a reply until its timeout expires.
- Neither Sendmail 8.19 nor Postfix 3.12 sends `SMFIC_QUIT_NC`. Both open one milter connection per SMTP session.
- The rust-milter milter client (`docs/reference/milter-client.md`) is designed to reuse connections with `SMFIC_QUIT_NC`.

### libmilter's rule for every command

`mi_engine` in `libmilter/engine.c` handles every command that does not fit the current state with one rule:

1. If a message is open, it calls `xxfi_abort`.
2. It checks whether the command fits from `ST_HELO`, using its transition table (`trans_ok`), which also skips stages the milter switched off (`fix_stm`).
3. If it fits, libmilter processes the command normally and replies. If it does not, libmilter ignores the command and sends no reply.

From `ST_HELO`, only `SMFIC_HELO`, `SMFIC_MAIL`, and `SMFIC_UNKNOWN` fit, plus commands of later stages the milter switched off. The outcome for the MTA therefore depends on the command:

| Command in a wrong state | libmilter | Effect on the MTA |
|---|---|---|
| `SMFIC_HELO`, `SMFIC_MAIL`, `SMFIC_UNKNOWN` | processed as if the state were `ST_HELO` | normal reply |
| `SMFIC_ABORT`, `SMFIC_QUIT` | ignored | none; neither command gets a reply |
| `SMFIC_QUIT_NC` | ignored | the next `SMFIC_CONNECT` is ignored too; the MTA waits until its timeout |
| every other command, such as `SMFIC_RCPT`, `SMFIC_DATA`, `SMFIC_HEADER`, `SMFIC_EOH`, `SMFIC_BODY`, `SMFIC_BODYEOB`, `SMFIC_CONNECT`, `SMFIC_OPTNEG` | ignored, no reply | the MTA waits for a reply until its timeout |

[ADR 0002](0002-library-runs-the-protocol-handlers-decide.md) decided that rust-milter follows libmilter's division of work. The question is whether it also follows libmilter where libmilter leaves the MTA waiting.

## Options considered

### For SMFIC_QUIT_NC

- (a) Copy libmilter and ignore the command. This reproduces the hang at the next `SMFIC_CONNECT`.
- (b) Treat it as a protocol error and close the connection. `SMFIC_QUIT_NC` stays valid exactly where libmilter accepts it; elsewhere the MTA can reconnect at once.
- (c) Accept it in any state after option negotiation and reset the session. This allows the most reuse, but every handler would have to drop its per-session state correctly from any point in the conversation. That state handling is hard to get right in application code, and a mistake leaks one SMTP session's data into the next. It also diverges from libmilter, so a client could not use one rule for libmilter and rust-milter servers.

### For every other command

- (a) Follow libmilter in every case, including ignoring commands that expect a reply, which leaves the MTA waiting.
- (b) Follow libmilter where it processes a command or ignores one harmlessly, and close the connection where libmilter would leave the MTA waiting.
- (c) Close the connection on every command in a wrong state, without libmilter's recovery.

## Decision

Option (b) in both cases: follow libmilter, except where libmilter would leave the MTA waiting.

| Command in a wrong state | rust-milter |
|---|---|
| `SMFIC_HELO`, `SMFIC_MAIL`, `SMFIC_UNKNOWN`, and any command that fits from `ST_HELO` under libmilter's transition rules | recover like libmilter: if a message is open, call the handler's `abort` and drop the message state; then process the command as if the state were `ST_HELO` |
| `SMFIC_ABORT`, `SMFIC_QUIT` | ignore like libmilter: no callback, no reply, no state change |
| `SMFIC_QUIT_NC` outside `ST_ENDM` and `ST_UNKN` | close the connection with a protocol error |
| every other command | close the connection with a protocol error |

- In `ST_ENDM` and `ST_UNKN`, `SMFIC_QUIT_NC` is valid. The protocol engine ends the session: it calls the handler's `session_end`, drops the session state, and keeps only the negotiated options (session isolation).
- The recovery check uses the same transition rules as libmilter, including the skipping of stages the handler left out of `events()`, so a command recovers in rust-milter exactly when it recovers in libmilter.
- The library's protocol engine enforces all of this. Handlers never see a command that was closed on or ignored.

## Consequences

Positive:
- rust-milter behaves like libmilter wherever libmilter's behavior works: the same commands recover, and the same harmless commands are ignored.
- No command leaves the MTA waiting for a reply. Where libmilter would hang, the connection closes at once, the MTA applies its failure policy, and it can reconnect.
- Handler authors never deal with protocol state, and meet `SMFIC_QUIT_NC` only as a `session_end` with `QuitNewConnection`.
- The milter client uses one reuse rule for libmilter and rust-milter servers. Its reuse policy is `libmilter_compatible` (default) or `never`; there is no `any_state` policy.

Negative:
- Connection reuse stays limited. A connection is reusable only when the milter ended the last message with `SMFIR_CONTINUE`, or after an unknown command. A milter that ends messages with `SMFIR_ACCEPT` gets a new connection per session.
- An MTA that sends commands out of order and relies on libmilter's silence to keep the connection open loses the connection instead. Neither Sendmail nor Postfix does this; both follow the state order.
- An MTA that relies on rust-milter accepting `SMFIC_QUIT_NC` in more states than libmilter does will see closed connections.
- An ignored `SMFIC_QUIT` in a wrong state, for example right after `SMFIC_HELO`, ends the session only when the MTA closes the connection, so the handler's `session_end` reports `ConnectionClosed`, not `Quit`. libmilter behaves the same way: it calls `xxfi_close` at end of file.

Follow-up work:
- Test every command in every protocol state against the decision table, with and without stages switched off in `events()`, and check the close reason (`error`, with `error.type` `Protocol`) in telemetry.
- Share one implementation of libmilter's transition rules between the protocol engine and the milter client's state mirror.

## References

- [api-specification.md, protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior)
- [milter-protocol.md, rust-milter protocol support](../reference/milter-protocol.md#rust-milter-protocol-support)
- [milter-protocol.md, protocol states](../reference/milter-protocol.md#protocol-states)
- [milter-client.md, reuse rules](../reference/milter-client.md#reuse-rules)
- [Connection reuse and session isolation](../explanation/connection-reuse.md)
- [Library architecture, state management](../explanation/architecture.md#state-management)
- Sendmail 8.19.0.2 source: `libmilter/engine.c` (`mi_engine`, `next_states`, `trans_ok`, `fix_stm`)
- Postfix 3.12 source: `src/milter/milter8.c`
