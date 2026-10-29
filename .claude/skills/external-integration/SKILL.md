---
name: external-integration
description: Rules and reliability patterns for integrating with external systems, APIs, and third-party SaaS. Use when code crosses a domain boundary, to keep coupling loose, data consistent, retries safe, and external data validated.
---

# External system integration

Apply these rules when internal code talks to an external domain: a third-party API, a SaaS product, or a separate microservice.

## 1. Domain boundaries and consistency

- Keep internal and external domains separate, so that a design change or outage on the other side has limited effect on yours.
- Prefer eventual consistency over strong consistency for external integrations. It keeps your system running when the other side is slow or down.
- Treat external data as a reference. Store the external system's reference key in place of a copy of its data structure, so a change in their schema does not force a migration in yours.

## 2. Retrieval and caching

- For data you read rarely, fetch it on demand by reference key. The data stays fresh and the coupling stays low.
- For data you read often or that is mission critical, cache volatile data in a cache such as Valkey or Redis. If you cache in an RDBMS, keep the cache in its own tables or database, physically or logically.

## 3. Trust and validation

- Never trust an external response. Third-party and SaaS systems can return unexpected, malicious, or malformed data at any time.
- Log every request to and response from an external system so you can troubleshoot later. Mask PII, credentials, and oversized payloads (see logging-principles).
- Filter, normalize, and validate all incoming data. Log a validation failure as `ERROR` right away, because it can mean the provider changed the API without notice.

## 4. Reliability patterns

### Retries

- Retry on `5xx` status codes, connection errors, and timeouts.
- Default to 10 attempts at most.
- Use exponential backoff with jitter ("full jitter", for example), starting from a short interval under 10 seconds. Jitter keeps many clients from retrying in lockstep.
- If you use a fixed interval against an external SaaS system, wait 1 minute between attempts so transient load can clear.
- Treat `429 Too Many Requests` and `503` as retryable. When the response carries a `Retry-After` header (RFC 9110, RFC 6585), wait that long in place of the computed backoff.
- Retry automatically only for idempotent requests (`GET`, `PUT`, `DELETE`) or for requests that carry an idempotency key. Do not retry a plain `POST`.

### Safe writes

- Use `PATCH` for partial updates when the API offers it.
- If the API offers only `PUT`, `GET` the current state first and merge your changes into it. Otherwise you can delete attributes the provider added that your system does not know about yet.
- Send that `PUT` as a conditional request with `If-Match: <ETag from the GET>` (RFC 9110). If someone else changed the resource between your `GET` and `PUT`, you get `412 Precondition Failed`; re-fetch, merge again, and retry. Without the condition, their change is lost without any error.

### Failure tests

Write unit tests for the unlikely external cases: unexpected JSON schemas, empty responses, `403`, and `429` both with and without `Retry-After`.

## Checklist

- [ ] Store reference keys and do not copy external schemas.
- [ ] Retry 5xx, 429, and timeout errors with exponential backoff and jitter, and honor `Retry-After`.
- [ ] Log all external I/O with masking.
- [ ] Validate every field of an external response before you use it.
- [ ] Unit-test malformed and unexpected responses.

## Sources

- Enterprise Integration Patterns, messaging channels and decoupling: loose coupling and reference keys.
- AWS Architecture Blog, "Exponential Backoff and Jitter": retry strategy.
- Microsoft Azure design patterns, Anti-Corruption Layer: protecting the internal domain from external schema changes.

## Related skills

- design-master: architectural boundaries such as Anti-Corruption Layers.
- logging-principles: masked logging of external I/O.
- canon-tdd: testing these integrations.
- rust-master: Rust implementation, such as Tokio retries and Serde validation.
- docs-first: diagram the external connections before you build them.
- milter-protocol: external anti-spam or DNS APIs called from an MTA filter.
