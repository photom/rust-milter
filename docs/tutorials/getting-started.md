# Getting started: build your first milter

Status: rust-milter is not implemented yet. This tutorial follows the designed API in [api-specification.md](../reference/api-specification.md) and will work once the library is released.

In this tutorial you build a milter that adds the header `X-Filtered-By: rust-milter` to every message, and connect it to Postfix.

## What you need

- Rust 1.85 or later
- a Postfix server you can reconfigure, on the same host
- an SMTP test client, such as `swaks`

## 1. Create the project

```sh
cargo new header-tagger
cd header-tagger
```

Add the dependencies to `Cargo.toml`:

```toml
[dependencies]
rust-milter = "0.1"
tokio = { version = "1", features = ["full"] }
tracing-subscriber = "0.3"
```

## 2. Write the handler

Replace `src/main.rs` with the handler. It answers option negotiation, continues every stage, and adds the header at end of message.

```rust
use rust_milter::protocol::SMFIF_ADDHDRS;
use rust_milter::{
    CancellationToken, Command, Connection, Handler, HandlerContext, Reply, Result, Server,
    ServerConfig,
};

#[derive(Clone)]
struct HeaderTagger;

impl HeaderTagger {
    fn new(_ctx: &HandlerContext<()>) -> Self {
        HeaderTagger
    }
}

impl Handler for HeaderTagger {
    async fn handle(&self, mut conn: Connection, shutdown: CancellationToken) -> Result<()> {
        loop {
            let frame = tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                frame = conn.read_frame() => frame?,
            };
            // `None` means the MTA closed the connection.
            let Some(frame) = frame else { return Ok(()) };

            let reply = match Command::from_frame(frame)? {
                Command::OptNeg { version, actions, .. } => {
                    // Close on versions below 2, or if the MTA cannot add headers.
                    if version < 2 || actions & SMFIF_ADDHDRS == 0 {
                        return Ok(());
                    }
                    Some(Reply::OptNeg {
                        version: version.min(6),
                        actions: SMFIF_ADDHDRS,
                        protocol: 0,
                        macros: Vec::new(),
                    })
                }
                // These commands get no reply.
                Command::Macro { .. } | Command::Abort | Command::QuitNewConnection => None,
                Command::Quit => return Ok(()),
                Command::BodyEob(_) => {
                    let header = Reply::AddHeader {
                        name: "X-Filtered-By".into(),
                        value: "rust-milter".into(),
                    };
                    conn.write_frame(&header.to_frame()?).await?;
                    Some(Reply::Continue)
                }
                _ => Some(Reply::Continue),
            };

            if let Some(reply) = reply {
                conn.write_frame(&reply.to_frame()?).await?;
                conn.flush().await?;
            }
        }
    }
}
```

This handler keeps no state between commands, so it has nothing to drop on `SMFIC_QUIT_NC`.

## 3. Configure and start the server

Add `main` below the handler:

```rust
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = ServerConfig {
        bind_addresses: vec!["127.0.0.1:8891".parse()?],
        max_connections: 1_000,
        ..ServerConfig::default()
    };

    let server = Server::builder()
        .config(config)
        .handler(HeaderTagger::new)
        .build()?;

    server.run().await?;
    Ok(())
}
```

Run it:

```sh
cargo run
```

The log shows `Listener bound address=127.0.0.1:8891`.

## 4. Connect Postfix

Add these lines to `/etc/postfix/main.cf`:

```
smtpd_milters = inet:127.0.0.1:8891
milter_protocol = 6
milter_default_action = accept
```

Reload Postfix:

```sh
sudo postfix reload
```

## 5. Send a test message

```sh
swaks --server 127.0.0.1 --to "$USER@localhost" --from test@example.org
```

Open the delivered message, for example in `/var/mail/$USER`. Its headers include:

```
X-Filtered-By: rust-milter
```

The milter log shows the connection being accepted and closed.

## Next steps

- [Reload the configuration without a restart](../how-to/reload-the-configuration.md) (how-to guide)
- [Export metrics with OpenTelemetry](../how-to/export-metrics-with-opentelemetry.md) (how-to guide)
