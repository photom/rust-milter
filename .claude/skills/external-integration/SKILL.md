---
name: external-integration
description: Guidelines and reliability patterns for integrating with external systems, APIs, and third-party SaaS. Use to ensure loose coupling, data consistency, resilience (retries/backoff), and secure data handling across domain boundaries.
---

# 🔌 External System Integration Principles

This skill provides architectural rules and implementation patterns to safely integrate internal systems with external domains (third-party APIs, SaaS, or separate microservices), ensuring resilience, loose coupling, and security.

## 1. Domain Boundaries & Consistency
- **Loose Coupling**: Maintain a clear separation between internal and external domains to minimize the impact of external design changes or failures.
- **Eventual Consistency**: Prioritize eventual consistency over strong consistency for external integrations to ensure system resilience.
- **Reference vs. Entity**: Treat external data as references rather than internal entities.
  - Store **Reference Keys** in your system instead of full external data structures.
  - This prevents external schema changes from forcing immediate internal database migrations.

## 2. Data Retrieval & Caching Strategy
- **Low Frequency Access**: Fetch data on-demand using reference keys to ensure data freshness and minimize coupling.
- **High Frequency / Mission Critical**: 
  - Implement Caching (e.g., Valkey/Redis) for volatile data.
  - If using an RDBMS for caching, separate tables/databases physically or logically to maintain decoupling.

## 3. Resilience & Risk Mitigation
- **Zero Trust Policy**: Never trust external responses. Assume that external systems (especially third-party/SaaS) can return unexpected, malicious, or malformed data at any time.
- **Logging**: Record all requests and responses for external systems to facilitate troubleshooting.
  - *Exception*: Mask PII, credentials, or excessively large payloads (see **logging-principles**).
- **Validation**: Apply strict filtering, normalization, and validation on all incoming data.
  - If validation fails, log it as an `ERROR` immediately, as it may indicate an unannounced API change.

## 4. Advanced Reliability Patterns
- **Retry Logic**: Implement retries for `5xx` status codes, connection errors, and timeouts.
  - **Max Retries**: Default to 10 attempts.
  - **Strategy**: Use Exponential Backoff starting at a short interval (e.g., <10s) to avoid slamming the external service.
  - If implementing fixed intervals, use a 1-minute delay for external SaaS systems to allow transient loads to subside.
- **Write Safety (GET before PUT)**: 
  - Prefer `PATCH` for partial updates.
  - If only `PUT` is available, `GET` the current state first and merge it with internal changes. This prevents accidental deletion of new attributes added by the external provider that your system does not yet recognize.
- **Failure Testing**: Mandate unit tests for "unlikely" external scenarios (e.g., unexpected JSON schemas, empty responses, or `403`/`429` errors).

## 🛠️ Implementation Checklist
- [ ] Use Reference Keys instead of duplicating external schemas.
- [ ] Implement Exponential Backoff for 5xx/Timeout errors.
- [ ] Log all External I/O (masked).
- [ ] Validate every field of an external response before processing.
- [ ] Write Unit Tests specifically for "Malformed/Unexpected Response" cases.

## 📚 References & Sources
These principles are based on enterprise integration patterns and resilience engineering:
- **Enterprise Integration Patterns**: Messaging Channels & Decoupling (Loose coupling and reference keys).
- **AWS Architecture Blog**: Exponential Backoff and Jitter (Retry logic best practices).
- **Microsoft Azure Design Patterns**: Anti-Corruption Layer (Protecting the internal domain from external schema changes).

## Related Skills
- For defining the architectural boundaries (like Anti-Corruption Layers), see **design-master**.
- For handling masked logging of External I/O, see **logging-principles**.
- For testing these robust integrations, follow **canon-tdd**.
- For Rust-specific implementation (Tokio Retries, Serde Validation), refer to **rust-master**.
- For diagramming these external connections before building, use **docs-first**.
- If integrating external anti-spam/DNS APIs into an MTA context, see **milter-protocol**.