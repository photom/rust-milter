# Reload the configuration without a restart

Status: rust-milter is not implemented yet. This tutorial follows the designed API in [api-specification.md](../reference/api-specification.md) and [hot-reload.md](../reference/hot-reload.md).

In this tutorial you move the settings of the header tagger from [Getting started](getting-started.md) into a file, change them while the milter runs, and send `SIGHUP` to apply them. You will see one change take effect and one change wait for a restart.

## What you need

- the `header-tagger` project from [Getting started](getting-started.md)

## 1. Add the dependencies

```toml
[dependencies]
rust-milter = "0.1"
tokio = { version = "1", features = ["full"] }
tracing-subscriber = "0.3"
serde = { version = "1", features = ["derive"] }
toml = "0.8"
thiserror = "2"
```

## 2. Create the configuration file

Create `milter.toml` next to `Cargo.toml`:

```toml
listen = ["127.0.0.1:8891"]
backlog = 1024
max_connections = 1000
header_value = "rust-milter"
```

## 3. Write the loader

Add a file type, an error type, and a loader function to `src/main.rs`. The loader reads the file and returns both the server configuration and the handler's own configuration.

```rust
use std::net::SocketAddr;
use std::path::PathBuf;

use rust_milter::{Reloaded, ServerConfig};

#[derive(serde::Deserialize)]
struct FileConfig {
    listen: Vec<SocketAddr>,
    backlog: u32,
    max_connections: usize,
    header_value: String,
}

#[derive(Debug, Default)]
struct TaggerConfig {
    header_value: String,
}

#[derive(Debug, thiserror::Error)]
enum LoadError {
    #[error("cannot read the configuration file")]
    Read(#[from] std::io::Error),
    #[error("cannot parse the configuration file")]
    Parse(#[from] toml::de::Error),
}

async fn load(path: PathBuf) -> Result<Reloaded<TaggerConfig>, LoadError> {
    let text = tokio::fs::read_to_string(&path).await?;
    let file: FileConfig = toml::from_str(&text)?;
    Ok(Reloaded {
        server: ServerConfig {
            bind_addresses: file.listen,
            backlog: file.backlog,
            max_connections: file.max_connections,
            ..ServerConfig::default()
        },
        app: TaggerConfig {
            header_value: file.header_value,
        },
    })
}
```

## 4. Read the configuration in the handler

Give the handler a `ConfigWatch` and read it once per message, at `SMFIC_MAIL`. Drop the value when the message or the session ends.

```rust
use std::sync::Arc;

use rust_milter::protocol::SMFIF_ADDHDRS;
use rust_milter::{
    CancellationToken, Command, ConfigWatch, Connection, Handler, HandlerContext, Reply, Result,
    Server,
};

#[derive(Clone)]
struct HeaderTagger {
    config: ConfigWatch<TaggerConfig>,
}

impl HeaderTagger {
    fn new(ctx: &HandlerContext<TaggerConfig>) -> Self {
        HeaderTagger { config: ctx.config_watch() }
    }
}

impl Handler for HeaderTagger {
    async fn handle(&self, mut conn: Connection, shutdown: CancellationToken) -> Result<()> {
        // Settings for the current message only.
        let mut message_config: Option<Arc<TaggerConfig>> = None;

        loop {
            let frame = tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                frame = conn.read_frame() => frame?,
            };
            let Some(frame) = frame else { return Ok(()) };

            let reply = match Command::from_frame(frame)? {
                Command::OptNeg { version, actions, .. } => {
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
                Command::Mail(_) => {
                    message_config = Some(self.config.current());
                    Some(Reply::Continue)
                }
                Command::BodyEob(_) => {
                    if let Some(config) = message_config.take() {
                        let header = Reply::AddHeader {
                            name: "X-Filtered-By".into(),
                            value: config.header_value.clone(),
                        };
                        conn.write_frame(&header.to_frame()?).await?;
                    }
                    Some(Reply::Continue)
                }
                Command::Abort | Command::QuitNewConnection => {
                    message_config = None;
                    None
                }
                Command::Macro { .. } => None,
                Command::Quit => return Ok(()),
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

## 5. Register the loader

Load the file once at startup, then register the same loader for reloads:

```rust
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let path = PathBuf::from("milter.toml");
    let initial = load(path.clone()).await?;

    let server = Server::builder()
        .config(initial.server)
        .app_config(initial.app)
        .reload_with(move || load(path.clone()))
        .handler(HeaderTagger::new)
        .build()?;

    println!("pid {}", std::process::id());
    server.run().await?;
    Ok(())
}
```

Run it and note the printed process ID:

```sh
cargo run
```

## 6. Change a setting that applies at once

Edit `milter.toml`:

```toml
header_value = "rust-milter, reloaded"
max_connections = 500
```

Send `SIGHUP`:

```sh
kill -HUP <pid>
```

The log shows the reload with generation 2 and the changed settings:

```
INFO Reload applied generation=2 changed=["max_connections"] pending_restart=[]
```

`header_value` belongs to the application configuration, which rust-milter does not inspect, so it is not listed. Send a test message as in [Getting started](getting-started.md). The new message carries `X-Filtered-By: rust-milter, reloaded`.

## 7. Change a setting that needs a restart

Edit `milter.toml` again:

```toml
backlog = 2048
```

Send `SIGHUP` again. The listener on `127.0.0.1:8891` stays bound, so it keeps its old backlog. The log says so:

```
WARN Setting needs restart setting="backlog"
INFO Reload applied generation=3 changed=["backlog"] pending_restart=["backlog"]
```

The milter keeps running with the old backlog until you restart it. The list of settings that need a restart is in [configuration.md](../reference/configuration.md#settings-that-need-a-restart).
