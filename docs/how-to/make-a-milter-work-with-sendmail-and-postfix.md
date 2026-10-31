# Make a milter work with both Sendmail and Postfix

Status: rust-milter is not implemented yet. This guide follows [milter-protocol.md](../reference/milter-protocol.md) and [api-specification.md](../reference/api-specification.md).

This guide lists what a rust-milter handler must do so that the same milter behaves correctly behind Sendmail and behind Postfix. Where the library already takes care of a point, the guide says so.

## Handle the connection lifecycle

1. Expect one SMTP session per milter connection.
2. Initialize per-session state on each `SMFIC_CONNECT`.
3. Clean up per-session state when the session ends: on `SMFIC_QUIT`, on `SMFIC_QUIT_NC`, and when `read_frame` returns `None` without a preceding `SMFIC_QUIT`. Postfix closes the socket without `SMFIC_QUIT` after a decision at connect or HELO.
4. Reset per-message state after `SMFIC_BODYEOB` or `SMFIC_ABORT`.
5. Accept `SMFIC_ABORT` outside a message transaction and ignore it. Postfix sends it after HELO or after a rejected `MAIL`.
6. Handle `SMFIC_QUIT_NC` for MTAs other than Sendmail and Postfix: drop all per-session state and keep only the negotiated options. The library closes the connection when `SMFIC_QUIT_NC` arrives in a state where it is not allowed, so the handler only sees it where it is valid.

## Negotiate the protocol version

1. Close the connection if the MTA offers a version below 2.
2. Answer with the lower of the MTA's version and 6.
3. Treat zero action flags or zero protocol flags from the MTA as the version 1 defaults (`SMFI_V1_ACTS`, `SMFI_V1_PROT`).
4. Request only action and protocol flags that the MTA offered. Sendmail closes the connection otherwise.
5. Expect no `SMFIC_UNKNOWN` below version 3 and no `SMFIC_DATA` below version 4. The table in [Protocol version](../reference/milter-protocol.md#protocol-version) lists what each version adds.

## Read hostnames correctly

1. Expect `[<ip>]` as the hostname of an unresolved client from either MTA.
2. Expect "unknown" from Postfix when the client's address is unknown too.
3. When the hostname is "unknown" or starts with `[`, take the IP address from the address field of `SMFIC_CONNECT`.

## Write reply codes both MTAs accept

1. Write every literal `%` as `%%`. Sendmail rejects a single `%`, and Postfix deletes it.
2. Start the reply with `4` for a temporary failure and `5` for a rejection, followed by two digits and a space.
3. Give an enhanced status code the same first digit as the reply code.
4. For a multi-line reply, start every line with the same 3-digit code, put `-` after the code on every line but the last and a space on the last, separate lines with CRLF, and end the last line without CRLF.
5. Do not rely on Sendmail showing the text of a reply to `SMFIC_CONNECT`; it keeps only the meaning of the code.

`Reply::to_frame` rejects a reply code that breaks these rules, so a mistake shows up as an error in the handler instead of a reply the MTA discards.

## Send each reply only where it is allowed

1. Send `SMFIR_PROGRESS` and `SMFIR_QUARANTINE` only at end of message. Postfix accepts them earlier; Sendmail treats them as an error.
2. Send `SMFIR_SKIP` only in reply to `SMFIC_BODY`, and only if the MTA offered `SMFIP_SKIP`.
3. Do not send `SMFIR_DISCARD` in reply to `SMFIC_CONNECT` or `SMFIC_HELO`. Postfix does not allow it there.
4. Send no reply to `SMFIC_MACRO`, `SMFIC_ABORT`, `SMFIC_QUIT`, or `SMFIC_QUIT_NC`, nor to a stage whose `SMFIP_NR_*` flag was negotiated.

## Accept every macro stage

Accept `SMFIC_MACRO` with any stage byte. Postfix also sends `L` (headers), `B` (body), and `U` (unknown commands), and sends macros even for stages the milter turned off. Ignore stages you do not use, and never reply to `SMFIC_MACRO`.

## Keep long work within the MTA's timeouts

1. Send `SMFIR_PROGRESS` while end-of-message processing takes long. It restarts the MTA's read timeout.
2. Keep the whole end-of-message stage within Sendmail's `E` timeout (5 minutes by default) and Postfix's `milter_content_timeout` (300 seconds by default). `SMFIR_PROGRESS` does not extend Sendmail's `E` timeout.

## Recover from errors

1. Keep processing after non-fatal errors.
2. Do not assume an error closes the connection; Postfix keeps the socket open after local queue file errors.
3. Handle missing or malformed commands without panicking. The library already rejects packets longer than `per_connection_buffer` before allocating memory.
4. Base no security decision on behavior that only one MTA shows.

## Test against both MTAs

Cover at least:
- the connection lifecycle, including Postfix closing without `SMFIC_QUIT` after a connect-level decision
- hostnames in the bracketed and "unknown" forms
- protocol versions 2, 3, 4, and 6, with Postfix's `milter_protocol`
- reply codes that contain `%`
- `SMFIC_ABORT` outside a transaction
- cleanup after errors

Use protocol version 6 in production; it has the full feature set.
