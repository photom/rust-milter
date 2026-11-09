# ADR 0015: SMFIC_QUIT ends the session in any state

- Status: Accepted
- Date: 2026-11-09
- Affects: protocol engine, handler contract (`SessionEnd::Quit`), the API and protocol references
- Supersedes: the `SMFIC_QUIT` row of [ADR 0001](0001-commands-in-wrong-state-recover-like-libmilter-or-close.md)

## Context

ADR 0001 decided that rust-milter ignores `SMFIC_QUIT` in a wrong state "like libmilter", and that libmilter calls `xxfi_close` only at end of file. libmilter's transition table accepts `SMFIC_QUIT` only from `ST_ENDM` and `ST_UNKN`, so rust-milter ignored it everywhere else.

That reading of libmilter is wrong. In `mi_engine` (Sendmail 8.19.0.2, `libmilter/engine.c`), a command that does not fit from `ST_HELO` takes a `continue` in the `do { ... } while (!bitset(CT_END, cmds[i].cm_todo))` loop. `SMFIC_QUIT` carries `CT_END`, so the loop ends. libmilter then calls `xxfi_abort` if a message was open, calls `xxfi_close`, and closes the connection. It does this in every state, even before option negotiation.

The difference shows in common flows. Sendmail and Postfix send `SMFIC_QUIT` when an SMTP client leaves after HELO without mail, and after a message the milter accepted, rejected, tempfailed, or discarded: those replies reset the state to `ST_HELO`. rust-milter ignored the `SMFIC_QUIT` in each case. The session stayed open until the MTA closed the socket, and `session_end` reported `ConnectionClosed` instead of `Quit`. Which of the two a handler saw depended on the reply to the last message, and the interoperability test INT-003 had to accept both.

## Options considered

- (a) Keep ignoring `SMFIC_QUIT` in a wrong state.
- (b) Follow libmilter: `SMFIC_QUIT` ends the session in any state.
- (c) Treat `SMFIC_QUIT` in a wrong state as a protocol error and close the connection with a WARN log.

## Decision

Option (b). `SMFIC_QUIT` is valid in every state:

- With a session open, the protocol engine calls `abort` if a message is open, then `session_end` with `SessionEnd::Quit`, sends no reply, and closes the connection.
- With no session open, before option negotiation, after it but before the first session command, or after `SMFIC_QUIT_NC`, it closes the connection without a callback.
- The close reason is `client_closed`.

The rest of ADR 0001 stands, including the `SMFIC_ABORT` row and every rule for `SMFIC_QUIT_NC`.

Option (c) would log a warning for flows that Sendmail and Postfix produce every day.

## Consequences

Positive:
- A handler sees `SessionEnd::Quit` whenever the MTA sent `SMFIC_QUIT`, whatever the reply to the last message was.
- The session and its state are dropped when the MTA quits, not when it gets round to closing the socket.
- rust-milter matches libmilter on `SMFIC_QUIT`, so the milter client and any MTA can rely on one rule for both.

Negative:
- A handler that treated `ConnectionClosed` after HELO as the normal end now sees `Quit` there.
- An MTA that sends `SMFIC_QUIT` and then more commands on the same connection loses the connection. libmilter closes it too, and neither Sendmail nor Postfix does this.

## References

- [api-specification.md, protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior)
- [milter-protocol.md, rust-milter protocol support](../reference/milter-protocol.md#rust-milter-protocol-support)
- [ADR 0001](0001-commands-in-wrong-state-recover-like-libmilter-or-close.md)
- Sendmail 8.19.0.2 source: `libmilter/engine.c` (`mi_engine`, the `cmds` table, `st_quit`)
