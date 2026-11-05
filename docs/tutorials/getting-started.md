# Getting started: build your first milter

rust-milter is not on crates.io yet, so this tutorial takes it from its GitHub repository. The API it uses is specified in [api-specification.md](../reference/api-specification.md).

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
rust-milter = { git = "https://github.com/photom/rust-milter" }
tokio = { version = "1", features = ["full"] }
tracing-subscriber = "0.3"
```

## 2. Write the handler

Replace `src/main.rs` with the handler. The library runs the protocol with the MTA; the handler only adds the header at end of message.

```rust
use rust_milter::{
    Actions, Decision, EomContext, Handler, HandlerContext, Result, Server, ServerConfig,
};

struct HeaderTagger;

impl HeaderTagger {
    fn new(_ctx: &HandlerContext<()>) -> Self {
        HeaderTagger
    }
}

impl Handler for HeaderTagger {
    // This milter keeps no state per session or per message.
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::ADD_HEADERS
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.add_header("X-Filtered-By", "rust-milter").await?;
        Ok(Decision::Continue)
    }
}
```

Every other callback keeps its default and lets the message continue.

## 3. Configure and start the server

Add `main` below the handler:

<!-- doctest-setup
use rust_milter::{Handler, HandlerContext, Server, ServerConfig};

struct HeaderTagger;

impl HeaderTagger {
    fn new(_ctx: &HandlerContext<()>) -> Self {
        HeaderTagger
    }
}

impl Handler for HeaderTagger {
    type Session = ();
    type Message = ();
}
-->
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

```text
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
swaks --server 127.0.0.1 --to "$USER@localhost" --from test@void.invalid
```

Open the delivered message, for example in `/var/mail/$USER`. Its headers include:

```text
X-Filtered-By: rust-milter
```

## Next steps

- [Reload the configuration without a restart](../how-to/reload-the-configuration.md) (how-to guide)
- [Export metrics with OpenTelemetry](../how-to/export-metrics-with-opentelemetry.md) (how-to guide)
- [Export traces with OpenTelemetry](../how-to/export-traces-with-opentelemetry.md) (how-to guide)
