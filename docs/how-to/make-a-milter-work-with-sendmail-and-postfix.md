# Make a milter work with both Sendmail and Postfix

This guide follows [milter-protocol.md](../reference/milter-protocol.md) and [api-specification.md](../reference/api-specification.md).

This guide lists what a rust-milter handler author must do so that the same milter behaves correctly behind Sendmail and behind Postfix.

The library's protocol engine already handles the protocol differences between the two MTAs:
- version and flag negotiation
- macros on every stage, including Postfix's `L`, `B`, and `U`
- `SMFIC_ABORT` outside a message
- connections closed without `SMFIC_QUIT`
- `SMFIC_QUIT_NC`
- the no-reply rules

The steps below cover what is left to the handler.

## Keep state in Session and Message

1. Put everything that belongs to one SMTP session in your handler's `Session` type, and everything that belongs to one message in its `Message` type.
2. Do not keep session or message data in the handler's own fields. One handler value serves all connections at once.
3. Do cleanup work, such as flushing per-session statistics, in `session_end`. The library calls it for every way a session can end, including Postfix closing the socket without `SMFIC_QUIT` after a decision at connect or HELO.

## Do not depend on optional stages

1. Do not rely on `unknown` being called; it needs protocol version 3 or later.
2. Do not rely on `data` being called; it needs protocol version 4 or later.
3. Check `negotiated().version` in a callback if your logic depends on the version.
4. Keep Postfix's `milter_protocol` at 6, its default since Postfix 2.6. Sendmail always offers version 6.

## Read the SMTP client's address from ConnectInfo

Both MTAs send `[<ip>]` as the hostname of a client whose name did not resolve, and Postfix sends "unknown" when the address is unknown too. For decisions based on the client, use `ConnectInfo::address`, not the hostname.

## Request only the actions you use

1. Return from `actions()` only the modifications your milter performs.
2. Leave out `CHANGE_FROM` and `ADD_RECIPIENTS_WITH_ARGS` unless you need them. They need Sendmail 8.14 or later and Postfix 2.6 or later; an MTA that does not offer a requested action makes the library close the connection.

## Write reply codes for both MTAs

1. Build custom replies with `ReplyCode::new` or `ReplyCode::multi_line`. They reject codes that either MTA would refuse, and the library escapes `%` for you.
2. Use a 4xx code for temporary problems and a 5xx code for rejections.
3. Do not rely on Sendmail showing the text of a reply returned from `connect`; it keeps only the meaning of the code.

## Answer temporary failures with TempFail

When a lookup your milter depends on fails, such as DNS or a database, return `TempFail` from the callback. An error returned from a callback closes the connection, and the MTA then applies its failure policy to the whole session: Sendmail's `F=` flags or Postfix's `milter_default_action`.

## Stay within the MTA's timeouts

1. Call `progress()` on the end-of-message context while long processing runs. It restarts Sendmail's `R` timeout and Postfix's `milter_content_timeout` (300 seconds by default).
2. Finish the whole end-of-message stage within Sendmail's `E` timeout (5 minutes by default). `progress()` does not extend it.

## Test against both MTAs

Cover at least:
- the connection lifecycle, including Postfix closing without `SMFIC_QUIT` after a connect-level decision
- hostnames in the bracketed and "unknown" forms
- protocol versions 2, 3, 4, and 6, with Postfix's `milter_protocol`
- reply codes that contain `%`
- a message aborted halfway, and `SMFIC_ABORT` outside a message
- `TempFail` and errors from callbacks
