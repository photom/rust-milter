# ADR 0014: Stored macros count against the connection's memory

- Status: Superseded by ADR 0019
- Date: 2026-11-09
- Affects: the protocol engine's macro store, the connection's `max_memory` reservation, `ResourceKind`, the TCP server, configuration, and API references

## Context

The protocol engine keeps the macros of up to ten stages until a later command or the end of the session drops them. `max_memory` counted only the read buffer, so the stored macros were outside every limit:

- Each name and value is a `MilterText` that shares the read buffer it arrived in. A stored macro pins that whole buffer, which can be far larger than the macro after a large frame. With 10,000 idle connections that is up to 1.3 GB nobody counts.
- Each stored pair costs 64 bytes of bookkeeping, two `MilterText` values, whatever its length. A 128 KB macro frame of empty names and values decodes to about 4 MB, and ten stages to about 40 MB per connection.

The milter protocol has no authentication, so any peer that reaches the port can send such frames. Sendmail and Postfix send a few hundred bytes of macros per stage; Sendmail accepts at most 50 macros per stage in a milter's request (`MAXFILTERMACROS`). libmilter has no limit of its own.

## Options considered

- (a) Copy each stored stage out of the read buffer, count the stored macros in the connection's `max_memory` reservation, and cap them per connection at `per_connection_buffer`.
- (b) Cap the number of macros per stage, for example at Sendmail's 50, and keep the rest as it is.
- (c) Like (a), with a fixed cap such as 64 KB instead of `per_connection_buffer`.

## Decision

Option (a):

- Storing a stage copies its names and values into one buffer of their exact size, so a stored macro no longer pins a read buffer.
- A connection's stored macros cost their text plus 64 bytes per name and value pair. The connection's reservation is that cost plus the frame arriving, and at least 4 KB. When a stored stage would make the reservation pass `max_memory`, the connection closes with `Error::ResourceLimit(ResourceKind::MaxMemory)`.
- The stored macros of one connection may cost at most `per_connection_buffer`, with or without `max_memory`. A stage that would pass it closes the connection with `Error::ResourceLimit(ResourceKind::PerConnectionBuffer)`.
- Both close with the reason `limit_reached`.

Option (b) bounds the bookkeeping but not the text: ten stages of 50 macros can still hold ten frames of data, and a count is a new number that Postfix, which takes any macro list, does not promise. Option (c) adds a constant with no protocol meaning; `per_connection_buffer` already states how much one connection may hold.

## Consequences

Positive:
- `max_memory` covers everything the library keeps for a connection between commands.
- Without `max_memory`, a connection holds at most twice `per_connection_buffer`: one frame and its stored macros.
- Real MTAs are not affected: their macros cost a few kilobytes per connection, far below the 65,540-byte minimum of `per_connection_buffer`.

Negative:
- Storing a stage copies its macros once. They are a few hundred bytes per stage in practice.
- The worst case without `max_memory` doubles, from `max_connections` times `per_connection_buffer` to twice that. Only a peer that sends hostile macro frames reaches it, and the production guide recommends `max_memory`.
- `ResourceKind` gets a variant, `PerConnectionBuffer`. The enum is `#[non_exhaustive]`, so that is not a breaking change.

## References

- [tcp-server.md, Backpressure](../reference/tcp-server.md#backpressure)
- [api-specification.md, Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior)
- [ADR 0013](0013-the-first-packet-is-limited-to-1-kb.md)
