# Write Rust in the procedural style

This guide follows [ADR 0008](../adr/0008-rust-code-is-written-in-a-procedural-style.md), [ADR 0009](../adr/0009-no-stored-closures-or-function-pointers.md), [ADR 0010](../adr/0010-builder-factory-type-parameter-and-loader-trait.md), and the [style check](../reference/test-environment.md#style-check).

rust-milter's code branches with `match`, `if let`, and `let ... else`, returns errors with `?`, and repeats with loops. It calls no method that takes a function, writes no closures outside the builder, and has no `Fn` bounds or function pointer types. This guide shows the rewrite for each pattern the checks reject.

## Check your code

Run the style check and the Clippy run that reports the disallowed methods, as in [Run the style check](run-the-tests.md#run-the-style-check). Both check every Rust file of the repository.

## Replace an Option combinator

Instead of `name.map(|name| name.len())`, match on the option:

```rust
fn name_length(name: Option<&str>) -> Option<usize> {
    match name {
        Some(name) => Some(name.len()),
        None => None,
    }
}
```

Instead of `value.ok_or_else(|| ConfigError::MissingPort)?`, take the value with `let ... else` and return the error yourself:

```rust
#[derive(Debug)]
enum ConfigError {
    MissingPort,
}

fn port(value: Option<u16>) -> Result<u16, ConfigError> {
    let Some(port) = value else {
        return Err(ConfigError::MissingPort);
    };
    Ok(port)
}
```

When several fields need the same error, write one named function that turns `None` into it, and call it for each field:

```rust
#[derive(Debug)]
enum DecodeError {
    Malformed { command: u8 },
}

fn required<T>(decoded: Option<T>, command: u8) -> Result<T, DecodeError> {
    match decoded {
        Some(value) => Ok(value),
        None => Err(DecodeError::Malformed { command }),
    }
}
```

Instead of `line.first().is_some_and(|byte| byte.is_ascii_digit())`, use `if let`, or `matches!` with a guard:

```rust
fn starts_with_digit(line: &[u8]) -> bool {
    if let Some(first) = line.first() {
        return first.is_ascii_digit();
    }
    false
}

fn starts_with_digit_too(line: &[u8]) -> bool {
    matches!(line.first(), Some(first) if first.is_ascii_digit())
}
```

The library builds with Rust 1.85, which has no let chains. In `src/`, nest the `if let` and the condition. The test crates build with Rust 1.89 and may write `if let Some(first) = line.first() && first.is_ascii_digit()`.

## Replace a Result combinator

Instead of `std::fs::read_to_string(path).map_err(|error| LoadError::Io(error))?`, implement `From` for your error and use `?`:

```rust
#[derive(Debug)]
enum LoadError {
    Io(std::io::Error),
}

impl From<std::io::Error> for LoadError {
    fn from(error: std::io::Error) -> Self {
        LoadError::Io(error)
    }
}

fn load(path: &str) -> Result<String, LoadError> {
    let text = std::fs::read_to_string(path)?;
    Ok(text)
}
```

When the error needs more than the source, such as the path, match on the result:

```rust
#[derive(Debug)]
enum LoadError {
    Read {
        path: String,
        source: std::io::Error,
    },
}

fn load(path: &str) -> Result<String, LoadError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(source) => Err(LoadError::Read {
            path: path.to_owned(),
            source,
        }),
    }
}
```

In a test, instead of `.unwrap_or_else(|error| panic!("cannot read {path}: {error}"))`, match and panic in the error arm:

```rust
fn read_fixture(path: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("cannot read {path}: {error}"),
    }
}
```

## Replace an iterator chain

Instead of `names.iter().map(|name| name.to_uppercase()).collect()`, push in a loop:

```rust
fn upper(names: &[&str]) -> Vec<String> {
    let mut upper = Vec::new();
    for name in names {
        upper.push(name.to_uppercase());
    }
    upper
}
```

Instead of `name.bytes().all(|byte| byte.is_ascii_graphic())` or `any`, loop and return as soon as the answer is known:

```rust
fn is_header_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    for byte in name.bytes() {
        if !byte.is_ascii_graphic() || byte == b':' {
            return false;
        }
    }
    true
}
```

Instead of `data.iter().position(|&byte| byte == 0)` or `find`, loop and `break` when you find it:

```rust
fn nul_position(data: &[u8]) -> Option<usize> {
    let mut end = None;
    for (index, byte) in data.iter().enumerate() {
        if *byte == 0 {
            end = Some(index);
            break;
        }
    }
    end
}
```

Instead of `filter` and `fold`, keep the condition and the running value in the loop:

```rust
fn total_of_large(sizes: &[u64]) -> u64 {
    let mut total = 0;
    for size in sizes {
        if *size > 1024 {
            total += size;
        }
    }
    total
}
```

Instead of `sort_by_key(|finding| (finding.line, finding.column))`, derive `Ord` with the fields in sort order and call `sort`:

```rust
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Finding {
    line: usize,
    column: usize,
}

fn in_source_order(mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort();
    findings
}
```

## Replace a closure in a variable

Instead of `let valid = |byte: u8| byte != b':';`, write a named function. When the closure captures a value, pass that value as a parameter:

```rust
fn is_digits(part: &str, max: usize) -> bool {
    if !(1..=max).contains(&part.len()) {
        return false;
    }
    for byte in part.bytes() {
        if !byte.is_ascii_digit() {
            return false;
        }
    }
    true
}

fn is_enhanced_code(parts: &[&str]) -> bool {
    parts.len() == 3
        && is_digits(parts[0], 1)
        && is_digits(parts[1], 3)
        && is_digits(parts[2], 3)
}
```

When a library takes a function, look for a form that takes a type instead. For example, `syn` parses a macro body with `parse_body::<T>()` for any `T` that implements `Parse`, so a small struct replaces the parser function that `parse_body_with` takes.

## Replace a stored callback or a function pointer

Instead of a table of function pointers, such as `[("name", check_name as fn(&str) -> bool)]`, or a `Box<dyn Fn>` field, name the cases with an enum and match on it:

```rust
#[derive(Debug, Clone, Copy)]
enum Check {
    NotEmpty,
    Ascii,
}

impl Check {
    fn passes(self, text: &str) -> bool {
        match self {
            Check::NotEmpty => !text.is_empty(),
            Check::Ascii => text.is_ascii(),
        }
    }
}

const CHECKS: [Check; 2] = [Check::NotEmpty, Check::Ascii];

fn passes_all(text: &str) -> bool {
    for check in CHECKS {
        if !check.passes(text) {
            return false;
        }
    }
    true
}
```

When the behavior comes from the caller, define a trait of your own and take a value that implements it.

## Pass a closure where one is allowed

The closures given to `ServerBuilder::handler`, OpenTelemetry's `with_callback`, `std::thread::spawn`, and `Once::call_once` are allowed. For the handler factory, pass a function by name when you have one, and a `move` closure when you already hold the value:

<!-- doctest-setup
use rust_milter::{Handler, HandlerContext, Server, ServerConfig};

struct MyHandler {
    greeting: String,
}

impl MyHandler {
    fn new(_ctx: &HandlerContext<()>) -> Self {
        MyHandler {
            greeting: String::from("hello"),
        }
    }
}

impl Handler for MyHandler {
    type Session = ();
    type Message = ();
}

async fn serve(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
---
Ok(())
}
-->
```rust
let by_name = Server::builder()
    .config(config.clone())
    .handler(MyHandler::new)
    .build()?;

let handler = MyHandler {
    greeting: String::from("welcome"),
};
let by_value = Server::builder()
    .config(config)
    .handler(move |_ctx| handler)
    .build()?;
```

A closure inside one of these closures is not allowed. The reload loader takes no closure: implement `ReloadLoader` on a type of your own, as in [Reload the configuration without a restart](reload-the-configuration.md#write-a-loader).

## Keep the methods that take no function

Methods that take no function stay, alone or chained: for example `unwrap_or`, `unwrap_or_default`, `ok`, `is_some`, `then_some`, `as_ref`, `iter`, `enumerate`, `skip`, `zip`, `rev`, `sum`, `count`, and `collect`. `for (index, line) in lines.iter().enumerate()` is a loop, and fine.

## Satisfy Clippy

- Clippy's `question_mark` lint rejects `let Some(end) = end else { return None; };`. Write `let end = end?;` on its own line.
- In `src/`, Clippy does not suggest let chains, because the library's minimum Rust version is 1.85. Nest the `if let` and the condition.
- The workspace allows the lints that suggest a closure, such as `manual_map`, `manual_find`, and `map_entry`, so a `match` or a loop does not trigger them.
