# Test scenarios: Interoperability with Sendmail and Postfix

Scenarios that run rust-milter behind real MTAs: the Postfix and Sendmail packages of the pinned Debian release. They run in the [interoperability environment](../reference/test-environment.md#interoperability-environment), outside `cargo test`; a scenario that names both MTAs runs once with each. The test sends mail with its own SMTP client, and reads the SMTP replies, the messages that reach the SMTP sink, and the logs. Every address uses the domain `void.invalid`. Conventions are in [README.md](README.md).

## Basic flow

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| INT-001 | interop | Both MTAs, each with a milter that adds `X-Filtered-By: rust-milter` at end of message | a client sends one message | the delivered message has the header, and the milter log shows no WARN or ERROR | [api: EomContext](../reference/api-specification.md#eomcontext) |
| INT-002 | interop | Postfix with `milter_protocol` = 2, 3, 4, and 6 (ports 2602, 2603, 2604, and 25), in four runs | a client sends one message with an unknown SMTP command before MAIL | the negotiated version equals the setting; `unknown` is called only for 3 and later, `data` only for 4 and later; the message is delivered | [protocol: Protocol version configuration](../reference/milter-protocol.md#protocol-version-configuration) |
| INT-003 | interop | Both MTAs | a client opens a session and quits without mail | the handler's `session_end` gets `Quit` or `ConnectionClosed`, and the milter log shows no WARN or ERROR | [protocol: Connection lifecycle](../reference/milter-protocol.md#connection-lifecycle) |
| INT-004 | interop | Both MTAs and a client that sends two messages in one SMTP session | the session runs | the milter sees two messages on one connection, each with a fresh `Message` value | [api: Associated types](../reference/api-specification.md#associated-types) |

## Decisions and replies

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| INT-010 | interop | Both MTAs and a handler whose `rcpt` rejects `bad@void.invalid` | a client sends to `bad@void.invalid` and `good@void.invalid` | the client gets 5xx for the first recipient and 250 for the second, and the message reaches only the second | [api: Callbacks](../reference/api-specification.md#callbacks) |
| INT-011 | interop | Both MTAs and a handler whose `end_of_message` returns `Reply` with `550 5.7.1 Rejected: 100% spam` | a client sends one message | the client sees exactly `550 5.7.1 Rejected: 100% spam` | [protocol: SMTP reply code handling](../reference/milter-protocol.md#smtp-reply-code-handling-smfir_replycode) |
| INT-012 | interop | Both MTAs and a multi-line `Reply` with three lines | a client sends one message | the client sees the three lines in order, each with the code and enhanced code | [protocol: Multi-line reply handling](../reference/milter-protocol.md#multi-line-reply-handling) |
| INT-013 | interop | Postfix and a handler whose `connect` returns `Reject` | a client connects | the client gets a 5xx reply; Postfix closes the milter connection without `SMFIC_QUIT`; `session_end` gets `ConnectionClosed`; the milter logs nothing above DEBUG | [protocol: Connection lifecycle](../reference/milter-protocol.md#connection-lifecycle) |
| INT-014 | interop | Both MTAs and a handler whose `end_of_message` returns `Discard` | a client sends one message | the client gets 250, and nothing is delivered | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| INT-015 | interop | Both MTAs and a handler that calls `quarantine("test")` at end of message | a client sends one message | Postfix puts the message on hold; Sendmail quarantines it with the reason `test` | [api: EomContext](../reference/api-specification.md#eomcontext) |
| INT-016 | interop | Both MTAs and a handler that tempfails at MAIL | a client sends one message | the client gets 4xx at MAIL, and can send a second message in the same session | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |

## Message modifications

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| INT-020 | interop | Both MTAs and a handler that changes one header, deletes another, adds a recipient, deletes a recipient, changes the sender, and replaces the body, with all those actions in `actions()` | a client sends one message | the delivered message and its envelope show every change | [api: EomContext](../reference/api-specification.md#eomcontext) |
| INT-021 | interop | Postfix and a handler that inserts a header at index 0 | a client sends one message | the header is placed relative to the message as delivered, including Postfix's own `Received:` header | [protocol: Headers and body](../reference/milter-protocol.md#headers-and-body) |
| INT-022 | interop | Both MTAs and a handler that replaces the body with 200,000 bytes | a client sends one message | the delivered body equals the replacement | [api: EomContext](../reference/api-specification.md#eomcontext) |

## Protocol details

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| INT-030 | interop | Both MTAs and a handler that records macros | a client sends one message | the handler sees `{i}` at end of message; with Postfix it also sees the `L`, `B`, and `U` stage macros without errors | [protocol: Macros](../reference/milter-protocol.md#macros) |
| INT-031 | interop | Postfix on port 2626, where the client's hostname is not looked up | the client connects | `ConnectInfo.hostname` is `[<client address>]`, and `address` is the client's address | [protocol: Hostname handling](../reference/milter-protocol.md#hostname-handling-smfic_connect) |
| INT-032 | interop | Both MTAs and a client that sends `MAIL` and `RCPT`, then `RSET`, then a new message | the session runs | the handler's `abort` runs for the first message, and the second message gets a fresh `Message` value | [protocol: Abort handling](../reference/milter-protocol.md#abort-handling) |
| INT-033 | interop | Postfix, `per_connection_buffer` = 256 KB, and a message with a 100,000-byte header | a client sends it | Withdrawn: when a milter is configured, Postfix cuts each header to 60,000 bytes, so a longer header never reaches the milter whole. INT-036 checks the cut | [protocol: Headers and body](../reference/milter-protocol.md#headers-and-body) |
| INT-034 | interop | Postfix with `milter_default_action = tempfail`, the default `per_connection_buffer`, and a message with a 200,000-byte header | a client sends it | Withdrawn: neither MTA sends a header packet larger than the default 65,540-byte packet, so `FrameTooLarge` cannot happen behind them. FRM-020 and SRV-038 test it against the fake MTA | [configuration: ResourceLimitsConfig](../reference/configuration.md#resourcelimitsconfig) |
| INT-035 | interop | Both MTAs and a handler that reads a header containing bytes that are not valid UTF-8 | a client sends it | the handler gets the raw bytes; `as_str()` returns `None` | [api: Text from the MTA](../reference/api-specification.md#text-from-the-mta) |
| INT-036 | interop | Postfix, the default `per_connection_buffer`, and a message with a 100,000-byte header | a client sends it | the handler receives the start of the header, cut to at most 60,000 bytes, and the message is delivered | [protocol: Headers and body](../reference/milter-protocol.md#headers-and-body) |

## Timeouts

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| INT-040 | interop | Postfix on port 2625, with `milter_content_timeout = 2s`, and a handler whose `end_of_message` takes 5 s and calls `progress()` every second | a client sends one message | the message is delivered: each `SMFIR_PROGRESS` restarts Postfix's timeout | [protocol: Timeouts](../reference/milter-protocol.md#timeouts) |
| INT-041 | interop | `sendmail-short`, with `T=E:3s;R:2s`, and the same handler | a client sends one message | Sendmail times out at end of message after about 3 s and applies the filter's `F=` flags: `progress()` does not extend the `E` timeout | [protocol: Timeouts](../reference/milter-protocol.md#timeouts) |
| INT-042 | interop | Both MTAs, `timeouts.idle` = 3 s, and a client that pauses 5 s between `RCPT` and `DATA` | the session runs | the milter closes the idle connection, and the MTA applies its failure policy to the session | [configuration: TimeoutConfig](../reference/configuration.md#timeoutconfig) |

## Server lifecycle

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| INT-050 | interop | Both MTAs, the milter running as a child process of the test, and a client that sends 50 messages in a loop | the milter receives `SIGTERM` during the loop | every message is either delivered or answered with the MTA's failure reply; none is lost without a reply; the milter exits with code 0 | [tcp-server: Shutdown sequence](../reference/tcp-server.md#shutdown-sequence) |
| INT-051 | interop | Both MTAs, the milter as a child process with a registered reload loader, and the same loop | the milter receives `SIGHUP` three times during the loop | every message is delivered, and the client sees no error reply | [hot-reload: Reload sequence](../reference/hot-reload.md#reload-sequence) |
| INT-052 | interop | Both MTAs and a stopped milter | a client sends one message | Postfix follows `milter_default_action`, and Sendmail follows the filter's `F=` flags | [protocol: Connection errors](../reference/milter-protocol.md#connection-errors) |
