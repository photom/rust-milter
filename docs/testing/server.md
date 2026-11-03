# Test scenarios: Server

Scenarios for the TCP server: listeners, socket options, the connection lifecycle and its timeouts, resource limits, accept errors, and status. Most are integration tests against a server on a leased test port, driven by a scripted fake MTA. Every timeout test sets short timeouts and finishes in under 10 seconds. Conventions are in [README.md](README.md).

## Listeners

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| SRV-001 | integration | A server with `bind_addresses` = the leased port on `127.0.0.1` | the fake MTA connects and sends option negotiation | the server answers with `SMFIC_OPTNEG` | [tcp-server: Socket address types](../reference/tcp-server.md#socket-address-types) |
| SRV-002 | integration | A server on the leased port on `::1`, with IPv6 tests enabled | the fake MTA connects over IPv6 and sends option negotiation | the server answers with `SMFIC_OPTNEG` | [tcp-server: Socket address types](../reference/tcp-server.md#socket-address-types) |
| SRV-003 | integration | A server with two leased ports on `127.0.0.1` | the fake MTA connects to each | both connections are served, and `status()` lists both listeners as `bound` and `accepting` | [tcp-server: Health check](../reference/tcp-server.md#health-check) |
| SRV-004 | unit | IPv6 listener sockets created with `ipv6_only` true, and with `ipv6_only` false | the socket's `IPV6_V6ONLY` option is read back | it matches `ipv6_only` in both cases | [tcp-server: IPv6 sockets](../reference/tcp-server.md#ipv6-sockets) |
| SRV-005 | integration | A server on the leased port on `127.0.0.1` only | the test tries to connect to the same port on `::1` | the connection is refused; the server binds no address beyond `bind_addresses` | [tcp-server: Security considerations](../reference/tcp-server.md#security-considerations) |
| SRV-006 | integration | `FailFast` and two leased ports, one already bound by the test | `run()` is called | `run()` returns `Error::Bind` naming the blocked address, and the other port can be bound by the test afterwards | [tcp-server: Partial bind failure](../reference/tcp-server.md#partial-bind-failure) |
| SRV-007 | integration | `BestEffort` and two leased ports, one already bound by the test | `run()` is called and the fake MTA connects to the free port | the connection is served; `status()` shows the blocked listener with `bound = false` and sets `last_error`; a WARN log names the blocked address | [tcp-server: Partial bind failure](../reference/tcp-server.md#partial-bind-failure) |
| SRV-008 | integration | `BestEffort` and two leased ports, both already bound by the test | `run()` is called | `run()` returns `Error::Bind` | [tcp-server: Partial bind failure](../reference/tcp-server.md#partial-bind-failure) |
| SRV-009 | integration | A server on a leased port, stopped with `shutdown()` after serving a connection | a new server starts on the same port at once | `run()` binds the port without an "address in use" error (`reuse_addr` defaults to true) | [configuration: SocketOptions](../reference/configuration.md#socketoptions) |

## Socket options

The options are applied once, right after `accept()` and before the first read, so the states before the first callback have the same options as the first callback reads. The tests read the options of the server's own accepted socket through the `test-support` feature ([test-environment.md](../reference/test-environment.md#socket-option-readback)).

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| SRV-020 | integration | Default socket options | a connection is accepted | the accepted socket has `TCP_NODELAY` on and keepalive on with 60 s time, 10 s interval, and 6 retries | [configuration: SocketOptions](../reference/configuration.md#socketoptions) |
| SRV-021 | integration | `keepalive = None` | a connection is accepted | the accepted socket has keepalive off | [configuration: SocketOptions](../reference/configuration.md#socketoptions) |
| SRV-022 | integration | `recv_buffer_size` and `send_buffer_size` of 64 KB | a connection is accepted | the socket's buffer sizes, read back, are at least 64 KB | [configuration: SocketOptions](../reference/configuration.md#socketoptions) |
| SRV-023 | integration | Default socket options, and a handler that reads its accepted socket's options in every callback | a session runs through every stage: connect, HELO, MAIL, RCPT, DATA, a header, EOH, a body chunk, end of message, an unknown command, an abort of a second message, `SMFIC_QUIT_NC`, a second session's connect, and `session_end` when the server shuts down | every read shows `TCP_NODELAY` on and keepalive on with 60 s time, 10 s interval, and 6 retries: nothing in any protocol state changes the options | [configuration: SocketOptions](../reference/configuration.md#socketoptions) |
| SRV-024 | integration | Keepalive with `time` = 30 s, `interval` = 5 s, `retries` = 3, and the same handler | the same session runs | every read shows those three values | [configuration: KeepaliveConfig](../reference/configuration.md#keepaliveconfig) |
| SRV-025 | integration | `keepalive = None` and `tcp_nodelay` = false, and the same handler | the same session runs | every read shows keepalive off and `TCP_NODELAY` off | [configuration: SocketOptions](../reference/configuration.md#socketoptions) |

## Connection lifecycle and timeouts

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| SRV-030 | integration | `timeouts.connect` = 200 ms | the fake MTA connects and sends nothing | the server closes the connection between 200 ms and 1 s after accept, with `Error::Timeout(Connect)` and close reason `timeout`, and logs "Connection timeout" at DEBUG with `timeout_type` | [configuration: TimeoutConfig](../reference/configuration.md#timeoutconfig) |
| SRV-031 | integration | `timeouts.connect` = 200 ms | the fake MTA sends option negotiation 100 ms after connecting, then waits 500 ms | the connection is still open after the wait | [configuration: TimeoutConfig](../reference/configuration.md#timeoutconfig) |
| SRV-032 | integration | `timeouts.idle` = 1 s, a session open after HELO | the fake MTA sends nothing more | the server closes the connection between 1 s and 2 s after the HELO reply, with `Error::Timeout(Idle)`; the handler's `session_end` gets `SessionEnd::Error` | [tcp-server: State transition rules](../reference/tcp-server.md#state-transition-rules) |
| SRV-033 | integration | `timeouts.idle` = 1 s | the fake MTA sends one command every 500 ms for 3 s | the connection stays open; each command restarts the idle wait | [configuration: TimeoutConfig](../reference/configuration.md#timeoutconfig) |
| SRV-034 | integration | `timeouts.read` = 200 ms | the fake MTA sends a frame's length field and half of its data, then stops | the server closes the connection between 200 ms and 1 s later, with `Error::Timeout(Read)` | [configuration: TimeoutConfig](../reference/configuration.md#timeoutconfig) |
| SRV-035 | integration | `timeouts.read` = 200 ms and `timeouts.idle` = 5 s | the fake MTA sends a 20-byte frame one byte every 50 ms | the server closes the connection with `Error::Timeout(Read)` before the frame is complete, although bytes keep arriving | [configuration: TimeoutConfig](../reference/configuration.md#timeoutconfig) |
| SRV-036 | integration | `timeouts.read`, `timeouts.write` = 200 ms, `timeouts.idle` = 1 s, and a handler whose `mail` sleeps 2 s | the fake MTA sends MAIL | the reply arrives after about 2 s and the connection stays open; time in a callback counts against no timeout | [configuration: TimeoutConfig](../reference/configuration.md#timeoutconfig) |
| SRV-037 | integration | `timeouts.write` = 200 ms, `send_buffer_size` = 4 KB, and a handler whose `end_of_message` replaces the body with 1 MB | the fake MTA sends end of message and stops reading | the server closes the connection with `Error::Timeout(Write)` within 2 s | [configuration: TimeoutConfig](../reference/configuration.md#timeoutconfig) |
| SRV-038 | integration | The default `per_connection_buffer` (128 KB) | the fake MTA sends a length field announcing 200,000 bytes, and no data | the server closes the connection with `Error::FrameTooLarge` without waiting for the data | [api: Frame](../reference/api-specification.md#frame) |
| SRV-039 | integration | A session open after HELO | the fake MTA closes its socket halfway through a frame | the connection closes with `Error::ConnectionClosed`, and the handler's `session_end` gets `SessionEnd::Error` | [api: Error](../reference/api-specification.md#error) |
| SRV-040 | integration | A session open after HELO | the fake MTA resets the connection (`SO_LINGER` 0, then close) | the connection closes with close reason `reset`, logged at DEBUG, not WARN | [tcp-server: Error recovery](../reference/tcp-server.md#error-recovery) |
| SRV-041 | integration | 100 connections, opened one after another | each is accepted | every `ConnectionInfo.id` is unique and larger than the one before, and matches the `conn_id` in the "Connection accepted" log | [tcp-server: Connection identifier](../reference/tcp-server.md#connection-identifier) |
| SRV-042 | integration | A session open after HELO | the fake MTA sends a frame with an unknown command code | the "Connection closed" log is WARN, with `reason` = `error` and an `error` field; a close for any other reason is DEBUG | [tcp-server: Connection lifecycle logs](../reference/tcp-server.md#connection-lifecycle-logs) |

## Resource limits

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| SRV-050 | integration | `max_connections` = 2 and two open connections | a third connection arrives | the server accepts it and closes it at once without sending data; the first two keep working; a WARN log "Connection rejected (limit)" is written, and `milter.server.connection.rejected` grows by 1 | [tcp-server: Backpressure](../reference/tcp-server.md#backpressure) |
| SRV-051 | integration | `max_connections` = 2, two open connections, and one of them closed by the fake MTA | a new connection arrives | it is served | [tcp-server: Backpressure](../reference/tcp-server.md#backpressure) |
| SRV-052 | integration | `per_connection_buffer` = 65,540, `max_memory` = 65,540, and connection A holding a 60,000-byte header frame in its buffer | connection B sends a 10,000-byte frame | B is closed with `Error::ResourceLimit(MaxMemory)`; A keeps working | [tcp-server: Backpressure](../reference/tcp-server.md#backpressure) |
| SRV-053 | unit | An accept loop over a fake listener that fails with `EMFILE` four times, then accepts, then fails once more, on the paused clock | the loop runs | it pauses 100, 200, 400, and 800 ms, accepts, then pauses 100 ms; each failure logs WARN "Accept error" with `consecutive_errors` | [tcp-server: Error recovery](../reference/tcp-server.md#error-recovery) |
| SRV-054 | unit | The same fake listener failing with `ENFILE` ten times in a row | the loop runs | the pause doubles from 100 ms and stays at 1 s once it reaches it | [tcp-server: Error recovery](../reference/tcp-server.md#error-recovery) |
| SRV-055 | unit | A fake listener that fails with `ECONNABORTED`, then accepts | the loop runs | it retries without a pause and logs at DEBUG | [tcp-server: Error recovery](../reference/tcp-server.md#error-recovery) |
| SRV-056 | integration | `per_connection_buffer` = 65,540, `max_memory` = 65,540, and connection A holding a 62,000-byte header frame in its buffer | connection B is accepted | B is closed at once without sending data, because its 4 KB reservation does not fit; a WARN log "Resource limit reached" has `limit_type` = `max_memory`; A keeps working | [tcp-server: Backpressure](../reference/tcp-server.md#backpressure) |

## Concurrency

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| SRV-060 | integration | A server with a recording handler | 100 fake MTAs each run a complete session at the same time | every session gets every reply, in order | [tcp-server: 10K connections support](../reference/tcp-server.md#10k-connections-support) |
| SRV-061 | integration | A handler whose `mail` sleeps 2 s on connection A only | connection B runs a session while A waits | B's replies each arrive within 100 ms | [tcp-server: 10K connections support](../reference/tcp-server.md#10k-connections-support) |

## Status and logs

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| SRV-070 | integration | A built server | `status()` is read before `run()`, after `run()` has bound, and with two connections open | `accepting` is false, then true; `connections` is 0, then 2; `uptime` grows; `shutting_down` is false; `config_generation` is 1 | [tcp-server: Health check](../reference/tcp-server.md#health-check) |
| SRV-071 | integration | A server and a fake MTA that runs one session and sends `SMFIC_QUIT` | the connection closes | the "Connection accepted" log has `conn_id`, `remote_addr`, `local_addr`, and `listener`; the "Connection closed" log has `duration_ms`, `bytes_read`, `bytes_written`, and `reason` = `client_closed`; both are DEBUG | [tcp-server: Connection lifecycle logs](../reference/tcp-server.md#connection-lifecycle-logs) |
| SRV-072 | integration | A server starting with two listeners | `run()` binds them | INFO logs "Server starting", "Listener bound" for each address with `backlog`, and "Server started" | [tcp-server: Server lifecycle logs](../reference/tcp-server.md#server-lifecycle-logs) |
| SRV-073 | integration | `max_connections` = 1 and one open connection | two more connections arrive and are closed at once | `status()` shows `connections_accepted` = 3 and `connections_rejected` = 2 | [api: ServerHandle](../reference/api-specification.md#serverhandle) |
