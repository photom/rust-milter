---
name: rust-master
description: Project-specific Rust conventions for naming, module organization, error handling, and enforcing Bounded Contexts through Rust's privacy and module system.
---

# Rust Master: Project Conventions & Standards

This skill defines project-specific conventions for Rust development, focusing on naming standards, module architecture, and enforcing domain boundaries.

## Naming Conventions (Ubiquitous Language)

### Standard Rust Naming
- **Types/Traits**: UpperCamelCase
- **Variables/Functions/Modules**: snake_case
- **Constants/Statics**: SCREAMING_SNAKE_CASE

### Domain Language Enforcement Rules
- **NO Stuttering**: Never repeat module names in type names
- **Match Domain Exactly**: Type names must match domain terminology without technical padding or suffixes
- **No Abbreviations**: Use full domain terms unless abbreviation is standard in the domain
- **Protocol Constants**: Preserve naming from specifications (e.g., Milter protocol constants as defined in spec)

## Error Handling Standards

### Project Convention
- Use type aliases for boxed dynamic errors at crate level
- Create custom error enums for domain-specific errors
- Never use unwrap/expect in production without safety comments
- Propagate errors with the question mark operator

## Async Development Rules

### Cancellation & Shutdown
- Always use tokio::select with CancellationToken for graceful shutdown
- Ensure all connection loops respect cancellation signals
- Handle partial operations during cancellation gracefully

### Executor Safety
- Never block the executor with synchronous I/O or CPU-heavy operations
- Use spawn_blocking for any blocking operations
- Never hold std::sync mutex guards across await points (makes future !Send)

### Testing Standards
- Use tokio::test for async unit tests
- Consider loom for testing concurrent primitives

## Enforcing Bounded Contexts via Modules

### Privacy Strategy
- **Privacy by Default**: Keep all types and fields private unless explicitly needed externally
- Use pub(crate) for internal crate visibility
- Use pub(super) for parent module visibility
- Only expose Aggregate Roots as public APIs

### Module Isolation Rules
- Group related domain objects within modules
- External modules interact ONLY with Aggregate Roots
- Internal implementation details remain private within modules

### Workspace Organization
- For strict Bounded Context boundaries, separate into distinct workspace crates
- Each crate represents a complete Bounded Context
- Minimize dependencies between context crates

## Tooling & Quality Requirements

### Mandatory Pre-Commit Checks
- Run cargo fmt to enforce consistent formatting
- Run cargo clippy and address ALL warnings
- Ensure all tests pass
- Document public APIs with rustdoc comments including examples

### Documentation Standards
- All public APIs must have rustdoc comments
- Include example usage blocks that serve as doctests
- Document safety requirements for unsafe code
- Explain non-obvious design decisions in module-level docs

## Related Skills

- **canon-tdd**: Apply TDD methodology to Rust development
- **design-master**: Understand Bounded Context architectural strategy
- **logging-principles**: Logging and observability standards for the project
- **external-integration**: Patterns for resilient external API integration