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

1. **Connect (`SMFIC_CONNECT`)**: Triggered when a client connects to the MTA. Provides hostname and IP.
2. **HELO/EHLO (`SMFIC_HELO`)**: Triggered when the client sends the `HELO` or `EHLO` command.
3. **MAIL FROM (`SMFIC_MAIL`)**: The envelope sender address.
4. **RCPT TO (`SMFIC_RCPT`)**: Triggered for each envelope recipient. Can be called multiple times.
5. **DATA (`SMFIC_DATA`)**: Triggered when the `DATA` command is issued, before the message content.
6. **Header (`SMFIC_HEADER`)**: Triggered for each individual message header (key/value pair).
7. **End of Headers (`SMFIC_EOH`)**: Triggered when the blank line separating headers and body is reached.
8. **Body (`SMFIC_BODY`)**: Triggered repeatedly with chunks of the message body.
9. **End of Message (`SMFIC_BODYEOB` / EOM)**: The final dot `.` indicating the end of the data transmission. This is the only phase where the milter can safely modify the message (add/change headers, modify body).
10. **Abort/Close (`SMFIC_ABORT` / `SMFIC_QUIT`)**: Triggered when the connection is aborted prematurely or when it closes.

### Milter Responses (Actions)

After any callback (except Abort/Close/Unknown), the milter must respond to the MTA with an action:
- **Accept (`SMFIR_ACCEPT`)**: Accept the message (or at least, this stage of it). Bypass further milter callbacks for this message.
- **Continue (`SMFIR_CONTINUE`)**: Proceed to the next state/callback.
- **Reject (`SMFIR_REJECT`)**: Reject the message with a 5xx permanent error.
- **Discard (`SMFIR_DISCARD`)**: Silently discard the message (accept it from the client but drop it).
- **Tempfail (`SMFIR_TEMPFAIL`)**: Reject with a 4xx temporary error (try again later).
- **Reply Code (`SMFIR_REPLYCODE`)**: Provide a custom SMTP reply code and message (e.g., `550 5.7.1 Custom rejection message`).
- **Modification Actions**: Returned *only* during the EOM phase, followed by a final Accept or Continue:
  - Add/Insert/Change/Delete Header
  - Replace Body
  - Add/Del Recipient
  - Quarantine

## SMTP Protocol & Related RFCs

When implementing milters, you are directly interacting with the state of an SMTP transaction. Strict adherence to RFCs is essential.

- **RFC 5321 (Simple Mail Transfer Protocol)**: Defines the envelope and the commands (HELO, MAIL, RCPT, DATA). This governs the Connect through Data phases.
- **RFC 5322 (Internet Message Format)**: Defines the structure of the email itself (Headers and Body). This governs the Header and Body phases. 
- **RFC 2045 / RFC 2046 (MIME)**: Relevant if your milter needs to parse complex multipart bodies.

## Implementation Guidelines

1. **Protocol Framing**: Milter packets are structured as `[Length (32-bit uint)] [Command/Response (1 byte)] [Data (variable)]`. Network byte order (Big Endian) is used for all integers.
2. **State Management**: A single milter connection can process multiple messages sequentially over the same TCP connection (if the client sends `RSET` or multiple `MAIL FROM` commands in one session). Always clear message-specific state in `SMFIC_MAIL` or `SMFIC_ABORT`/`SMFIC_BODYEOB`.
3. **Modification Restrictions**: You cannot modify headers or the body during the `SMFIC_HEADER` or `SMFIC_BODY` phases. You must cache the necessary data and issue modification commands *only* during the `SMFIC_BODYEOB` (End of Message) phase.
4. **Timeouts**: MTAs impose strict timeouts on milters. Ensure body processing and DNS lookups (e.g., SPF/DKIM) are highly optimized or asynchronous.

## Reference Material
For exact byte-level packet structures, see the Sendmail libmilter source code (`include/libmilter/mfdef.h`) or standard protocol documentation.

## Related Skills

- For logging, masking, and observability guidelines within the milter, see **logging-principles**.
- For implementing the high-performance async aspects of the milter in Rust, refer to **rust-master**.
- For safely querying third-party APIs (e.g., DNSBL, spam engines) from the milter, follow **external-integration**.