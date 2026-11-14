# Keep the leading space of header values

This guide follows [api-specification.md](../reference/api-specification.md#negotiation-methods).

By default, Sendmail and Postfix remove one space from the front of each header value before your handler sees it, and add one in front of each value your handler adds or changes. This guide shows how a handler that needs the exact bytes, such as a DKIM verifier or signer, turns that off. [Whitespace in header values](../explanation/header-whitespace.md) explains with examples why the bytes change.

## Request the option

Return `HEADER_LEADING_SPACE` from `protocol_options()`. `header` then receives each value as the message has it after the colon, so the name, a colon, and the value make up the field byte for byte:

```rust
use rust_milter::{Decision, Handler, MessageContext, MilterText, ProtocolOptions, Result};

/// The header fields of one message, each as the message has it.
#[derive(Default)]
struct Fields(Vec<Vec<u8>>);

struct RawHeaders;

impl Handler for RawHeaders {
    type Session = ();
    type Message = Fields;

    fn protocol_options(&self) -> ProtocolOptions {
        ProtocolOptions::HEADER_LEADING_SPACE
    }

    async fn header(
        &self,
        ctx: &mut MessageContext<Self>,
        name: &MilterText,
        value: &MilterText,
    ) -> Result<Decision> {
        let mut field = name.as_bytes().to_vec();
        field.push(b':');
        field.extend_from_slice(value.as_bytes());
        ctx.message().0.push(field);
        Ok(Decision::Continue)
    }
}
```

For `Subject: test` the handler stores `Subject: test`, and for `Subject:test` it stores `Subject:test`.

## Write the space in headers you add

With the option in force, the MTA stores the value of `add_header`, `insert_header`, and `change_header` as you send it. Put the space after the colon in the value yourself:

<!-- doctest-setup
use rust_milter::{Decision, EomContext, Handler, Result};
struct Adds;
impl Handler for Adds {
type Session = ();
type Message = ();
async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
---
Ok(Decision::Continue)
}
}
-->
```rust
ctx.add_header("X-Checked", " yes").await?;
```

The message then holds `X-Checked: yes`. Without the leading space in the value, it holds `X-Checked:yes`. The handler still needs `Actions::ADD_HEADERS` from `actions()`, as it does without the option.

## Keep the MTA at protocol version 6

Only protocol version 6 defines the option. Sendmail offers it on every connection. For Postfix, keep `milter_protocol` at 6, its default:

```text
milter_protocol = 6
```

If the MTA does not offer the option, rust-milter closes the connection during option negotiation, and the MTA applies its setting for a failed milter, such as Postfix's `milter_default_action`. rust-milter writes the "Connection closed" log at WARN, with this `error`:

```text
protocol error: the MTA did not offer the options ProtocolOptions(HEADER_LEADING_SPACE) at protocol version 6
```

If you see it, check the MTA's protocol version before you check the handler.
