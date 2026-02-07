# TCP Server Specification

## Table of Contents

- [Overview](#overview)
- [Requirements Summary](#requirements-summary)
- [Architecture](#architecture)
  - [Component Overview](#component-overview)
  - [Async Runtime](#async-runtime)
- [Configuration](#configuration)
  - [Server Configuration](#server-configuration)
  - [Timeout Configuration](#timeout-configuration)
  - [Socket Options](#socket-options)
  - [Keepalive Configuration](#keepalive-configuration)
  - [Shutdown Configuration](#shutdown-configuration)
  - [Resource Limits Configuration](#resource-limits-configuration)
  - [Default Values](#default-values)
- [Socket Address Types](#socket-address-types)
  - [Unix Domain Sockets](#unix-domain-sockets)
  - [IPv4 Sockets](#ipv4-sockets)
  - [IPv6 Sockets](#ipv6-sockets)
  - [Address Parsing Errors](#address-parsing-errors)
- [Connection Lifecycle](#connection-lifecycle)
  - [State Machine](#state-machine)
  - [State Transition Rules](#state-transition-rules)
  - [Connection States](#connection-states)
- [Graceful Shutdown](#graceful-shutdown)
  - [Signal Handling Requirements](#signal-handling-requirements)
  - [Shutdown Sequence](#shutdown-sequence)
  - [Shutdown Exit Codes](#shutdown-exit-codes)
  - [Shutdown Coordination](#shutdown-coordination)
  - [Shutdown Timeline](#shutdown-timeline)
- [Connection Handling](#connection-handling)
  - [Connection Handler](#connection-handler)
  - [Connection Properties](#connection-properties)
  - [Connection Operations](#connection-operations)
  - [Connection Identifier](#connection-identifier)
- [Concurrency Model](#concurrency-model)
  - [10K Connections Support](#10k-connections-support)
  - [Resource Limits](#resource-limits)
  - [Backpressure](#backpressure)
- [Error Handling](#error-handling)
  - [Server Errors](#server-errors)
  - [Connection Errors](#connection-errors)
  - [Error Recovery](#error-recovery)
  - [Partial Bind Failure](#partial-bind-failure)
- [Logging Specification](#logging-specification)
  - [Log Levels](#log-levels)
  - [Connection Lifecycle Logs](#connection-lifecycle-logs)
  - [Server Lifecycle Logs](#server-lifecycle-logs)
  - [Error Logs](#error-logs)
  - [Log Fields](#log-fields)
- [Monitoring and Observability](#monitoring-and-observability)
  - [Metrics](#metrics)
  - [Health Check](#health-check)
- [API Design](#api-design)
  - [Server Builder](#server-builder)
  - [Server Handle](#server-handle)
- [Testing Requirements](#testing-requirements)
  - [Test Coverage](#test-coverage)
  - [Unit Tests](#unit-tests)
  - [Integration Tests](#integration-tests)
  - [Load Tests](#load-tests)
  - [Chaos Tests](#chaos-tests)
- [Security Considerations](#security-considerations)
- [Implementation Notes](#implementation-notes)
- [Dependencies](#dependencies)
- [Future Considerations](#future-considerations)
- [References](#references)

## Overview

This document specifies the design and requirements for a high-performance, async TCP server for the rust-milter implementation. The server must handle the milter protocol over TCP/IP and Unix domain sockets, supporting high concurrency with graceful shutdown capabilities.

## Requirements Summary

| Requirement | Target |
|-------------|--------|
| Concurrent connections | 10,000+ (C10K) |
| Socket types | Unix, IPv4, IPv6 |
| Graceful shutdown | SIGTERM and SIGINT signals |
| Configurable backlog | Yes |
| Configurable timeouts | Read/Write/Idle |
| Async runtime | Tokio |

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        TCP Server                                │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │   Listener  │  │   Listener  │  │   Listener  │   ...        │
│  │  (Unix)     │  │  (IPv4)     │  │  (IPv6)     │              │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘              │
│         │                │                │                      │
│         └────────────────┼────────────────┘                      │
│                          ▼                                       │
│              ┌───────────────────────┐                          │
│              │   Connection Acceptor │                          │
│              │   (with backlog)      │                          │
│              └───────────┬───────────┘                          │
│                          │                                       │
│         ┌────────────────┼────────────────┐                     │
│         ▼                ▼                ▼                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │ Connection  │  │ Connection  │  │ Connection  │   ...        │
│  │  Handler    │  │  Handler    │  │  Handler    │              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
│                                                                  │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                  Shutdown Coordinator                        ││
│  │  - Signal handling (SIGTERM, SIGINT, SIGQUIT)               ││
│  │  - Connection draining                                       ││
│  │  - Graceful termination                                      ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

### Async Runtime

The server MUST use **Tokio** as the async runtime with the following configuration:

| Setting | Description | Default |
|---------|-------------|---------|
| Runtime type | Multi-threaded runtime for production use | multi_thread |
| Worker threads | Number of worker threads | Number of CPU cores |
| I/O driver | Platform-specific async I/O | epoll (Linux), kqueue (macOS/BSD) |
| Thread stack size | Stack size per worker thread | 2 MiB |

## Configuration

### Server Configuration

| Field | Type | Description |
|-------|------|-------------|
| `bind_addresses` | Vec\<SocketAddr\> | Socket addresses to bind (supports multiple listeners) |
| `backlog` | u32 | Listen backlog size |
| `max_connections` | usize | Maximum concurrent connections (0 = unlimited) |
| `worker_threads` | Option\<usize\> | Number of worker threads (None = CPU cores) |
| `timeouts` | TimeoutConfig | Connection timeout settings |
| `socket_options` | SocketOptions | Socket options |
| `shutdown` | ShutdownConfig | Shutdown settings |
| `resource_limits` | ResourceLimitsConfig | Resource limit settings |

### Timeout Configuration

| Field | Type | Description |
|-------|------|-------------|
| `read_timeout` | Duration | Read timeout for socket operations |
| `write_timeout` | Duration | Write timeout for socket operations |
| `idle_timeout` | Duration | Connection idle timeout (no data exchanged) |
| `connect_timeout` | Duration | Initial connection timeout (time to complete handshake) |

### Socket Options

| Field | Type | Description |
|-------|------|-------------|
| `tcp_nodelay` | bool | Enable TCP_NODELAY (disable Nagle's algorithm) |
| `reuse_addr` | bool | Enable SO_REUSEADDR |
| `reuse_port` | bool | Enable SO_REUSEPORT (Linux 3.9+) |
| `keepalive` | Option\<KeepaliveConfig\> | TCP keepalive settings |
| `recv_buffer_size` | Option\<usize\> | Receive buffer size (SO_RCVBUF), None = OS default |
| `send_buffer_size` | Option\<usize\> | Send buffer size (SO_SNDBUF), None = OS default |
| `ipv6_only` | bool | Enable IPV6_V6ONLY (disable dual-stack) |

### Keepalive Configuration

| Field | Type | Description |
|-------|------|-------------|
| `time` | Duration | Time before first keepalive probe |
| `interval` | Duration | Interval between keepalive probes |
| `retries` | u32 | Number of failed probes before connection is dropped |

### Shutdown Configuration

| Field | Type | Description |
|-------|------|-------------|
| `grace_period` | Duration | Time to wait before starting drain (allows in-flight requests to arrive) |
| `drain_timeout` | Duration | Maximum time to wait for existing connections to complete after grace period |

### Resource Limits Configuration

| Field | Type | Description |
|-------|------|-------------|
| `max_fds` | Option\<usize\> | Maximum file descriptors (None = system limit) |
| `max_memory` | Option\<usize\> | Memory limit for buffers in bytes (None = unlimited) |
| `per_connection_buffer` | usize | Per-connection buffer limit in bytes |

### Default Values

| Parameter | Default Value | Range |
|-----------|---------------|-------|
| `backlog` | 1024 | 1 - 65535 |
| `max_connections` | 10000 | 0 (unlimited) - 1000000 |
| `worker_threads` | CPU cores | 1 - 1024 |
| `read_timeout` | 30 seconds | 1ms - 1 hour |
| `write_timeout` | 30 seconds | 1ms - 1 hour |
| `idle_timeout` | 300 seconds | 1s - 24 hours |
| `connect_timeout` | 10 seconds | 1ms - 5 minutes |
| `tcp_nodelay` | true | - |
| `reuse_addr` | true | - |
| `reuse_port` | false | - |
| `ipv6_only` | false | - |
| `keepalive.time` | 60 seconds | 1s - 24 hours |
| `keepalive.interval` | 10 seconds | 1s - 1 hour |
| `keepalive.retries` | 6 | 1 - 30 |
| `recv_buffer_size` | OS default | 1KB - 64MB |
| `send_buffer_size` | OS default | 1KB - 64MB |
| `grace_period` | 5 seconds | 0s - 1 minute |
| `drain_timeout` | 30 seconds | 1s - 5 minutes |
| `max_fds` | System limit | - |
| `max_memory` | Unlimited | - |
| `per_connection_buffer` | 64 KB | 1KB - 16MB |

## Socket Address Types

The server MUST support the following socket types as defined in the milter protocol:

### Unix Domain Sockets

| Format | Example |
|--------|---------|
| `unix:/path/to/socket` | `unix:/var/run/milter.sock` |
| `local:/path/to/socket` | `local:/tmp/milter.sock` |

| Setting | Default | Description |
|---------|---------|-------------|
| `file_permissions` | 0o660 | Unix file permissions for socket |
| `remove_existing` | true | Remove existing socket file before binding |
| `max_path_length` | 108 bytes | Maximum socket path length (OS limit) |

**Unix socket considerations:**
- Remove existing socket file before binding (configurable)
- Set appropriate file permissions (default: 0o660)
- Handle `EADDRINUSE` errors gracefully
- Path length MUST NOT exceed 108 bytes (sun_path limit)

### IPv4 Sockets

| Format | Example |
|--------|---------|
| `inet:port@host` | `inet:8891@localhost` |
| `host:port` | `127.0.0.1:8891` |

### IPv6 Sockets

| Format | Example |
|--------|---------|
| `inet6:port@host` | `inet6:8891@::1` |
| `[host]:port` | `[::1]:8891` |

| Setting | Default | Description |
|---------|---------|-------------|
| `ipv6_only` | false | When false, accept IPv4 connections on IPv6 socket (dual-stack) |

**IPv6 considerations:**
- Support dual-stack sockets (IPv4-mapped IPv6) when `ipv6_only` is false
- When `ipv6_only` is true, only accept IPv6 connections

### Address Parsing Errors

| Error | Description |
|-------|-------------|
| `InvalidFormat` | Address format not recognized |
| `InvalidHost` | Host name or IP address invalid |
| `InvalidPort` | Port number out of range (1-65535) |
| `PathTooLong` | Unix socket path exceeds 108 bytes |

## Connection Lifecycle

### State Machine

```
                              ┌─────────────────────────────────────┐
                              │                                     │
                              ▼                                     │
┌─────────┐   accept()   ┌───────────┐                              │
│ PENDING │─────────────▶│ CONNECTED │──────────────────────────────┤
└─────────┘              └─────┬─────┘                              │
                               │                                    │
              ┌────────────────┼────────────────┐                   │
              │                │                │                   │
              ▼                ▼                ▼                   │
       ┌───────────┐    ┌───────────┐    ┌───────────┐             │
       │   IDLE    │◀──▶│  ACTIVE   │───▶│   ERROR   │─────────────┤
       │ (waiting  │    │(processing│    │           │             │
       │ for data) │    │ request)  │    │           │             │
       └─────┬─────┘    └─────┬─────┘    └───────────┘             │
             │                │                                     │
             │                │   shutdown signal                   │
             │                ▼                                     │
             │        ┌───────────────┐                             │
             └───────▶│   DRAINING    │                             │
                      │(completing    │                             │
                      │ current work) │                             │
                      └───────┬───────┘                             │
                              │                                     │
                              ▼                                     │
                      ┌───────────────┐                             │
                      │    CLOSED     │◀────────────────────────────┘
                      └───────────────┘
```

### State Transition Rules

| From | To | Trigger |
|------|----|---------|
| PENDING | CONNECTED | `accept()` succeeds |
| PENDING | CLOSED | `accept()` fails or server shutting down |
| CONNECTED | IDLE | Connection ready, waiting for first data |
| CONNECTED | ERROR | Handshake timeout or error |
| CONNECTED | CLOSED | Immediate close (client disconnect) |
| IDLE | ACTIVE | Data received |
| IDLE | ERROR | Idle timeout exceeded |
| IDLE | DRAINING | Shutdown signal received |
| IDLE | CLOSED | Client closes connection |
| ACTIVE | IDLE | Request processing complete |
| ACTIVE | ERROR | I/O error or protocol error |
| ACTIVE | DRAINING | Shutdown signal received (after current request) |
| ACTIVE | CLOSED | Client closes connection |
| ERROR | CLOSED | Error logged, cleanup complete |
| DRAINING | CLOSED | Current work complete or drain timeout |

### Connection States

| State | Description |
|-------|-------------|
| `PENDING` | Connection in accept queue (backlog) |
| `CONNECTED` | TCP handshake complete, ready for data |
| `IDLE` | Waiting for data, subject to idle timeout |
| `ACTIVE` | Currently processing a request |
| `DRAINING` | Shutdown initiated, completing current work |
| `ERROR` | Error occurred, connection will be closed |
| `CLOSED` | Connection terminated |

## Graceful Shutdown

### Signal Handling Requirements

The TCP server **MUST** handle the following signals for graceful shutdown:

| Signal | Required | Behavior |
|--------|----------|----------|
| `SIGTERM` | **REQUIRED** | Initiate graceful shutdown |
| `SIGINT` | **REQUIRED** | Initiate graceful shutdown (Ctrl+C) |
| `SIGQUIT` | Optional | Graceful shutdown with core dump (if enabled) |
| `SIGHUP` | Optional | Reserved for future configuration reload |

**Signal handling implementation requirements:**

1. Signal handlers MUST be registered before the server starts accepting connections
2. Signal handlers MUST be async-safe (use `tokio::signal`)
3. Multiple signals of the same type MUST be deduplicated (only first triggers shutdown)
4. Receiving a second SIGTERM or SIGINT during shutdown SHOULD force immediate termination
5. Signal handlers MUST NOT block the async runtime

**Double signal behavior:**

| First Signal | Second Signal | Behavior |
|--------------|---------------|----------|
| SIGTERM | SIGTERM | Force immediate shutdown (skip drain) |
| SIGINT | SIGINT | Force immediate shutdown (skip drain) |
| SIGTERM | SIGINT | Force immediate shutdown (skip drain) |
| SIGINT | SIGTERM | Force immediate shutdown (skip drain) |

### Shutdown Sequence

1. **Signal Reception**: Server receives shutdown signal (SIGTERM or SIGINT)
2. **Log Signal**: Log the received signal at INFO level
3. **Stop Accepting**: Immediately stop accepting new connections on all listeners
4. **Grace Period**: Wait `grace_period` for in-flight requests to arrive
5. **Notify Connections**: Propagate shutdown notification to all active connections via cancellation token
6. **Drain Period**: Allow existing connections to complete within `drain_timeout`
7. **Force Close**: After drain timeout, forcefully close remaining connections
8. **Cleanup**: Release resources, remove Unix socket files
9. **Log Completion**: Log shutdown completion at INFO level
10. **Exit**: Server process terminates with exit code 0

### Shutdown Exit Codes

| Condition | Exit Code |
|-----------|-----------|
| Normal shutdown (SIGTERM/SIGINT) | 0 |
| Forced shutdown (double signal) | 0 |
| Shutdown due to fatal error | 1 |
| Configuration error | 2 |

### Shutdown Coordination

The shutdown coordinator manages the following:

| Component | Description |
|-----------|-------------|
| Cancellation token | Coordinated shutdown signal propagation |
| Broadcast channel | Shutdown notifications with reason |
| Active connection counter | Track connections for drain completion |
| Completion notifier | Signal when all connections are closed |

**Shutdown reasons:**
- Signal (with signal number)
- Manual (programmatic shutdown)
- Error (with error description)

### Shutdown Timeline

```
Signal     Grace Period          Drain Timeout              Force Close
  │        (5 seconds)           (30 seconds)                    │
  │             │                      │                         │
  ▼             ▼                      ▼                         ▼
──┬─────────────┬──────────────────────┬─────────────────────────┬──▶ time
  │             │                      │                         │
  │ Stop        │ Start draining       │ Force close             │ Exit
  │ accepting   │ existing             │ remaining               │
  │ new         │ connections          │ connections             │
  │ connections │                      │                         │
```

## Connection Handling

### Connection Handler

The connection handler trait defines the following methods:

| Method | Parameters | Return | Required | Description |
|--------|------------|--------|----------|-------------|
| `handle` | Connection | Result\<(), ConnectionError\> | Yes | Handle a new connection |
| `on_connect` | &Connection | Result\<(), ConnectionError\> | No | Called when connection is established |
| `on_disconnect` | &Connection, DisconnectReason | () | No | Called when connection is closing |

### Connection Properties

| Property | Type | Description |
|----------|------|-------------|
| `id` | ConnectionId | Unique connection identifier (see below) |
| `remote_addr` | Option\<SocketAddr\> | Remote address (None for Unix sockets without SO_PEERCRED) |
| `local_addr` | SocketAddr | Local address |
| `connected_at` | Instant | Connection established time |
| `state` | ConnectionState | Current connection state |
| `bytes_read` | u64 | Total bytes read on this connection |
| `bytes_written` | u64 | Total bytes written on this connection |

### Connection Operations

| Operation | Parameters | Return | Description |
|-----------|------------|--------|-------------|
| `read_with_timeout` | &mut [u8], Duration | Result\<usize, ConnectionError\> | Read data with timeout |
| `write_with_timeout` | &[u8], Duration | Result\<(), ConnectionError\> | Write data with timeout |
| `is_shutdown_requested` | - | bool | Check if shutdown was requested |
| `read_or_shutdown` | &mut [u8] | Result\<ReadResult, ConnectionError\> | Wait for data or shutdown |
| `shutdown_write` | - | Result\<(), ConnectionError\> | Half-close the write side |

**Read results:**
| Variant | Description |
|---------|-------------|
| `Data(usize)` | Data received with byte count |
| `Shutdown` | Shutdown signal received |
| `Timeout` | Read timeout occurred |

**Disconnect reasons:**
| Reason | Description |
|--------|-------------|
| `ClientClosed` | Client initiated close |
| `ServerShutdown` | Server shutdown in progress |
| `Timeout` | Idle or read/write timeout |
| `Error` | Error occurred |
| `Reset` | Connection reset by peer |

### Connection Identifier

| Property | Description |
|----------|-------------|
| Type | 64-bit unsigned integer (u64) |
| Generation | Monotonically increasing, wraps at u64::MAX |
| Uniqueness | Unique within server lifetime (until wrap) |
| Format | Opaque, no guaranteed structure |

The connection ID is generated atomically when a connection is accepted and is used for logging and tracking purposes.

## Concurrency Model

### 10K Connections Support

To support 10,000+ concurrent connections:

1. **Non-blocking I/O**: All socket operations MUST be non-blocking
2. **Efficient polling**: Use epoll/kqueue via Tokio
3. **Task spawning**: Each connection runs in its own Tokio task
4. **Resource limits**: Implement connection limits to prevent resource exhaustion

### Resource Limits

| Limit | Description |
|-------|-------------|
| `max_fds` | Maximum file descriptors (connections + internal) |
| `max_memory` | Memory limit for buffers (approximate) |
| `per_connection_buffer` | Per-connection buffer limit |

### Backpressure

When approaching resource limits:

1. **Connection limit reached**: Reject new connections with `ECONNREFUSED`
2. **Memory pressure**: Apply backpressure to slow clients
3. **CPU saturation**: Limit concurrent request processing

## Error Handling

### Server Errors

| Error | Description |
|-------|-------------|
| `Bind` | Failed to bind to address |
| `Accept` | Failed to accept connection |
| `Config` | Configuration error |
| `Shutdown` | Shutdown error |
| `ResourceLimit` | Resource limit exceeded |
| `PartialBind` | Some addresses failed to bind (see below) |

### Connection Errors

| Error | Description |
|-------|-------------|
| `Io` | I/O error |
| `ReadTimeout` | Read timeout exceeded |
| `WriteTimeout` | Write timeout exceeded |
| `IdleTimeout` | Idle timeout exceeded |
| `Reset` | Connection reset by peer (ECONNRESET) |
| `Closed` | Connection closed (EOF) |
| `BrokenPipe` | Broken pipe (EPIPE) |
| `NotConnected` | Socket not connected (ENOTCONN) |
| `WouldBlock` | Operation would block (should not occur in async) |
| `Protocol` | Protocol error |
| `Shutdown` | Shutdown requested |

### Error Recovery

| Error | Recovery Action |
|-------|-----------------|
| `EMFILE` (too many open files) | Log, reject new connections temporarily, back off |
| `ENFILE` (system file table full) | Log, reject new connections temporarily, back off |
| `ECONNRESET` | Log at debug level, close connection |
| `ETIMEDOUT` | Close connection, log |
| `EPIPE` | Close connection, log at debug level |
| `ENOTCONN` | Close connection, log at debug level |
| `EWOULDBLOCK` / `EAGAIN` | Retry (handled by Tokio automatically) |
| `EINTR` | Retry operation (handled by Tokio automatically) |
| `ECONNREFUSED` | Log, close connection |
| Temporary errors | Retry with exponential backoff (max 5 retries) |

### Partial Bind Failure

When binding to multiple addresses, the server behavior is configurable:

| Mode | Behavior |
|------|----------|
| `FailFast` (default) | Fail if any address fails to bind |
| `BestEffort` | Continue with successfully bound addresses, fail only if all fail |

When `BestEffort` is used:
- Log WARNING for each failed bind with error details
- Log INFO for each successful bind
- Return `PartialBind` error with list of failed addresses (can be ignored)
- Server operates on successfully bound addresses

## Logging Specification

### Log Levels

| Level | Usage |
|-------|-------|
| ERROR | Fatal errors, server cannot continue |
| WARN | Recoverable errors, degraded operation |
| INFO | Significant events (start, stop, config) |
| DEBUG | Connection events, detailed flow |
| TRACE | Packet-level details, verbose debugging |

### Connection Lifecycle Logs

| Event | Level | Required Fields |
|-------|-------|-----------------|
| Connection accepted | DEBUG | `conn_id`, `remote_addr`, `local_addr` |
| Connection established | DEBUG | `conn_id`, `remote_addr`, `local_addr`, `duration_ms` (time to establish) |
| Connection active | TRACE | `conn_id`, `state` |
| Connection idle | TRACE | `conn_id`, `idle_since` |
| Connection draining | DEBUG | `conn_id`, `reason` |
| Connection closed (normal) | DEBUG | `conn_id`, `remote_addr`, `duration_ms`, `bytes_read`, `bytes_written`, `reason` |
| Connection closed (error) | WARN | `conn_id`, `remote_addr`, `duration_ms`, `bytes_read`, `bytes_written`, `error`, `reason` |
| Connection timeout | DEBUG | `conn_id`, `timeout_type`, `timeout_value` |

**Connection accepted log fields:**

| Field | Type | Description |
|-------|------|-------------|
| `conn_id` | u64 | Unique connection identifier |
| `remote_addr` | String | Remote address (IP:port or "unix") |
| `local_addr` | String | Local address |
| `listener` | String | Listener address that accepted |

**Connection closed log fields:**

| Field | Type | Description |
|-------|------|-------------|
| `conn_id` | u64 | Unique connection identifier |
| `remote_addr` | String | Remote address |
| `duration_ms` | u64 | Connection duration in milliseconds |
| `bytes_read` | u64 | Total bytes read |
| `bytes_written` | u64 | Total bytes written |
| `reason` | String | Close reason (see DisconnectReason) |
| `error` | String | Error message (if applicable) |
| `requests` | u64 | Number of requests processed (if applicable) |

### Server Lifecycle Logs

| Event | Level | Required Fields |
|-------|-------|-----------------|
| Server starting | INFO | `version`, `bind_addresses` |
| Server started | INFO | `bind_addresses`, `pid` |
| Listener bound | INFO | `address`, `backlog` |
| Listener bind failed | ERROR | `address`, `error` |
| Shutdown signal received | INFO | `signal`, `signal_name`, `reason` |
| Second shutdown signal received | WARN | `signal`, `signal_name`, `action` (force shutdown) |
| Shutdown grace period started | INFO | `grace_period_ms` |
| Shutdown draining started | INFO | `active_connections`, `drain_timeout_ms` |
| Shutdown complete | INFO | `uptime_s`, `total_connections`, `total_bytes_read`, `total_bytes_written`, `exit_code` |
| Configuration loaded | INFO | `config_summary` |
| Configuration error | ERROR | `error`, `field` |

### Error Logs

| Event | Level | Required Fields |
|-------|-------|-----------------|
| Accept error | WARN | `listener`, `error`, `consecutive_errors` |
| Accept error (fatal) | ERROR | `listener`, `error` |
| Resource limit reached | WARN | `limit_type`, `current`, `max` |
| I/O error | DEBUG | `conn_id`, `operation`, `error` |
| Protocol error | WARN | `conn_id`, `error`, `details` |

### Log Fields

All log entries SHOULD include:

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | RFC3339 | Event timestamp with microseconds |
| `level` | String | Log level |
| `target` | String | Module path |
| `message` | String | Human-readable message |

## Monitoring and Observability

### Metrics

The server SHOULD expose the following metrics:

| Metric | Type | Description |
|--------|------|-------------|
| `connections_accepted_total` | Counter | Total connections accepted |
| `connections_active` | Gauge | Current active connections |
| `connections_closed_total` | Counter | Total connections closed |
| `connections_rejected_total` | Counter | Connections rejected (limit reached) |
| `connection_errors_total` | Counter (labeled) | Connection errors by type |
| `connection_duration_seconds` | Histogram | Connection duration distribution |
| `bytes_received_total` | Counter | Total bytes received |
| `bytes_sent_total` | Counter | Total bytes sent |
| `backlog_size` | Gauge | Current backlog size (approximate) |

### Health Check

Health status includes:

| Field | Type | Description |
|-------|------|-------------|
| `accepting` | bool | Server is accepting connections |
| `connections` | usize | Current connection count |
| `uptime` | Duration | Server uptime |
| `last_error` | Option\<String\> | Last error (if any) |
| `shutting_down` | bool | Shutdown in progress |
| `listeners` | Vec\<ListenerStatus\> | Status of each listener |

**Listener status:**

| Field | Type | Description |
|-------|------|-------------|
| `address` | String | Listener address |
| `bound` | bool | Successfully bound |
| `accepting` | bool | Currently accepting connections |

## API Design

### Server Builder

The server uses a builder pattern with the following methods:

| Method | Description |
|--------|-------------|
| `bind(addr)` | Add a bind address (can be called multiple times) |
| `backlog(size)` | Set listen backlog |
| `max_connections(count)` | Set maximum concurrent connections |
| `worker_threads(count)` | Set number of worker threads |
| `read_timeout(duration)` | Set read timeout |
| `write_timeout(duration)` | Set write timeout |
| `idle_timeout(duration)` | Set idle timeout |
| `tcp_nodelay(enabled)` | Enable/disable TCP_NODELAY |
| `keepalive(config)` | Set keepalive configuration |
| `grace_period(duration)` | Set shutdown grace period |
| `drain_timeout(duration)` | Set shutdown drain timeout |
| `bind_mode(mode)` | Set bind failure mode (FailFast/BestEffort) |
| `handler(handler)` | Set connection handler |
| `build()` | Build the server |

### Server Handle

The server handle provides:

| Method | Description |
|--------|-------------|
| `shutdown()` | Trigger graceful shutdown |
| `force_shutdown()` | Force immediate shutdown |
| `wait()` | Wait for server to stop |
| `status()` | Get current server status |
| `metrics()` | Get server metrics |
| `is_running()` | Check if server is running |

## Testing Requirements

### Test Coverage

| Category | Minimum Coverage |
|----------|------------------|
| Unit tests | 80% line coverage |
| Integration tests | All public APIs |
| Error paths | All error variants |
| Edge cases | Timeouts, limits, shutdown |

### Unit Tests

- Configuration validation
- Socket address parsing
- Error handling
- Timeout behavior
- State machine transitions
- Connection ID generation

### Integration Tests

- Accept connections on all socket types (Unix, IPv4, IPv6)
- Handle concurrent connections (100+)
- Graceful shutdown with active connections
- Timeout enforcement (read, write, idle)
- Reconnection after server restart
- Partial bind failure handling
- Resource limit enforcement

### Load Tests

- Sustain 10,000 concurrent connections for 1 hour
- Handle connection churn (1000 connect/disconnect per second)
- Memory stability under load (no leaks)
- Latency percentiles under various loads (p50, p95, p99)

### Chaos Tests

- Connection drops (random)
- Network partitions (for TCP)
- Resource exhaustion (fd, memory)
- Signal handling during high load
- Slow clients (slowloris-style)

## Security Considerations

1. **Input validation**: Validate all configuration parameters
2. **Resource limits**: Enforce limits to prevent DoS
3. **Socket permissions**: Restrict Unix socket access (0o660 default)
4. **No root required**: Server should not require root privileges
5. **Timeout enforcement**: Prevent slowloris-style attacks
6. **Connection limits per source**: Optional rate limiting
7. **Path traversal**: Validate Unix socket paths

## Implementation Notes

### Tokio Best Practices

1. Use `tokio::select!` for cancellation-safe operations
2. Prefer `tokio::io::copy` for efficient data transfer
3. Use `BufReader`/`BufWriter` for buffered I/O
4. Avoid blocking operations in async context
5. Use `tokio::task::spawn_blocking` for CPU-intensive work

### Memory Efficiency

1. Reuse buffers where possible
2. Limit per-connection buffer sizes
3. Use `bytes::Bytes` for zero-copy operations
4. Consider memory pools for frequent allocations

### File Descriptor Management

1. Set appropriate `ulimit` for production (at least 10000 + headroom)
2. Monitor fd usage
3. Clean up fds on connection close
4. Handle fd exhaustion gracefully with backoff

## Dependencies

| Crate | Purpose | Version |
|-------|---------|---------|
| `tokio` | Async runtime | 1.x |
| `tokio-util` | Utilities (CancellationToken, etc.) | 0.7.x |
| `socket2` | Advanced socket options | 0.5.x |
| `tracing` | Structured logging | 0.1.x |
| `thiserror` | Error types | 1.x |
| `bytes` | Efficient byte handling | 1.x |

## Future Considerations

- **TLS support**: Optional TLS termination
- **Connection pooling**: For milter client connections
- **Hot reload**: Configuration reload without restart (SIGHUP)
- **Clustering**: Multiple server instances coordination
- **Metrics export**: Prometheus/OpenTelemetry integration

## References

- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [C10K Problem](http://www.kegel.com/c10k.html)
- [Linux Socket Programming](https://man7.org/linux/man-pages/man7/socket.7.html)
- [Graceful Shutdown Patterns](https://tokio.rs/tokio/topics/shutdown)
