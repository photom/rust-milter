# Test scenarios: Load and chaos

Scenarios that check the server under sustained load and under failures. They run on dedicated machines or CI runners, outside `cargo test`, against a release build with a handler that answers `Continue` to every command. Load scenarios use the prefix `LOD`, chaos scenarios `CHS`. Conventions are in [README.md](README.md).

## Load

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| LOD-001 | load | A server with `max_connections` = 10,000 and a load generator | 10,000 connections stay open for 1 hour, each running a session every 10 s | every reply arrives, no connection is closed by the server, and resident memory does not grow between the second and the last 10-minute window | [tcp-server: Load tests](../reference/tcp-server.md#load-tests) |
| LOD-002 | load | A server with default settings | the load generator opens and closes 1,000 connections per second for 10 minutes, each running one session | every session completes, and the number of open file descriptors returns to its starting value afterwards | [tcp-server: Load tests](../reference/tcp-server.md#load-tests) |
| LOD-003 | load | 1,000 concurrent sessions sending commands continuously | the time from a command's last byte to its reply's last byte is measured for 10 minutes | p50 is under 100 µs, p99 under 1 ms, and p99.9 under 10 ms | [api: Performance characteristics](../reference/api-specification.md#performance-characteristics) |
| LOD-004 | load | The same load at 100, 1,000, and 10,000 concurrent sessions | latency is measured at each level | p50, p95, and p99 are reported for each level | [tcp-server: Load tests](../reference/tcp-server.md#load-tests) |

## Chaos

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| CHS-001 | chaos | 1,000 concurrent sessions | the load generator drops 10% of its connections at random points, some halfway through a frame | the server keeps serving the other sessions; the open-connection count and `status()` match the generator's count of open connections | [tcp-server: Chaos tests](../reference/tcp-server.md#chaos-tests) |
| CHS-002 | chaos | 100 sessions with `timeouts.read` = 2 s and `timeouts.idle` = 10 s | the network between the generator and the server drops every packet for 30 s | each affected connection closes through a timeout or keepalive; after the partition ends, new connections are served | [tcp-server: Chaos tests](../reference/tcp-server.md#chaos-tests) |
| CHS-003 | chaos | A server whose process file descriptor limit is lower than the load needs | the load generator opens connections until `accept()` fails with `EMFILE` | the accept loop backs off and logs "Accept error"; when connections close, accepting resumes without a restart | [tcp-server: Error recovery](../reference/tcp-server.md#error-recovery) |
| CHS-004 | chaos | `max_memory` set below what the load needs | the load generator sends large header frames on many connections | new connections are closed once the limit is reached; the process stays below the limit and keeps serving the open connections | [tcp-server: Backpressure](../reference/tcp-server.md#backpressure) |
| CHS-005 | chaos | 5,000 active sessions and the default shutdown timings | the server receives `SIGTERM` | `run()` returns `Ok(())` within `grace_period` plus `drain_timeout`, and no process or socket is left behind | [tcp-server: Shutdown sequence](../reference/tcp-server.md#shutdown-sequence) |
| CHS-006 | chaos | 1,000 active sessions and a registered reload loader | the server receives 10 `SIGHUP` signals per second for 1 minute | the requests coalesce, every reload applies, and no session gets an error | [hot-reload: Reload coordinator states](../reference/hot-reload.md#reload-coordinator-states) |
| CHS-007 | chaos | 1,000 connections that send one byte per second, with `timeouts.read` = 5 s, next to 100 normal sessions | the load runs for 2 minutes | each slow connection closes through the read timeout, and the normal sessions are unaffected | [tcp-server: Security considerations](../reference/tcp-server.md#security-considerations) |
| CHS-008 | chaos | Sessions held in `ST_HELO`, in `ST_BODY`, and after end of message, with keepalive `time` = 2 s, `interval` = 1 s, `retries` = 3, `timeouts.read` = 1 h, and `timeouts.idle` = 1 h, so only keepalive can notice a lost peer | the network between the generator and the server drops every packet | each connection closes through keepalive within 10 s: `abort` for the session in `ST_BODY`, then `session_end` with `SessionEnd::Error` for every session | [api: Protocol engine behavior](../reference/api-specification.md#protocol-engine-behavior) |
