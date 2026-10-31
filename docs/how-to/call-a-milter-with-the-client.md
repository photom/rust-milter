# Call a milter with the milter client

Status: the milter client is a design and is not implemented yet. This guide follows [milter-client.md](../reference/milter-client.md).

This guide shows how to send SMTP sessions from your own Rust program to a milter, with connection reuse.

## Add the dependencies

```sh
cargo add rust-milter
cargo add tokio --features full
cargo add bytes
```

## Build a client

Build one client per milter endpoint and share it between tasks. It holds the connection pool:

```rust
use rust_milter::client::MilterClient;

let client = MilterClient::builder()
    .endpoint("127.0.0.1:8891".parse()?)
    .max_size(16)
    .build()?;
```

The pool and timeout settings are listed in [milter-client.md](../reference/milter-client.md#configuration).

## Run one SMTP session

Call the session methods in protocol order, apply the modifications from end of message to your copy of the message, and end the session:

```rust
use bytes::Bytes;
use rust_milter::client::{ConnectInfo, Decision};

let mut session = client.session().await?;

let info = ConnectInfo {
    hostname: "client.example.org".into(),
    address: Some("192.0.2.10:52000".parse()?),
};
if session.connect(&info).await? != Decision::Continue {
    session.end().await?;
    return Ok(());
}
session.helo("client.example.org").await?;
session.mail(&["<sender@example.org>"]).await?;
session.rcpt(&["<rcpt@example.net>"]).await?;
session.header("Subject", "test").await?;
session.end_of_headers().await?;
session.body(Bytes::from_static(b"Hello.\r\n")).await?;

let eom = session.end_of_message().await?;
apply_modifications(&eom.modifications);

session.end().await?;
```

Always call `end()`. A session dropped without it closes its connection instead of returning it to the pool.

## Send macros

Set macro values for a stage before the command of that stage. They apply to this session only:

```rust
use rust_milter::protocol::SMFIM_ENVFROM;

session.set_macros(SMFIM_ENVFROM, &[("{auth_authen}", "alice")]);
session.mail(&["<alice@example.org>"]).await?;
```

## Turn off connection reuse

For a milter that has not been reviewed for session isolation, turn reuse off. Every session then opens and closes its own connection, as Sendmail and Postfix do:

```rust
use rust_milter::client::ReusePolicy;

let client = MilterClient::builder()
    .endpoint("127.0.0.1:8891".parse()?)
    .reuse_policy(ReusePolicy::Never)
    .build()?;
```

## Check whether connections are reused

Run with debug logging for rust-milter and look for these lines:

```
DEBUG Connection reused conn_id=1 sessions_served=1
DEBUG Connection not reusable conn_id=2 reason=mirror_state
```

`reason=mirror_state` means the milter's last reply left it in a state where `SMFIC_QUIT_NC` is not allowed, usually because it ended the message with `SMFIR_ACCEPT`. The connection is closed and the next session opens a new one; [milter-client.md](../reference/milter-client.md#reuse-rules) lists all reasons.

## Shut the client down

When your program stops, shut the client down so it closes its pooled connections with `SMFIC_QUIT`:

```rust
client.shutdown().await;
```
