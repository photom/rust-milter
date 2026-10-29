---
name: milter-protocol
description: Knowledge base and procedural guidance for the Sendmail/Postfix Milter (Mail Filter) protocol, SMTP stages, and related RFCs (e.g., RFC 5321, 5322). Use when implementing MTA integrations, writing mail filters, or parsing SMTP transactions.
---

# Milter and SMTP Protocols

This skill provides domain knowledge and procedural guidance for working with the Sendmail/Postfix Milter (Mail Filter) protocol, SMTP (Simple Mail Transfer Protocol), and their integration. 

## The Milter Protocol Overview

The Milter protocol is a binary, packet-based protocol used by Mail Transfer Agents (MTAs) like Sendmail and Postfix to communicate with external mail filtering applications. 

### Milter Transaction Lifecycle (Macro to Micro)

MTAs trigger milter callbacks at specific stages of the SMTP transaction. Understanding this flow is critical:

0. **Option Negotiation (`SMFIC_OPTNEG`)**: Sent first on every milter connection. MTA and milter agree on protocol version (current: **v6**), action flags (`SMFIF_*`), and protocol flags (`SMFIP_*`, e.g. which stages to skip, which stages need no reply).
   - **Macros (`SMFIC_MACRO`)**: Sent *before* the stage command they belong to (e.g. `{i}` queue ID, `{j}`, `{auth_authen}`). No reply is sent.
1. **Connect (`SMFIC_CONNECT`)**: Triggered when a client connects to the MTA. Provides hostname, address family (`SMFIA_INET`/`SMFIA_INET6`/`SMFIA_UNIX`/`SMFIA_UNKNOWN`), port, and address.
2. **HELO/EHLO (`SMFIC_HELO`)**: Triggered when the client sends the `HELO` or `EHLO` command.
3. **MAIL FROM (`SMFIC_MAIL`)**: The envelope sender address plus ESMTP arguments.
4. **RCPT TO (`SMFIC_RCPT`)**: Triggered for each envelope recipient. Can be called multiple times.
5. **DATA (`SMFIC_DATA`)**: Triggered when the `DATA` command is issued, before the message content (protocol v4+).
6. **Header (`SMFIC_HEADER`)**: Triggered for each individual message header (name/value pair).
7. **End of Headers (`SMFIC_EOH`)**: Triggered when the blank line separating headers and body is reached.
8. **Body (`SMFIC_BODY`)**: Triggered repeatedly with chunks of the message body (max `MILTER_CHUNK_SIZE` = 65535 bytes per chunk).
9. **End of Message (`SMFIC_BODYEOB` / EOM)**: The final dot `.` indicating the end of the data transmission. This is the only phase where the milter can safely modify the message (add/change headers, modify body, change envelope).
10. **Abort (`SMFIC_ABORT`)**: The current message is aborted (e.g. `RSET`); the connection continues. No reply is sent.
11. **Close (`SMFIC_QUIT` / `SMFIC_QUIT_NC`)**: `SMFIC_QUIT` ends the milter session. `SMFIC_QUIT_NC` (v6) ends the SMTP session but **keeps the milter socket open for a new SMTP connection** (Sendmail reuses connections; Postfix opens a new one per session). No reply is sent.
- **Unknown (`SMFIC_UNKNOWN`)**: Any SMTP command the MTA does not recognize (protocol v3+). May arrive at most stages and **expects a reply** like any other callback.

### Milter Responses (Actions)

After every command except `SMFIC_OPTNEG` (answered with its own `SMFIC_OPTNEG` packet), `SMFIC_MACRO`, `SMFIC_ABORT`, `SMFIC_QUIT`, and `SMFIC_QUIT_NC`, the milter must respond with an action — **unless** the corresponding no-reply flag (`SMFIP_NR_CONN`, `SMFIP_NR_HELO`, `SMFIP_NR_MAIL`, `SMFIP_NR_RCPT`, `SMFIP_NR_DATA`, `SMFIP_NR_UNKN`, `SMFIP_NR_EOH`, `SMFIP_NR_BODY`, `SMFIP_NR_HDR`) was negotiated in `SMFIC_OPTNEG`:
- **Accept (`SMFIR_ACCEPT`)**: Accept the message (or, at connection level, the whole connection). Bypass further milter callbacks for this message.
- **Continue (`SMFIR_CONTINUE`)**: Proceed to the next state/callback.
- **Reject (`SMFIR_REJECT`)**: Reject with a 5xx permanent error (for `RCPT`, only that recipient is rejected).
- **Discard (`SMFIR_DISCARD`)**: Silently discard the message (accept it from the client but drop it).
- **Tempfail (`SMFIR_TEMPFAIL`)**: Reject with a 4xx temporary error (try again later).
- **Reply Code (`SMFIR_REPLYCODE`)**: Provide a custom SMTP reply code and message (e.g., `550 5.7.1 Custom rejection message`).
- **Skip (`SMFIR_SKIP`)**: During `SMFIC_BODY`, stop receiving further body chunks and jump to EOM (v6; only if the MTA advertised `SMFIP_SKIP`).
- **Progress (`SMFIR_PROGRESS`)**: Keep-alive during long EOM processing; resets the MTA's timeout. May be sent multiple times before the final reply.
- **Modification Actions**: Returned *only* during the EOM phase, followed by a final Accept or Continue. Each requires the matching `SMFIF_*` action flag to have been negotiated:
  - Add/Insert/Change Header (`SMFIR_ADDHEADER`, `SMFIR_INSHEADER`, `SMFIR_CHGHEADER`; deleting a header = `SMFIR_CHGHEADER` with an empty value)
  - Replace Body (`SMFIR_REPLBODY`)
  - Change Envelope Sender (`SMFIR_CHGFROM`, v6)
  - Add/Delete Recipient (`SMFIR_ADDRCPT`, `SMFIR_ADDRCPT_PAR` with ESMTP args (v6), `SMFIR_DELRCPT`)
  - Quarantine (`SMFIR_QUARANTINE`)
- **Macro selection (`SMFIR_SETSYMLIST`)**: Not a stage reply; sent as part of the `SMFIC_OPTNEG` response to request specific macros per stage (v6).

## SMTP Protocol & Related RFCs

When implementing milters, you are directly interacting with the state of an SMTP transaction. Strict adherence to RFCs is essential.

- **RFC 5321 (Simple Mail Transfer Protocol)**: Defines the envelope and the commands (HELO, MAIL, RCPT, DATA). This governs the Connect through Data phases.
- **RFC 5322 (Internet Message Format)**: Defines the structure of the email itself (Headers and Body). This governs the Header and Body phases. 
- **RFC 2045 / RFC 2046 (MIME)**: Relevant if your milter needs to parse complex multipart bodies.
- **RFC 6531 / RFC 6532 (SMTPUTF8 / Internationalized Email)**: Envelope addresses and header values may contain raw UTF-8 when the client uses `SMTPUTF8` (enabled by default since Postfix 3.0). Do not assume ASCII-only data.

## Implementation Guidelines

1. **Protocol Framing**: Milter packets are structured as `[Length (32-bit uint)] [Command/Response (1 byte)] [Data (variable)]`. Network byte order (Big Endian) is used for all integers. The length **includes the command byte**. Default maximum data size is 64 KiB − 1 (`MILTER_MAX_DATA_SIZE`); Sendmail can negotiate 256 KiB / 1 MiB via `SMFIP_MDS_256K` / `SMFIP_MDS_1M`. Reject oversized lengths before allocating.
2. **State Management**: A single milter connection can process multiple messages sequentially (if the client sends `RSET` or multiple `MAIL FROM` commands in one session), and with `SMFIC_QUIT_NC` even multiple SMTP sessions. Always clear message-specific state in `SMFIC_MAIL` or `SMFIC_ABORT`/`SMFIC_BODYEOB`, and connection-specific state on `SMFIC_QUIT_NC`.
3. **Modification Restrictions**: You cannot modify headers or the body during the `SMFIC_HEADER` or `SMFIC_BODY` phases. You must cache the necessary data and issue modification commands *only* during the `SMFIC_BODYEOB` (End of Message) phase.
4. **Timeouts**: MTAs impose strict timeouts on milters. Ensure body processing and DNS lookups (e.g., SPF/DKIM) are highly optimized or asynchronous, and send `SMFIR_PROGRESS` during long EOM processing.
5. **Version Negotiation**: Postfix defaults to `milter_protocol = 6` (since 2.6; earlier defaults to 2). Never assume features beyond what was negotiated in `SMFIC_OPTNEG`; mask unsupported `SMFIP_*` bits instead of failing.

## Reference Material
- Project specification: `docs/reference/milter-protocol.md` (authoritative for this repository).
- Sendmail libmilter source (`include/libmilter/mfdef.h` for constants, `libmilter/engine.c` for the state machine).
- Postfix `MILTER_README` (supported requests per Postfix version, limitations).

## Related Skills

- For logging, masking, and observability guidelines within the milter, see **logging-principles**.
- For implementing the high-performance async aspects of the milter in Rust, refer to **rust-master**.
- For safely querying third-party APIs (e.g., DNSBL, spam engines) from the milter, follow **external-integration**.