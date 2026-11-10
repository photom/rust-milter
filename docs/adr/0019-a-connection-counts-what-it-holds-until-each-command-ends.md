# ADR 0019: A connection counts what it holds until each command ends

- Status: Accepted
- Date: 2026-11-10
- Supersedes: [ADR 0014](0014-stored-macros-count-against-the-connections-memory.md)
- Affects: the connection's read buffer and its `max_memory` reservation, command decoding, the protocol engine's macro store, `ResourceKind`, the TCP server, configuration, and API references

## Context

`max_memory` and `per_connection_buffer` bound what a connection may hold, so that a peer cannot make the server allocate more than the operator planned for. The milter protocol has no authentication, so any peer that reaches the port can send hostile frames. Sendmail and Postfix send a few hundred bytes of macros per stage, and a MAIL or RCPT frame of one address and a few ESMTP arguments; Sendmail accepts at most 50 macros per stage in a milter's request (`MAXFILTERMACROS`). libmilter has no limit of its own.

Three things a connection holds have to be counted:

- Stored macros. The protocol engine keeps the macros of up to ten stages until a later command or the end of the session drops them. A name or value that shares the read buffer it arrived in pins that whole buffer, which can be far larger than the macro after a large frame. Each stored pair also costs 64 bytes of bookkeeping, two `MilterText` values, whatever its length.
- Decoded strings ([#58](https://github.com/photom/rust-milter/issues/58)). The decoder splits MAIL, RCPT, and macro frames into one `MilterText` per NUL-terminated string. A `MilterText` takes 32 bytes on a 64-bit target and an empty string takes 1 byte on the wire, so a frame of NUL bytes decodes to 32 times its size: 4 MiB for a 128 KB frame. While a vector grows, its old and new buffers exist together, and the peak reaches 48 times. The decoder builds the vector after the frame is complete, when the reservation had already fallen back, and a MAIL or RCPT vector lives through the callback and the reply.
- The read buffer. To take a large frame, the read buffer grows to the frame's size. Once the frame was parsed, the reservation fell back to 4 KB, but the buffer kept its allocation: on the next read, the buffer took back the whole allocation. An idle connection could hold 128 KB for up to `timeouts.idle` (2 hours) while `max_memory` counted 4 KB.

## Options considered

For the decoded strings:
- (a) Count the strings before decoding the frame, reserve their bookkeeping with the frame, and keep the reservation until the command has finished.
- (b) Cap the number of strings in MAIL, RCPT, and macro frames.
- (c) Decode no vector: hand the handler the address and a view that splits the ESMTP arguments out of the frame on demand. This changes `Handler::mail` and `Handler::rcpt`.

For the read buffer:
- (d) Replace the read buffer with a new 4 KB buffer after a frame larger than 4 KB.
- (e) Count the read buffer's capacity in the reservation.

For the stored macros:
- (f) Copy each stored stage out of the read buffer, count the stored macros in the reservation, and cap them per connection at `per_connection_buffer`.
- (g) Cap the number of macros per stage, for example at Sendmail's 50.

## Decision

Options (a), (d), and (f):

- A command holds its frame and the strings decoded from it. Its cost is the packet's length plus 32 bytes per NUL byte in the data of a command that decodes to text: MAIL, RCPT, macros, CONNECT, HELO, a header, and an unknown command. Each string ends with a NUL byte, so the count bounds the strings. The 32 bytes are one `MilterText` on a 64-bit target, 16 on a 32-bit one.
- The engine counts the strings and reserves the command's cost before it decodes the frame, and decodes each list of strings into a vector of its exact size. The reservation then stays until the command has finished: its callback has returned and its reply is written.
- A command may cost at most `per_connection_buffer`. A frame whose command would cost more closes the connection with `Error::ResourceLimit(ResourceKind::PerConnectionBuffer)` before it is decoded, with or without `max_memory`.
- Storing a stage copies its names and values into one buffer of their exact size, so a stored macro does not pin a read buffer. A connection's stored macros cost their text plus 64 bytes per name and value pair, and may cost at most `per_connection_buffer`. A stage that would pass it closes the connection with `Error::ResourceLimit(ResourceKind::PerConnectionBuffer)`.
- The connection's reservation is the cost of its stored macros, plus the frame arriving or the command running, and at least 4 KB. When the reservation would pass `max_memory`, the connection closes with `Error::ResourceLimit(ResourceKind::MaxMemory)`.
- After a frame larger than 4 KB, the connection replaces its read buffer with a new 4 KB buffer, and copies into it any bytes of the next frame that arrived with the frame. The large allocation then belongs to the frame alone and is freed with it.
- Every one of these closes has the reason `limit_reached`.

Option (b) bounds the vector by a constant with no protocol meaning, and Postfix, which takes any macro list, does not promise a count. Option (c) removes the amplification but breaks the handler API for a cost that (a) already bounds. Option (e) keeps the accounting honest, but a peer could still make every connection hold `per_connection_buffer` for its whole life, and `max_memory` would then turn honest connections away. Option (g) bounds the bookkeeping of stored macros but not their text.

The cap on a command uses `per_connection_buffer` because the default configuration has no `max_memory`. Without the cap, one 128 KB frame would still make a connection allocate 4 MiB there.

## Consequences

Positive:
- `max_memory` covers everything the library allocates for a connection: the read buffer, the command being processed, and the stored macros.
- Without `max_memory`, a connection holds at most twice `per_connection_buffer`: one frame or command, and its stored macros.
- An idle connection holds its 4 KB buffer and its stored macros, and nothing more.
- Real MTAs are not affected: their commands cost a few hundred bytes more than their frames, and their macros a few kilobytes per connection, far below the 65,540-byte minimum of `per_connection_buffer`.

Negative:
- Before decoding a command that holds text, the engine reads its data once more to count the NUL bytes, and the decoder counts them again to size its vector. The data is at most `per_connection_buffer`.
- A NUL byte that ends no string, such as one in a CONNECT frame's port, still counts 32 bytes. A CONNECT frame has at most a few.
- A frame larger than 4 KB costs one more 4 KB allocation, plus a copy of the next frame's bytes that came with it. Body chunks are the common case, and reading one already costs far more.
- While the engine stores a stage, the stage's decoded form and its copy exist side by side for a moment. The decoded form is counted, and the copy is checked as soon as it is stored; each is bounded by `per_connection_buffer`.
- A handler that keeps a value from a frame, such as a body chunk, keeps the frame's memory alive after the reservation has fallen back. Values a handler keeps are its own, and are not counted.

## References

- [tcp-server.md, Backpressure](../reference/tcp-server.md#backpressure)
- [api-specification.md, Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior)
- [ADR 0013](0013-the-first-packet-is-limited-to-1-kb.md)
- [Issue #58](https://github.com/photom/rust-milter/issues/58)
