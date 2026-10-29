---
name: milter-protocol
description: Reference for the Sendmail/Postfix Milter (mail filter) protocol, SMTP stages, and the related RFCs (RFC 5321, RFC 5322, and others). Use when implementing MTA integrations, writing mail filters, or parsing SMTP transactions.
---

# Milter and SMTP

Milter is a binary, packet-based protocol. Mail Transfer Agents (MTAs) such as Sendmail and Postfix use it to talk to external mail filter applications.

## Transaction lifecycle

The MTA sends one command per stage of the SMTP transaction. In order:

0. `SMFIC_OPTNEG` (option negotiation) comes first on every milter connection. The MTA and the milter agree on the protocol version (current: v6), the action flags (`SMFIF_*`), and the protocol flags (`SMFIP_*`), which select the stages to skip and the stages that need no reply.
   - `SMFIC_MACRO` carries macros such as `{i}` (queue ID), `{j}`, and `{auth_authen}`. The MTA sends it just before the stage command the macros belong to. The milter does not reply.
1. `SMFIC_CONNECT`: a client connected to the MTA. Carries the hostname, the address family (`SMFIA_INET`, `SMFIA_INET6`, `SMFIA_UNIX`, or `SMFIA_UNKNOWN`), the port, and the address.
2. `SMFIC_HELO`: the client sent `HELO` or `EHLO`.
3. `SMFIC_MAIL`: the envelope sender (`MAIL FROM`) and its ESMTP arguments.
4. `SMFIC_RCPT`: one envelope recipient (`RCPT TO`). Arrives once per recipient.
5. `SMFIC_DATA`: the client sent `DATA`; the message content has not arrived yet (protocol v4 and later).
6. `SMFIC_HEADER`: one message header as a name/value pair. Arrives once per header.
7. `SMFIC_EOH`: the MTA reached the blank line between headers and body.
8. `SMFIC_BODY`: one chunk of the message body, at most `MILTER_CHUNK_SIZE` (65535) bytes. Repeats until the body ends.
9. `SMFIC_BODYEOB` (end of message, EOM): the client sent the final `.`. EOM is the only stage where the milter can modify the message: add or change headers, replace the body, or change the envelope.
10. `SMFIC_ABORT`: the MTA aborted the current message, for example after `RSET`. The connection stays open. The milter does not reply.
11. `SMFIC_QUIT` ends the milter session. `SMFIC_QUIT_NC` (v6) ends the SMTP session but keeps the milter socket open for the next SMTP connection; Sendmail reuses connections this way, while Postfix opens a new connection per session. The milter does not reply to either.

`SMFIC_UNKNOWN` (protocol v3 and later) carries any SMTP command the MTA does not recognize. It can arrive at most stages and needs a reply like any other stage command.

## Replies

The milter replies with an action to every command except these:

- `SMFIC_OPTNEG`, which gets an `SMFIC_OPTNEG` packet in return
- `SMFIC_MACRO`, `SMFIC_ABORT`, `SMFIC_QUIT`, and `SMFIC_QUIT_NC`
- any stage whose no-reply flag was negotiated in `SMFIC_OPTNEG`: `SMFIP_NR_CONN`, `SMFIP_NR_HELO`, `SMFIP_NR_MAIL`, `SMFIP_NR_RCPT`, `SMFIP_NR_DATA`, `SMFIP_NR_UNKN`, `SMFIP_NR_EOH`, `SMFIP_NR_BODY`, or `SMFIP_NR_HDR`

Actions:

- `SMFIR_ACCEPT` accepts the message, or the whole connection when sent at connection level. The MTA sends no further commands for that message.
- `SMFIR_CONTINUE` moves on to the next stage.
- `SMFIR_REJECT` rejects with a 5xx permanent error. At the `RCPT` stage it rejects only that recipient.
- `SMFIR_DISCARD` accepts the message from the client and then drops it without notice.
- `SMFIR_TEMPFAIL` rejects with a 4xx temporary error, so the client retries later.
- `SMFIR_REPLYCODE` sends a custom SMTP reply code and text, such as `550 5.7.1 Custom rejection message`.
- `SMFIR_SKIP` (v6) stops the body chunks during `SMFIC_BODY` and jumps to EOM. The milter may send it only if the MTA advertised `SMFIP_SKIP`.
- `SMFIR_PROGRESS` keeps the session alive during long EOM processing by resetting the MTA's timeout. The milter can send it several times before the final reply.

Modification actions are valid only at EOM, and the milter must follow them with a final `SMFIR_ACCEPT` or `SMFIR_CONTINUE`. Each one requires its `SMFIF_*` action flag from the negotiation:

- headers: `SMFIR_ADDHEADER`, `SMFIR_INSHEADER`, and `SMFIR_CHGHEADER` (an empty value deletes the header)
- body: `SMFIR_REPLBODY`
- envelope sender: `SMFIR_CHGFROM` (v6)
- recipients: `SMFIR_ADDRCPT`, `SMFIR_ADDRCPT_PAR` with ESMTP arguments (v6), and `SMFIR_DELRCPT`
- quarantine: `SMFIR_QUARANTINE`

`SMFIR_SETSYMLIST` (v6) is not a stage reply. The milter sends it with its `SMFIC_OPTNEG` response to ask for specific macros at each stage.

## SMTP and related RFCs

A milter acts on the state of a live SMTP transaction, so follow the RFCs exactly.

- RFC 5321 (SMTP) defines the envelope and the commands `HELO`, `MAIL`, `RCPT`, and `DATA`. It governs the stages from connect through `DATA`.
- RFC 5322 (Internet Message Format) defines the message itself, headers and body. It governs the header and body stages.
- RFC 2045 and RFC 2046 (MIME) apply when the milter parses multipart bodies.
- RFC 6531 and RFC 6532 (SMTPUTF8, internationalized email) allow raw UTF-8 in envelope addresses and header values when the client uses `SMTPUTF8`. Postfix enables it by default since 3.0. Do not assume ASCII.

## Implementation rules

1. Framing. Each packet is `[length (32-bit uint)] [command or reply (1 byte)] [data (variable)]`. All integers are in network byte order (big endian). The length includes the command byte. The default data limit is 64 KiB minus 1 (`MILTER_MAX_DATA_SIZE`); Sendmail can negotiate 256 KiB or 1 MiB with `SMFIP_MDS_256K` or `SMFIP_MDS_1M`. Reject an oversized length before you allocate a buffer for it.
2. State. One milter connection can carry several messages in sequence, when the client sends `RSET` or several `MAIL FROM` commands in one session. With `SMFIC_QUIT_NC` it can also carry several SMTP sessions. Clear per-message state on `SMFIC_MAIL`, `SMFIC_ABORT`, or `SMFIC_BODYEOB`, and clear per-connection state on `SMFIC_QUIT_NC`.
3. Modifications. The milter cannot modify headers or the body during `SMFIC_HEADER` or `SMFIC_BODY`. Keep the data you need and send the modification actions at `SMFIC_BODYEOB`.
4. Timeouts. MTAs give milters strict timeouts. Keep body processing and DNS lookups (SPF, DKIM) fast or asynchronous, and send `SMFIR_PROGRESS` during long EOM processing.
5. Version negotiation. Postfix defaults to `milter_protocol = 6` since Postfix 2.6; earlier versions default to 2. Use only the features negotiated in `SMFIC_OPTNEG`. When the MTA offers `SMFIP_*` bits the milter does not support, mask them out and continue.

## References

- `docs/reference/milter-protocol.md`: the project specification, authoritative for this repository.
- Sendmail libmilter source: `include/libmilter/mfdef.h` for the constants and `libmilter/engine.c` for the state machine.
- Postfix `MILTER_README`: the requests each Postfix version supports, and its limitations.

## Related skills

- logging-principles: logging, masking, and observability inside the milter.
- rust-master: the async Rust implementation.
- external-integration: calls from the milter to third-party APIs such as DNSBLs and spam engines.
