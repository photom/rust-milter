# Call a milter from Rust with the milter client

Status: the milter client is a design and is not implemented yet. This tutorial follows [milter-client.md](../reference/milter-client.md).

In this tutorial you send two test messages to the header tagger from [Getting started](getting-started.md) through the milter client, and see the second message reuse the first message's connection.

## What you need

- the header tagger from [Getting started](getting-started.md), running on `127.0.0.1:8891`
- a new Cargo project for the client

## 1. Create the project

```sh
cargo new tagger-client
cd tagger-client
cargo add rust-milter
cargo add tokio --features full
cargo add bytes
cargo add tracing-subscriber --features env-filter
```

## 2. Write a function that sends one message

Replace `src/main.rs`:

```rust
use bytes::Bytes;
use rust_milter::client::{ConnectInfo, Decision, MilterClient};

async fn send_one(client: &MilterClient, n: u32) -> Result<(), Box<dyn std::error::Error>> {
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
    session.header("Subject", &format!("test {n}")).await?;
    session.end_of_headers().await?;
    session.body(Bytes::from_static(b"Hello.\r\n")).await?;

    let eom = session.end_of_message().await?;
    println!("message {n}: decision {:?}", eom.decision);
    for change in &eom.modifications {
        println!("message {n}: {change:?}");
    }

    // QUIT_NC and back to the pool if the connection can be reused.
    session.end().await?;
    Ok(())
}
```

## 3. Send two messages

Add `main`:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("rust_milter=debug").init();

    let client = MilterClient::builder()
        .endpoint("127.0.0.1:8891".parse()?)
        .max_size(4)
        .build()?;

    send_one(&client, 1).await?;
    send_one(&client, 2).await?;

    client.shutdown().await;
    Ok(())
}
```

## 4. Run it

```sh
cargo run
```

The output shows the header the milter added to each message:

```
message 1: decision Continue
message 1: AddHeader { name: "X-Filtered-By", value: "rust-milter" }
message 2: decision Continue
message 2: AddHeader { name: "X-Filtered-By", value: "rust-milter" }
```

The debug log shows one new connection and one reuse:

```
DEBUG Connection opened conn_id=1
DEBUG Connection reused conn_id=1 sessions_served=1
```

The header tagger ends each message with `SMFIR_CONTINUE`, which leaves the connection reusable. A milter that ends messages with `SMFIR_ACCEPT` would show `Connection not reusable reason=mirror_state` instead, and the second message would open a new connection. [milter-client.md](../reference/milter-client.md#reuse-rules) explains why.
