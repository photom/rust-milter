# ADR 0013: The first packet of a connection is limited to 1 KB

- Status: Accepted
- Date: 2026-11-08
- Affects: the connection's read path, `Error::FrameTooLarge`, the TCP server and API references

## Context

The milter protocol has no authentication, so anyone who can reach the port can open connections. When the 4-byte length field of a packet arrives, the server reserves room for the whole packet from `max_memory` and allocates the buffer before the data comes. `per_connection_buffer` bounds the packet, at 128 KB by default.

That reservation happens for the first packet too. A peer that opens `max_connections` connections and sends 4 bytes on each makes the server hold up to `max_connections` times `per_connection_buffer`: about 1.25 GB with the defaults, which leave `max_memory` unlimited. The connect timeout (10 s by default) ends each such connection, and the peer can open new ones.

Before option negotiation only `SMFIC_OPTNEG` is valid, and any other command closes the connection (ADR 0001). Sendmail and Postfix send it with 12 bytes of data, a 17-byte packet. libmilter accepts a longer option negotiation and reads its first 12 bytes.

## Options considered

- (a) Limit the first packet of every connection to a small fixed size, 1,024 bytes, and keep the defaults.
- (b) Also give `max_memory` a default instead of unlimited.
- (c) Also limit open connections per source address with a new setting.

## Decision

Option (a). Until the first packet has arrived, the server accepts a packet of at most 1,024 bytes, or `per_connection_buffer` if that is smaller. A longer one fails with `Error::FrameTooLarge`, with `max` = 1,024, as soon as its length field arrives, before any reservation for it. Later packets keep `per_connection_buffer` as their limit.

## Consequences

Positive:
- Before negotiation a connection holds at most its 4 KB base reservation, so the defaults bound that case at about 40 MB for 10,000 connections.
- No setting and no default changes, and real MTAs are not affected: their option negotiation is 17 bytes.

Negative:
- An MTA that pads its option negotiation past 1,024 bytes is refused. No known MTA does that.
- A peer can still hold 4 KB per connection and open as many connections as `max_connections` allows. Per-source limits stay with the firewall, and the production guide recommends setting `max_memory`.

## References

- [tcp-server.md, Backpressure](../reference/tcp-server.md#backpressure)
- [api-specification.md, Frame](../reference/api-specification.md#frame)
- [ADR 0001](0001-commands-in-wrong-state-recover-like-libmilter-or-close.md)
