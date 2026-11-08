# Reload the configuration without a restart

This guide follows [api-specification.md](../reference/api-specification.md) and [hot-reload.md](../reference/hot-reload.md). rust-milter is not on crates.io yet, so the guide takes it from its GitHub repository.

This guide shows how to make a rust-milter server reread its configuration file on `SIGHUP`. It assumes you already have a working milter, such as the one from [Getting started](../tutorials/getting-started.md).

## Enable the serde feature

Add the `serde` feature to rust-milter, plus the crates for your file format and error type:

```toml
[dependencies]
rust-milter = { git = "https://github.com/photom/rust-milter", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
toml = "0.9"
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

The loader is a type of yours that implements `ReloadLoader`. Its `load` method reads the file and splits it into the server part and your part:

```rust
use std::path::PathBuf;

use rust_milter::{ReloadLoader, Reloaded, ServerConfig};

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

struct FileLoader {
    path: PathBuf,
}

impl ReloadLoader<TaggerConfig> for FileLoader {
    type Error = LoadError;

    async fn load(&self) -> Result<Reloaded<TaggerConfig>, LoadError> {
        load(self.path.clone()).await
    }
}
```

The `Error` type must implement `std::error::Error` and be `Send + Sync + 'static`. The startup code calls the plain `load` function too, so it reads the file the same way.

Keep the loader to reading and checking. rust-milter runs its own checks after the loader returns and can still reject the reload, for example when `backlog` changed. It cannot undo anything the loader did, so the loader must not replace values that other code reads, close connections, or start work based on the new settings. Return an error to reject settings your application cannot use; the reload then fails and nothing changes.

## Register the loader

Load the file once at startup, then register a loader for the same file for reloads:

<!-- doctest-setup
use std::path::PathBuf;

use rust_milter::{Handler, HandlerContext, ReloadLoader, Reloaded, Server, ServerConfig};

struct TaggerConfig;

#[derive(Debug, thiserror::Error)]
#[error("cannot load the configuration")]
struct LoadError;

async fn load(_path: PathBuf) -> Result<Reloaded<TaggerConfig>, LoadError> {
    Ok(Reloaded {
        server: ServerConfig::default(),
        app: TaggerConfig,
    })
}

struct FileLoader {
    path: PathBuf,
}

impl ReloadLoader<TaggerConfig> for FileLoader {
    type Error = LoadError;

    async fn load(&self) -> Result<Reloaded<TaggerConfig>, LoadError> {
        load(self.path.clone()).await
    }
}

struct HeaderTagger;

impl HeaderTagger {
    fn new(_ctx: &HandlerContext<TaggerConfig>) -> Self {
        HeaderTagger
    }
}

impl Handler for HeaderTagger {
    type Session = ();
    type Message = ();
}

async fn start() -> Result<(), Box<dyn std::error::Error>> {
---
let _ = server;
Ok(())
}
-->
```rust
let path = PathBuf::from("milter.toml");
let initial = load(path.clone()).await?;

let server = Server::builder()
    .config(initial.server)
    .app_config(initial.app)
    .reload_with(FileLoader { path })
    .handler(HeaderTagger::new)
    .build()?;
```

## Read your settings in the handler

Take a `ConfigWatch` from the handler context. Read it once per message, in `mail`, and keep the value in the handler's `Message` type. The library drops the `Message` value when the message ends or is aborted.

<!-- doctest-setup
struct TaggerConfig {
    header_value: String,
}
-->
```rust
use std::sync::Arc;

use rust_milter::{
    Actions, ConfigWatch, Decision, EomContext, Handler, HandlerContext, MessageContext, MilterText,
    Result,
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
        _sender: &MilterText,
        _args: &[MilterText],
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

## Act after a reload is applied

If something outside the handler depends on your settings, such as a cache built from them, rebuild it from a task that waits for new settings. `changed()` completes only when a reload has been applied, never for one that failed:

<!-- doctest-setup
use rust_milter::{Handler, Server};

struct TaggerConfig;

fn rebuild_cache(_config: &TaggerConfig) {}

fn watch_changes<H: Handler>(server: &Server<H, TaggerConfig>) {
---
}
-->
```rust
let mut watch = server.handle().config_watch();
tokio::spawn(async move {
    loop {
        watch.changed().await;
        let config = watch.current();
        // Rebuild what depends on the settings from `config`.
        rebuild_cache(&config);
    }
});
```

## Trigger a reload

Send `SIGHUP` to the milter process:

```sh
kill -HUP <pid>
```

Or reload from your own code, for example from an admin endpoint:

<!-- doctest-setup
async fn reload(server_handle: rust_milter::ServerHandle) -> Result<(), rust_milter::ReloadError> {
---
Ok(())
}
-->
```rust
let report = server_handle.reload().await?;
println!("generation {}, changed {:?}", report.generation, report.changed);
```

## Check the result

A successful reload logs the new generation and the changed settings:

```text
INFO Reload applied generation=2 changed=["max_connections"] listeners_added=[] listeners_removed=[]
```

Changes to your own `[tagger]` table are not listed; rust-milter does not inspect them.

## If a reload fails

The milter keeps running with the previous generation and logs the reason:

```text
ERROR Reload failed generation=3 error="cannot parse the configuration file: unknown field `max_conections`"
```

Fix the file and send `SIGHUP` again. Common causes:
- a misspelled key, rejected because unknown keys are errors
- a value outside its range, such as `max_connections = 0`
- a new listen address that cannot be bound, with `bind_mode = "fail_fast"`
- a changed setting that requires a restart, such as `backlog`. The error names it:

  ```text
  ERROR Reload failed generation=3 error="changing backlog requires a restart; nothing was applied"
  ```

  Restart the milter to apply the file, or set the value back and send `SIGHUP` again. The settings that require a restart are listed in [configuration.md](../reference/configuration.md#settings-that-need-a-restart).
