# Test scenarios: Protocol engine

Scenarios for the library's protocol engine: option negotiation, callbacks and decisions, macros, the protocol state machine, abort and session end, and session isolation. Most are integration tests against a server on a leased test port, driven by a scripted fake MTA. Conventions are in [README.md](README.md).

## Option negotiation

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| ENG-001 | integration | A handler with `actions()` = `ADD_HEADERS` and all events | the MTA offers version 6, all actions, all protocol flags | the reply is version 6, actions `ADD_HEADERS`, and no `SMFIP_NO*` flags | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-002 | integration | A handler whose `events()` leave out headers and body | the MTA offers version 6 and all flags | the reply requests `SMFIP_NOHDRS` and `SMFIP_NOBODY` | [api: Negotiation methods](../reference/api-specification.md#negotiation-methods) |
| ENG-003 | integration | A default handler | the MTA offers version 2 with the version 2 protocol flags (`SMFI_V2_PROT`) | the reply is version 2, and requests no flag defined only in later versions | [protocol: rust-milter protocol support](../reference/milter-protocol.md#rust-milter-protocol-support) |
| ENG-004 | integration | A default handler | the MTA offers version 1, and in another run version 0 | the connection closes without a reply | [protocol: rust-milter protocol support](../reference/milter-protocol.md#rust-milter-protocol-support) |
| ENG-005 | integration | A handler with `actions()` = `CHANGE_FROM` | the MTA offers version 6 with actions 0 | the actions are treated as `SMFI_V1_ACTS`, which lacks `CHANGE_FROM`, and the connection closes | [protocol: Option negotiation message format](../reference/milter-protocol.md#option-negotiation-message-format) |
| ENG-006 | integration | A handler with `actions()` = `QUARANTINE` | the MTA offers every action except `QUARANTINE` | the connection closes without a reply | [api: Negotiation methods](../reference/api-specification.md#negotiation-methods) |
| ENG-007 | integration | A handler whose `events()` leave out `data` | the MTA offers version 6 but not `SMFIP_NODATA` | the reply does not request `SMFIP_NODATA`, and the reply flags are a subset of the offer | [protocol: Option negotiation message format](../reference/milter-protocol.md#option-negotiation-message-format) |
| ENG-008 | integration | A default handler | the MTA sends an option negotiation frame with 11 data bytes | the connection closes | [protocol: Option negotiation message format](../reference/milter-protocol.md#option-negotiation-message-format) |
| ENG-009 | integration | A default handler | the MTA offers version 7 | the reply is version 6 | [protocol: rust-milter protocol support](../reference/milter-protocol.md#rust-milter-protocol-support) |

## Callbacks and decisions

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| ENG-020 | integration | A handler that implements no callbacks | the MTA sends a complete session: macros, connect, HELO, MAIL, two RCPTs, DATA, three headers, EOH, two body chunks, end of message, QUIT | every command except the macros and QUIT is answered with `SMFIR_CONTINUE`, in order | [api: Callbacks](../reference/api-specification.md#callbacks) |
| ENG-021 | integration | A handler whose `connect` returns `Accept` | the MTA sends connect, then HELO, unlike Sendmail and Postfix, which stop after a connect-level accept | `SMFIR_ACCEPT` for connect; the state returns to `ST_HELO` as in libmilter, so `helo` is called and HELO gets its reply | [protocol: State transitions](../reference/milter-protocol.md#state-transitions) |
| ENG-022 | integration | A handler whose `rcpt` rejects `<bad@void.invalid>` | the MTA sends two RCPTs, the first one bad | `SMFIR_REJECT` for the first, and `rcpt` is still called for the second | [api: Callbacks](../reference/api-specification.md#callbacks) |
| ENG-023 | integration | A handler whose `mail` returns `Reject` | the MTA sends MAIL, then a new MAIL | `SMFIR_REJECT`; the second MAIL gets a fresh `Message` value, and `abort` is not called, because the reject ended the first message | [api: Associated types](../reference/api-specification.md#associated-types) |
| ENG-024 | integration | A handler whose `rcpt` returns `Reply` with code 451, and in another run 550 | the MTA sends RCPT | `SMFIR_REPLYCODE` with the reply text, `%` doubled | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| ENG-025 | integration | A handler whose `body` returns `Skip`, and an MTA that offered `SMFIP_SKIP` | the MTA sends three body chunks and end of message | `SMFIR_SKIP` after the first chunk; `body` is not called again; `end_of_message` is called | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| ENG-026 | integration | The same handler, and an MTA that did not offer `SMFIP_SKIP` | the MTA sends three body chunks and end of message | `SMFIR_CONTINUE` for every chunk; `body` is called only for the first | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| ENG-027 | integration | A handler that records which callbacks run | protocol version 2 is negotiated and the MTA sends a full session | `data` and `unknown` are never called | [api: Callbacks](../reference/api-specification.md#callbacks) |
| ENG-028 | integration | A handler whose `end_of_message` adds two headers, then returns `Continue` | the MTA sends end of message | two `SMFIR_ADDHEADER` frames in call order, then `SMFIR_CONTINUE` | [api: EomContext](../reference/api-specification.md#eomcontext) |
| ENG-029 | integration | A handler without `ADD_HEADERS` in `actions()` whose `end_of_message` calls `add_header` | the MTA sends end of message | `add_header` returns `ActionNotNegotiated`, no frame is sent for it, the final reply is still sent, and the connection stays open | [api: EomContext](../reference/api-specification.md#eomcontext) |
| ENG-030 | integration | A handler whose `end_of_message` replaces the body with 200,000 bytes | the MTA sends end of message | four `SMFIR_REPLBODY` frames, none larger than 65,535 bytes, whose data joins to the new body | [api: EomContext](../reference/api-specification.md#eomcontext) |
| ENG-031 | integration | A handler whose `end_of_message` calls `progress()` twice before deciding | the MTA sends end of message | two `SMFIR_PROGRESS` frames, then the final reply | [api: EomContext](../reference/api-specification.md#eomcontext) |
| ENG-032 | integration | A handler whose `header` returns an error | the MTA sends a header | `session_end` is called with `SessionEnd::Error`, and the connection closes | [api: Callbacks](../reference/api-specification.md#callbacks) |
| ENG-033 | integration | A recording handler | a full session runs | callbacks for one connection never overlap, and run in protocol order | [api: Callbacks](../reference/api-specification.md#callbacks) |

## Macros

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| ENG-040 | integration | A handler that reads `{client_addr}` in `connect` | the MTA sends a stage `C` macro with that value, then connect | `macro_value("{client_addr}")` returns the value | [api: SessionContext](../reference/api-specification.md#sessioncontext) |
| ENG-041 | integration | A handler that reads `{i}` in `end_of_message` | the MTA sent `{i}` with the end-of-message macros | `macro_value("{i}")` returns the queue ID | [api: SessionContext](../reference/api-specification.md#sessioncontext) |
| ENG-042 | integration | A handler that reads `{mail_addr}` in `rcpt` and `end_of_message` | the MTA sent `{mail_addr}` with the MAIL macros | both callbacks see the value | [api: SessionContext](../reference/api-specification.md#sessioncontext) |
| ENG-043 | integration | A default handler | the MTA sends macros with the stage bytes `L`, `B`, and `U` | no reply, no error, and the next command is processed | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-044 | integration | A handler whose `events()` leave out headers | the MTA sends a stage `L` macro and no header, as Postfix does | no reply, and the session continues | [protocol: Macros](../reference/milter-protocol.md#macros) |
| ENG-045 | integration | A handler that reads `{mail_addr}` in `mail` | message 1 sends `{mail_addr}` = `a@void.invalid` with the MAIL macros, and message 2 sends `b@void.invalid` | the second `mail` sees `b@void.invalid`: macros sent again for a stage replace the earlier ones | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-046 | integration | A handler that reads `{x}` in `rcpt` and in `mail` | message 1 sends `{x}` = `m1` with the MAIL macros and `r1` with the RCPT macros; message 2 sends `{x}` = `m2` with the MAIL macros | message 1's `rcpt` sees `r1`; message 2's `mail` sees `m2`, because a later stage's macros are not searched | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-047 | unit | The engine's macro store | macros arrive with the stage byte `Z`, which names no stage, and with `C` | only the `C` macros are stored: macros of a byte that is no stage cannot fill memory, and no lookup would reach them | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |

## Protocol state machine

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| ENG-050 | integration | A default handler, after HELO | the MTA sends, in separate runs, RCPT, DATA, a header, EOH, a body chunk, end of message, CONNECT, and option negotiation | the connection closes with a protocol error in every case, and no callback sees the command | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-051 | integration | A handler whose `end_of_message` returns `Continue` | the MTA sends `SMFIC_QUIT_NC`, then a new connect | `session_end` with `QuitNewConnection`; the connection stays open; the new connect gets a fresh `Session` value | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-052 | integration | A handler whose `end_of_message` returns `Accept` | the MTA sends `SMFIC_QUIT_NC` | the connection closes with a protocol error, and no callback sees the command | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-053 | integration | A handler whose `unknown` returns `Continue` | the MTA sends an unknown command, then `SMFIC_QUIT_NC` | the command is accepted and the session ends with `QuitNewConnection` | [protocol: rust-milter protocol support](../reference/milter-protocol.md#rust-milter-protocol-support) |
| ENG-054 | integration | A default handler, brought to each of the states `ST_CONN`, `ST_HELO`, `ST_MAIL`, `ST_RCPT`, `ST_DATA`, `ST_HDRS`, `ST_EOHS`, `ST_BODY` in turn | the MTA sends `SMFIC_QUIT_NC` | the connection closes in every case | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-055 | integration | Handlers that reject at MAIL, tempfail at end of message, and reject with a reply code at HELO | each is followed by `SMFIC_QUIT_NC` | the connection closes in every case, because each reply resets the state to `ST_HELO` | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-056 | integration | A default handler | the MTA sends a frame with an unknown command code | the connection closes with `ProtocolError::UnknownCommand` | [api: ProtocolError](../reference/api-specification.md#protocolerror) |
| ENG-057 | integration | A recording handler with a message open in `ST_BODY` | the MTA sends MAIL | `abort` is called for the open message, then `mail` with a fresh `Message` value, and MAIL gets its reply | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-058 | integration | A recording handler with a message open in `ST_RCPT` | the MTA sends HELO | `abort` is called, then `helo`, and HELO gets its reply | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-059 | integration | A recording handler with a message open in `ST_DATA` | the MTA sends an unknown SMTP command | `abort` is called, then `unknown`, and the command gets its reply | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-065 | integration | A handler whose `events()` leave out `mail`, after HELO | the MTA sends RCPT | `rcpt` is called and RCPT gets its reply, because libmilter's transition rules skip the switched-off stage | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-066 | integration | A recording handler, after HELO and no MAIL | the MTA sends `SMFIC_QUIT`, then closes the connection, as Postfix does when a client leaves without mail | no reply and no callback for the QUIT; `session_end` with `SessionEnd::ConnectionClosed` when the connection closes | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-067 | integration | A recording handler, after HELO | the MTA sends `SMFIC_ABORT`, then MAIL | the abort is ignored, and MAIL is processed normally | [ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md) |
| ENG-068 | integration | A recording handler whose `events()` leave out `connect`, and an MTA that offered `SMFIP_NOCONNECT` | the MTA sends HELO right after option negotiation, then `SMFIC_QUIT` after a complete message | `helo` is called with a fresh `Session` value, and `session_end` with `SessionEnd::Quit`: the session started with HELO | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-069 | integration | A recording handler with a message open in `ST_RCPT` | the MTA sends an unknown SMTP command, which libmilter allows there, then MAIL | `unknown` is called without `abort`; then `abort` for the open message, and `mail` with a fresh `Message` value | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-070 | integration | A recording handler whose `mail` returns `Reject` | the MTA sends RCPT anyway | the connection closes and `rcpt` is not called: the reject reset the state to `ST_HELO`, where RCPT does not fit | [protocol: State transitions](../reference/milter-protocol.md#state-transitions) |

## Abort and session end

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| ENG-060 | integration | A handler that marks its `Message` value in `mail` | the MTA sends MAIL, RCPT, `SMFIC_ABORT`, then a new MAIL | `abort` is called, no reply is sent for the abort, and the new MAIL gets a fresh `Message` value | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-061 | integration | A recording handler | the MTA sends `SMFIC_ABORT` after HELO, outside a message, as Postfix can | no callback runs, no reply is sent, and the connection stays open | [protocol: Abort handling](../reference/milter-protocol.md#abort-handling) |
| ENG-062 | integration | A recording handler, after a complete message | the MTA sends `SMFIC_QUIT` | `session_end` with `SessionEnd::Quit`, and no reply | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-063 | integration | A handler whose `connect` returns `Reject` | the MTA closes the socket without `SMFIC_QUIT`, as Postfix does after a connect-level decision | `session_end` with `SessionEnd::ConnectionClosed`, and no error is logged above DEBUG | [protocol: Connection lifecycle](../reference/milter-protocol.md#connection-lifecycle) |
| ENG-064 | integration | An open session waiting for the next command | the server shuts down | `session_end` with `SessionEnd::ServerShutdown` | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |

## Connection reset and end of file

A keepalive failure, such as an MTA host that went away, reaches the protocol engine as a failed read, the same way a reset does. These scenarios reset the connection from the fake MTA, by closing its socket with a linger time of 0, in every state the connection can be in. The keepalive socket options are covered in [server.md](server.md), and a real keepalive timeout in [load-and-chaos.md](load-and-chaos.md).

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| ENG-100 | integration | A recording handler; a connection just accepted, and in another run one after the option negotiation reply | the MTA resets the connection | no callback runs, and the server accepts and serves a new connection afterwards | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-101 | integration | A recording handler, brought to `ST_CONN`, `ST_HELO`, `ST_ENDM`, and `ST_UNKN` in turn, with no message open | the MTA resets the connection | `session_end` with `SessionEnd::Error`, and no `abort` | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-102 | integration | A recording handler, brought to `ST_MAIL`, `ST_RCPT`, `ST_DATA`, `ST_HDRS`, `ST_EOHS`, and `ST_BODY` in turn | the MTA resets the connection | `abort`, then `session_end` with `SessionEnd::Error` | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-103 | integration | A recording handler, after a session ended with `SMFIC_QUIT_NC` | the MTA resets the connection before the next connect | no callback runs after the first session's `session_end` | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-104 | integration | A recording handler, inside a message, and in another run outside one | the MTA sends half of a frame, then resets the connection | `abort` if a message is open, then `session_end` with `SessionEnd::Error` | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-105 | integration | A recording handler whose `end_of_message` waits 200 ms | the MTA sends end of message and resets the connection while the callback runs | the callback completes, then `abort` and `session_end` with `SessionEnd::Error`, because the final reply cannot be sent | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-106 | integration | A recording handler inside a message | the MTA sends half of a frame, then closes the connection normally | `abort`, then `session_end` with `SessionEnd::Error`: end of file in the middle of a frame is an error | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-107 | integration | A recording handler inside a message | the MTA closes the connection normally between frames | `abort`, then `session_end` with `SessionEnd::ConnectionClosed` | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |

## Session isolation

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| ENG-080 | integration | A handler that stores a marker in its `Session` value in `connect` | a session ends with `SMFIC_QUIT_NC` and a new connect follows | the new session's value is `Default`, without the marker | [api: Associated types](../reference/api-specification.md#associated-types) |
| ENG-081 | integration | A handler that stores a marker in its `Message` value in `mail` | a message ends and a second MAIL follows in the same session | the second message's value is `Default` | [api: Associated types](../reference/api-specification.md#associated-types) |
| ENG-082 | integration | A handler that reads every macro in `connect` | session 1 sends `{auth_authen}`=`alice`, ends with `SMFIC_QUIT_NC`, and session 2 sends no `{auth_authen}` | in session 2, `macro_value("{auth_authen}")` returns `None` | [explanation: Connection reuse and session isolation](../explanation/connection-reuse.md#why-a-reused-connection-forgets-everything) |
| ENG-083 | integration | Two connections from two fake MTAs, each with its own session data | both run sessions at the same time | each callback sees only its own connection's `Session` and `Message` values | [api: Callbacks](../reference/api-specification.md#callbacks) |

## Connection details

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| ENG-090 | integration | A handler that records `ConnectInfo` | the MTA sends connect with hostname `[192.0.2.1]` and address `192.0.2.1` port 4711 | `hostname` is `[192.0.2.1]` and `address` is `192.0.2.1:4711` | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
| ENG-091 | integration | The same handler | the MTA sends connect with family `SMFIA_UNKNOWN` | `address` is `None` | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
