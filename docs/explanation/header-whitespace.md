# Whitespace in header values

Why a handler receives header values with or without their leading space, why DKIM needs the space, and why rust-milter closes the connection when the MTA cannot keep it. The rules are in [api-specification.md, Negotiation methods](../reference/api-specification.md#negotiation-methods) and [milter-protocol.md, Headers and body](../reference/milter-protocol.md#headers-and-body), and the decision is in [ADR 0020](../adr/0020-protocol-options-are-a-fixed-handler-method-and-close-when-missing.md).

## The space after the colon belongs to the value

A header field is a field name, a colon, and a field body (RFC 5322 §2.2). The space that most messages put after the colon is part of the field body. `Subject: test` and `Subject:test` are both valid fields, one byte apart.

Sendmail stores everything after the colon as the value. Before it sends a value to a milter, it removes the first character if that is a space, unless the milter asked for `SMFIP_HDR_LEADSPC`. Postfix does the same, with a comment in its source that Sendmail "by default eats one space (not tab) after the colon". A handler asks for `SMFIP_HDR_LEADSPC` by returning `HEADER_LEADING_SPACE` from `protocol_options()`.

## What the handler receives

In the table, TAB stands for a tab character.

| Field in the message | Value without `HEADER_LEADING_SPACE` | Value with `HEADER_LEADING_SPACE` |
|----------------------|--------------------------------------|-----------------------------------|
| `Subject: test` | `test` | ` test` (one space first) |
| `Subject:test` | `test` | `test` |
| `Subject:  test` (two spaces) | ` test` (one space first) | `  test` (two spaces first) |
| `Subject:` TAB `test` | TAB `test` | TAB `test` |

Without the option, the handler gets the same value from the first two fields, and nothing in it shows which field the message held. The MTAs remove a space but never a tab, so the last row arrives unchanged in both modes. With the option, the value is the field body byte for byte, and the handler can rebuild the field as `Subject:` followed by the value.

## What the MTA stores for a header the handler adds

The flag also works in the other direction. Without it, both MTAs put one space in front of each value that a handler sends with `add_header`, `insert_header`, or `change_header`. They add the space without looking at the value, so a value that already starts with a space ends up with two.

| Value the handler sends for `X-Added` | Stored without the option | Stored with the option |
|---------------------------------------|---------------------------|------------------------|
| `added` | `X-Added: added` | `X-Added:added` |
| ` added` (one space first) | `X-Added:  added` (two spaces) | `X-Added: added` |

With the option in force, a handler that wants the usual `Name: value` form sends the space as the first character of the value.

## Why DKIM needs the exact bytes

DKIM's "simple" header canonicalization requires header fields "exactly as they are in the message being signed or verified", and says that "whitespace MUST NOT be changed" (RFC 6376 §3.4.1).

Take a message whose signer used `c=simple/simple` and signed the field `Subject:test`. Without the option, a verifier behind rust-milter receives `test` and has to guess the field. If it assumes the MTA removed one space, it hashes `Subject: test`, which differs from what the signer hashed, and the signature fails. If it assumes the MTA removed nothing, it verifies this message and fails the signature of every message written `Subject: test`, the common form. A failed signature looks the same whether someone altered the message or the guess was wrong. With the option, the verifier receives `test` for this message and ` test` for the common form, and rebuilds both fields as their signers hashed them.

A signer has the same problem with the existing fields it signs. It also adds its own `DKIM-Signature` field, which RFC 6376 §3.7 hashes with the header canonicalization named in `c=`. Without the option, the signer has to hash that field with the space the MTA will add. With the option, it sends the bytes it hashed, and the MTA stores them.

The "relaxed" canonicalization deletes the whitespace before and after the colon (RFC 6376 §3.4.2), so the option makes no difference to a relaxed signature.

## What the option cannot recover

The obsolete syntax of RFC 5322 allows "any amount of white space" before the colon (§4.5), as in `Subject :test`. Sendmail ends the field name at the first space and drops the whitespace up to the colon, and Postfix removes the spaces and tabs between the name and the colon when it sends the header. The handler receives the name `Subject` in both modes, so a simple signature over such a field still fails.

## Why a missing option closes the connection

A handler that asks for `HEADER_LEADING_SPACE` depends on it in each `header` call. If rust-milter went on without the option, a DKIM verifier would receive stripped values while it treats them as raw ones, and failed signatures would be the only sign. rust-milter therefore treats a missing option like a missing action: it closes the connection during negotiation with `OptionsNotOffered`, and the WARN log names the option. The MTA then applies its own policy for a failed milter, such as Postfix's `milter_default_action`.

The option counts as offered only when the MTA offers `SMFIP_HDR_LEADSPC` and the negotiated version is 6. Sendmail offers it on every connection. Postfix offers it only with `milter_protocol = 6`, its default. Postfix does not check the milter's answer, so with a lower setting it would still apply the flag. rust-milter cannot see that from the offer, so it refuses that case too.
