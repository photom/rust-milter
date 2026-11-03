# Test scenarios: Command, Reply, and ReplyCode

Scenarios for decoding commands, encoding replies, and validating custom reply codes. Conventions are in [README.md](README.md).

## Command decoding

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| CMD-001 | unit | An `O` frame with version 6, actions `0x1FF`, and protocol flags `0x1FFFFF` | the frame is decoded | `OptNeg` with those three values | [protocol: Option negotiation message format](../reference/milter-protocol.md#option-negotiation-message-format) |
| CMD-002 | unit | An `O` frame with 11 data bytes | the frame is decoded | a protocol error | [protocol: Option negotiation message format](../reference/milter-protocol.md#option-negotiation-message-format) |
| CMD-003 | unit | An `O` frame with actions 0 and protocol flags 0 | the frame is decoded | `OptNeg` keeps the zeros; the version 1 defaults are applied later by the protocol engine (ENG-005) | [protocol: Option negotiation message format](../reference/milter-protocol.md#option-negotiation-message-format) |
| CMD-010 | unit | A `D` frame with stage `C` and the pairs `{j}`=`mx.void.invalid`, `{daemon_name}`=`smtpd` | the frame is decoded | `Macro` with stage `C` and both pairs in order | [protocol: Macro format](../reference/milter-protocol.md#macro-format-smfic_macro) |
| CMD-011 | unit | `D` frames with the stage bytes `L`, `B`, and `U` | each frame is decoded | `Macro` with the raw stage byte, no error | [protocol: Macros](../reference/milter-protocol.md#macros) |
| CMD-012 | unit | A `D` frame with a macro name and no value | the frame is decoded | a protocol error | [api: Command](../reference/api-specification.md#command) |
| CMD-020 | unit | A `C` frame with hostname `mail.void.invalid`, family `4`, port 25 in network byte order, and address `192.0.2.1` | the frame is decoded | `Connect` with those values and port 25 | [protocol: Connection info format](../reference/milter-protocol.md#connection-info-format-smfic_connect) |
| CMD-021 | unit | A `C` frame with family `6` and address `2001:db8::1` | the frame is decoded | `Connect` with family IPv6 and that address | [protocol: Connection info format](../reference/milter-protocol.md#connection-info-format-smfic_connect) |
| CMD-022 | unit | A `C` frame with family `U` and no port or address | the frame is decoded | `Connect` with family unknown and no port or address | [protocol: Connection info format](../reference/milter-protocol.md#connection-info-format-smfic_connect) |
| CMD-023 | unit | A `C` frame with family `L`, port 0, and an address string, as an MTA sends for a local client | the frame is decoded | `Connect` with family Unix, port 0, and the address string | [protocol: Connection info format](../reference/milter-protocol.md#connection-info-format-smfic_connect) |
| CMD-024 | unit | A `C` frame with family `4` that ends after the hostname and family | the frame is decoded | a protocol error | [protocol: Connection info format](../reference/milter-protocol.md#connection-info-format-smfic_connect) |
| CMD-025 | unit | A `C` frame whose family byte is `X`, not an `SMFIA_*` code | the frame is decoded | a protocol error | [protocol: Connection info format](../reference/milter-protocol.md#connection-info-format-smfic_connect) |
| CMD-030 | unit | An `H` frame with `client.void.invalid\0` | the frame is decoded | `Helo` with `client.void.invalid` | [api: Command](../reference/api-specification.md#command) |
| CMD-031 | unit | An `M` frame with `<a@void.invalid>\0SIZE=100\0BODY=8BITMIME\0` | the frame is decoded | `Mail` with the sender and both arguments in order | [protocol: Commands](../reference/milter-protocol.md#commands-from-mta-to-milter) |
| CMD-032 | unit | An `R` frame with `<b@void.invalid>\0` | the frame is decoded | `Rcpt` with the recipient and no arguments | [protocol: Commands](../reference/milter-protocol.md#commands-from-mta-to-milter) |
| CMD-033 | unit | An `L` frame whose value holds a folded header, with LF between the lines | the frame is decoded | `Header` with the value unchanged, LF included | [protocol: Headers and body](../reference/milter-protocol.md#headers-and-body) |
| CMD-034 | unit | A `B` frame with binary data that includes NUL and CR bytes | the frame is decoded | `Body` with the data unchanged | [api: Command](../reference/api-specification.md#command) |
| CMD-035 | unit | An `E` frame with no data, and another with 10 data bytes | each frame is decoded | `BodyEob` with empty data, and with the 10 bytes | [protocol: Commands](../reference/milter-protocol.md#commands-from-mta-to-milter) |
| CMD-036 | unit | `A`, `Q`, `K`, `T`, and `N` frames with no data | each frame is decoded | `Abort`, `Quit`, `QuitNewConnection`, `Data`, and `Eoh` | [api: Command](../reference/api-specification.md#command) |
| CMD-037 | unit | A `U` frame with `XFOO bar\0` | the frame is decoded | `Unknown` with `XFOO bar` | [api: Command](../reference/api-specification.md#command) |
| CMD-040 | unit | A frame with the command byte `Z` | the frame is decoded | a protocol error | [api: Command](../reference/api-specification.md#command) |
| CMD-041 | unit | An `H` frame whose string is not null-terminated | the frame is decoded | a protocol error | [api: Command](../reference/api-specification.md#command) |
| CMD-042 | unit | An `M` frame whose sender holds bytes that are not valid UTF-8 | the frame is decoded | `Mail` with the raw bytes kept in a `MilterText` whose `as_str()` returns `None`; no error | [api: Text from the MTA](../reference/api-specification.md#text-from-the-mta) |
| CMD-043 | unit | An `M` frame with an SMTPUTF8 address such as `<ユーザー@void.invalid>` | the frame is decoded | `Mail` whose sender's `as_str()` returns the address | [api: Text from the MTA](../reference/api-specification.md#text-from-the-mta) |
| CMD-044 | unit | `H`, `U`, `L`, and `C` frames with bytes after their last field, and an `O` frame with 16 data bytes | each frame is decoded | a protocol error for the first four; `OptNeg` from the first 12 bytes for the last, because libmilter accepts a longer option negotiation | [api: Command](../reference/api-specification.md#command) |

## Reply encoding

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RPL-001 | unit | The replies `Continue`, `Accept`, `Reject`, `TempFail`, `Discard`, `Skip`, and `Progress` | each is encoded | one frame each, with codes `c`, `a`, `r`, `t`, `d`, `s`, `p` and no data | [protocol: Replies](../reference/milter-protocol.md#replies-from-milter-to-mta) |
| RPL-002 | unit | `OptNeg` with version 6, actions `0x01`, protocol `0x00`, and no macro list | it is encoded | an `O` frame with exactly 12 data bytes in network byte order | [protocol: Option negotiation message format](../reference/milter-protocol.md#option-negotiation-message-format) |
| RPL-003 | unit | `OptNeg` with a macro list for stage `SMFIM_ENVFROM`: `{auth_authen} {mail_addr}` | it is encoded | the 12 bytes, then stage 2 as 4 bytes, then the list null-terminated | [protocol: Symbol list](../reference/milter-protocol.md#symbol-list-in-option-negotiation-optional) |
| RPL-004 | unit | `AddHeader` with `X-Test` and `yes` | it is encoded | an `h` frame with `X-Test\0yes\0` | [protocol: Header modification data formats](../reference/milter-protocol.md#header-modification-data-formats) |
| RPL-005 | unit | `InsertHeader` with index 0 | it is encoded | an `i` frame with index 0 as 4 bytes, then name and value | [protocol: Header modification data formats](../reference/milter-protocol.md#header-modification-data-formats) |
| RPL-006 | unit | `ChangeHeader` with index 1 and an empty value | it is encoded | an `m` frame with index 1, the name, and an empty value | [protocol: Header modification data formats](../reference/milter-protocol.md#header-modification-data-formats) |
| RPL-007 | unit | `ChangeFrom` with and without ESMTP arguments, and `AddRecipientWithArgs` with arguments | each is encoded | `e` and `2` frames with the address, then the arguments only when given | [protocol: Replies](../reference/milter-protocol.md#replies-from-milter-to-mta) |
| RPL-008 | unit | `ReplaceBody` with 65,536 bytes | it is encoded | an error; one frame carries at most 65,535 bytes | [api: Reply](../reference/api-specification.md#reply) |
| RPL-009 | unit | `AddHeader` with the names `X Test`, `X:Test`, and a name with a control character | each is encoded | an error for each | [api: Reply](../reference/api-specification.md#reply) |
| RPL-010 | unit | `ReplyCode` with the wire text `550 5.7.1 100% spam`, where `%` is not doubled | it is encoded | an error | [api: Reply](../reference/api-specification.md#reply) |
| RPL-011 | unit | `AddHeader` with an empty name | it is encoded | an error | [api: Reply](../reference/api-specification.md#reply) |
| RPL-012 | unit | `AddHeader` with a NUL byte in the value, and `AddRecipient` with a NUL byte in the address | each is encoded | an error for each; a NUL would end the field early on the wire | [api: Reply](../reference/api-specification.md#reply) |
| RPL-013 | unit | `ReplyCode` with the wire texts `250 OK`, `550`, `550-5.7.1 a` as the only line, `550 5.7.1 a` CRLF `550 5.7.1 b`, `550-a` CRLF `451 b`, and `550 4.7.1 x` | each is encoded | an error for each: a code that is not 4xx or 5xx, a line shorter than 4 characters, a wrong separator after the code, a different code on a later line, and an enhanced code with a different first digit | [protocol: Multi-line reply handling](../reference/milter-protocol.md#multi-line-reply-handling) |
| RPL-014 | unit | `AddHeader` with the values `ok` LF `Bcc: x@void.invalid`, `a` CR `b`, `a` LF space, and `a` LF TAB LF `b`, and with `first` LF TAB `second` | each is encoded | an error for the first four: they would add a header line or a line of only whitespace, which some parsers read as the end of the headers; the folded last value is encoded | [api: Reply](../reference/api-specification.md#reply) |
| RPL-015 | unit | `Quarantine`, `AddRecipient`, `DeleteRecipient`, and `ChangeFrom` with empty text | each is encoded | an error for each, as libmilter's `smfi_quarantine`, `smfi_addrcpt`, `smfi_delrcpt`, and `smfi_chgfrom` refuse it | [api: Reply](../reference/api-specification.md#reply) |
| RPL-016 | unit | `AddHeader` and `InsertHeader` with the name `X-Prüfung`, and `ChangeHeader` with the same name | each is encoded | an error for the first two, because a new header name is printable ASCII; `ChangeHeader` is encoded, because it names a header the MTA delivered | [api: Reply](../reference/api-specification.md#reply) |
| RPL-017 | unit | `AddRecipient`, `ChangeFrom` arguments, and `Quarantine` with text that contains CR or LF | each is encoded | an error for each; the line break would reach the MTA inside an envelope field | [api: Reply](../reference/api-specification.md#reply) |

## ReplyCode validation

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RCD-001 | unit | Code 550, enhanced code `5.7.1`, text `Rejected by policy` | the reply code is built and encoded | the wire text is `550 5.7.1 Rejected by policy` | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-002 | unit | Codes 250, 399, and 600 | each reply code is built | an error for each | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-003 | unit | Code 550 with enhanced code `4.7.1` | the reply code is built | an error: the first digits differ | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-004 | unit | Enhanced codes `5.7`, `5.7.1.1`, and `5.1000.1` | each reply code is built | an error for each | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-005 | unit | Text that contains CR or LF | the reply code is built | an error | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-006 | unit | Text of 980 bytes, and of 981 bytes | each reply code is built | the first succeeds, the second fails | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-007 | unit | Text `100% spam` | the reply code is encoded | the wire text contains `100%% spam` | [protocol: SMTP reply code handling](../reference/milter-protocol.md#smtp-reply-code-handling-smfir_replycode) |
| RCD-008 | unit | A multi-line reply with code 550, enhanced code `5.7.1`, and the lines `a`, `b`, `c` | it is encoded | the wire text is `550-5.7.1 a`, CRLF, `550-5.7.1 b`, CRLF, `550 5.7.1 c`, with no CRLF at the end | [protocol: Multi-line reply handling](../reference/milter-protocol.md#multi-line-reply-handling) |
| RCD-009 | unit | Multi-line replies with 32 and with 33 lines | each is built | the first succeeds, the second fails | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-010 | property | Any reply code that builds successfully | it is encoded | the wire text passes Postfix's line rules (each line at least 4 characters, `4` or `5` and two digits, `-` or space, and any digit after that equal to the code's first digit) and Sendmail's check (no single `%`) | [protocol: Multi-line reply handling](../reference/milter-protocol.md#multi-line-reply-handling) |
| RCD-011 | unit | Text that contains a NUL byte | the reply code is built | an error | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-012 | unit | A multi-line reply with no lines | it is built | an error | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
| RCD-013 | unit | Code 550 with no enhanced code, and the texts `100 messages` and `5 messages` | each reply code is built | the first fails, because Postfix reads a digit after the code as the start of an enhanced code; the second succeeds | [protocol: Multi-line reply handling](../reference/milter-protocol.md#multi-line-reply-handling) |
| RCD-014 | unit | Text of 490 `%` characters, and of 491 | each reply code is built | the first succeeds and the second fails: written as `%%`, they are 980 and 982 bytes, and the limit applies to the text on the wire | [api: Decisions and reply codes](../reference/api-specification.md#decisions-and-reply-codes) |
