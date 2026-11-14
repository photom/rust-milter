# ADR 0020: Handlers request protocol options with a fixed method, and a missing option closes the connection

- Status: Accepted
- Date: 2026-11-13
- Affects: `Handler`, `Negotiated`, `ProtocolError`, the protocol engine's option negotiation

## Context

Sendmail and Postfix remove one leading space from each header value before they send it to a milter, unless the milter put `SMFIP_HDR_LEADSPC` in the protocol flags of its `SMFIC_OPTNEG` answer. The same flag decides whether they put a space in front of the value of a header that the milter adds, inserts, or changes. The engine answers with the `SMFIP_NO*` flags of the stages a handler leaves out and nothing else, so a handler cannot ask for the flag.

Without it, a handler receives `test` for both `Subject: test` and `Subject:test`. The "simple" header canonicalization of DKIM requires header fields "exactly as they are in the message being signed or verified" (RFC 6376 §3.4.1). A verifier behind rust-milter that assumes one removed space fails every simple signature over a field written without a space after the colon, and cannot tell when its guess was wrong. hallmark-milter, a DKIM and DKIM2 signer and verifier built on rust-milter, needs the flag (issue #61).

A handler declares what it needs with two synchronous methods, `events()` and `actions()`. libmilter also offers `xxfi_negotiate`, a callback that receives the MTA's offer and returns the flags to use. A second negotiation feature is pending: the macro list per stage, libmilter's `smfi_setsymlist`, which goes into the same `SMFIC_OPTNEG` answer. Both features should take the same API shape, so that the negotiation API changes once.

If a milter asks for a protocol flag that the MTA did not offer:
- libmilter closes the connection before it answers ("does not fulfill protocol requirements" in `st_optionneg`).
- Sendmail closes the connection, because the milter's protocol flags must be a subset of its offer (`milter_negotiate`).
- Postfix does not check. It applies `SMFIP_HDR_LEADSPC` from the answer even when its `milter_protocol` setting left the flag out of the offer.

Sendmail always offers version 6 and every protocol flag. Postfix offers `SMFIP_HDR_LEADSPC` only with `milter_protocol = 6`, its default.

## Options considered

- (a) One fixed method per negotiation feature: `protocol_options()` next to `events()` and `actions()`, and later one for the macro list. An option the MTA did not offer closes the connection, as a missing action does.
- (b) A negotiate callback that receives the MTA's offer and returns the events, actions, options, and macro lists, as `xxfi_negotiate` does. A handler could go on without an option and see in `negotiated()` which options are in force.
- (c) The same method as (a), but the engine drops an option the MTA did not offer, and the handler checks `negotiated().options()` in its callbacks.
- (d) No API change: handlers assume that the MTA removed one space.

## Decision

Option (a):

- `Handler` gets `protocol_options(&self) -> ProtocolOptions`, synchronous, with the default `ProtocolOptions::NONE`. `ProtocolOptions` is a flag set like `Events` and `Actions`. Its one flag is `HEADER_LEADING_SPACE`, with the value of `SMFIP_HDR_LEADSPC`. Options added later go in the same set.
- The engine adds the requested options to the protocol flags of its answer, next to the `SMFIP_NO*` flags.
- An option counts as offered when the MTA's protocol flags hold it and the negotiated version is 6, the only version that defines `SMFIP_HDR_LEADSPC`. The engine already treats `SMFIP_SKIP` this way. Protocol flags of 0 mean `SMFI_V1_PROT`, which holds no option.
- If the MTA did not offer an option, the engine closes the connection without an answer and reports the new error `ProtocolError::OptionsNotOffered(ProtocolOptions)`, which names the missing options. The engine checks the actions first, so a handler that misses an action and an option gets `ActionsNotOffered`.
- `Negotiated::options()` returns the options in force. `Negotiated::protocol()` still returns every protocol flag of the answer, options included.
- With `HEADER_LEADING_SPACE` in force, `header` receives each value as the MTA sent it, and `add_header`, `insert_header`, and `change_header` send each value as the handler gave it. A handler that wants a space after the colon writes it as the first character of the value.
- The macro list will be a fourth method of the same kind: synchronous, with a default that requests nothing, returning the macro names per `SMFIM_*` stage, which the engine appends to the same answer. Its limits and checks need no decision now, because that change is additive.
- `protocol_options()` runs inside `catch_unwind`, as every `Handler` method does ([ADR 0018](0018-panics-in-handler-code-and-the-reload-loader-are-caught.md)).

Option (b) gives handlers a fallback, but it adds a callback with the MTA's raw offer and a return type that overlaps `events()` and `actions()`. A DKIM verifier has no use for the fallback: without the flag it cannot tell whether the MTA removed a space, so a failed simple signature may be its own error. With option (c), a configuration error shows up only as wrong verification results. Option (d) breaks simple verification for fields written without a space, as the context describes.

## Consequences

Positive:
- A DKIM verifier receives header values with their whitespace as the message has it. A signer sends the exact bytes it hashed for the fields it adds, its own `DKIM-Signature` field included, which RFC 6376 §3.7 hashes with the header canonicalization of `c=`.
- Negotiation with an MTA that cannot keep the space fails at connection time, with a WARN log that names the missing option, before any message passes.
- Negotiation keeps one declarative shape: three methods, and a fourth for the macro list, each with a default. Existing handlers compile and behave as before.
- `ProtocolError` is `#[non_exhaustive]`, so the new variant is not a breaking change.

Negative:
- A handler cannot fall back. Behind Postfix with `milter_protocol` below 6, negotiation fails on every connection of a handler that requests `HEADER_LEADING_SPACE`, and Postfix applies `milter_default_action`, tempfail by default, to all mail.
- Postfix would apply the flag without offering it, and rust-milter still refuses that case, because the offer gives no sign that the MTA keeps the space.
- With the option in force, a handler that adds or changes a header writes the leading space itself, or the MTA stores `Name:value`.
- Neither MTA sends the whitespace between a field name and its colon, which the obsolete syntax of RFC 5322 §4.5 allows. The option cannot recover it, so a simple signature over such a field still fails.

## References

- Issue #61: Let a handler request `SMFIP_HDR_LEADSPC` to receive header values unchanged
- [api-specification.md, Negotiation methods](../reference/api-specification.md#negotiation-methods)
- [milter-protocol.md, Headers and body](../reference/milter-protocol.md#headers-and-body)
- RFC 6376, §3.4.1 and §3.7; RFC 5322, §2.2 and §4.5
- Sendmail 8.19.0.2: `libmilter/engine.c` (`st_optionneg`); `sendmail/milter.c` (`milter_negotiate`, `milter_headers`, `milter_addheader`, `milter_insheader`, `milter_changeheader`); `sendmail/headers.c` (`chompheader`)
- Postfix 3.12: `src/milter/milter8.c` (`milter8_connect`, `milter8_header`, `MILTER8_HDR_SPACE`)
