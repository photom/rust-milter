# Reload the configuration without a restart

Status: rust-milter is not implemented yet. This guide follows the designed API in [api-specification.md](../reference/api-specification.md) and [hot-reload.md](../reference/hot-reload.md).

This guide shows how to make a rust-milter server reread its configuration file on `SIGHUP`. It assumes you already have a working milter, such as the one from [Getting started](../tutorials/getting-started.md).

## Enable the serde feature

Add the `serde` feature to rust-milter, plus the crates for your file format and error type:

```toml
[dependencies]
rust-milter = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
thiserror = "2"
```

## Lay out the configuration file

Put rust-milter's settings in one table and your handler's settings in another:

```toml
[server]
bind_addresses = ["127.0.0.1:8891"]
backlog = 1024
max_connections = 1000

[tagger]
header_value = "rust-milter"
```

Keys you leave out in `[server]` keep their defaults. The key names and value formats are listed in [configuration.md](../reference/configuration.md#file-format-serde-feature).

## Write a loader

The loader reads the file and splits it into the server part and your part:

```rust
use std::path::PathBuf;

use rust_milter::{Reloaded, ServerConfig};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    server: ServerConfig,
    tagger: TaggerConfig,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
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
        server: file.server,
        app: file.tagger,
    })
}
```

The loader's error type must implement `std::error::Error` and be `Send + Sync`.

## Register the loader

Load the file once at startup, then register the same loader for reloads:

```rust
let path = PathBuf::from("milter.toml");
let initial = load(path.clone()).await?;

let server = Server::builder()
    .config(initial.server)
    .app_config(initial.app)
    .reload_with(move || load(path.clone()))
    .handler(HeaderTagger::new)
    .build()?;
```

## Read your settings in the handler

Take a `ConfigWatch` from the handler context. Read it once per message, in `mail`, and keep the value in the handler's `Message` type. The library drops the `Message` value when the message ends or is aborted.

```rust
use std::sync::Arc;

use rust_milter::{
    Actions, ConfigWatch, Decision, EomContext, Handler, HandlerContext, MessageContext, Result,
};

struct HeaderTagger {
    config: ConfigWatch<TaggerConfig>,
}

impl HeaderTagger {
    fn new(ctx: &HandlerContext<TaggerConfig>) -> Self {
        HeaderTagger { config: ctx.config_watch() }
    }
}

impl Handler for HeaderTagger {
    type Session = ();
    // The settings that apply to the current message.
    type Message = Option<Arc<TaggerConfig>>;

    fn actions(&self) -> Actions {
        Actions::ADD_HEADERS
    }

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &str,
        _args: &[String],
    ) -> Result<Decision> {
        *ctx.message() = Some(self.config.current());
        Ok(Decision::Continue)
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        if let Some(config) = ctx.message().clone() {
            ctx.add_header("X-Filtered-By", &config.header_value).await?;
        }
        Ok(Decision::Continue)
    }
}
```

To read the settings once per SMTP session instead, store them in `Session` from the `connect` callback.

## Trigger a reload

Send `SIGHUP` to the milter process:

```sh
kill -HUP <pid>
```

Or reload from your own code, for example from an admin endpoint:

```rust
let report = server_handle.reload().await?;
println!("generation {}, changed {:?}", report.generation, report.changed);
```

## Check the result

A successful reload logs the new generation and the changed settings:

```
INFO Reload applied generation=2 changed=["max_connections"] pending_restart=[]
```

Changes to your own `[tagger]` table are not listed; rust-milter does not inspect them.

If `pending_restart` is not empty, the listed settings keep their running values until the next restart. Restart the milter when convenient. Which settings need a restart is listed in [configuration.md](../reference/configuration.md#settings-that-need-a-restart).

## If a reload fails

The milter keeps running with the previous generation and logs the reason:

```
ERROR Reload failed generation=3 error="cannot parse the configuration file: unknown field `max_conections`"
```

Fix the file and send `SIGHUP` again. Common causes:
- a misspelled key, rejected because unknown keys are errors
- a value outside its range, such as `backlog = 0`
- a new listen address that cannot be bound, with `bind_mode = "fail_fast"`
