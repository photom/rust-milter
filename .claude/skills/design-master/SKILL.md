---
name: design-master
description: Procedural guidance for architecting software systems using Domain-Driven Design (DDD) and Clean Architecture. Focuses on Ubiquitous Language enforcement, Bounded Context isolation, and strategic design workflow.
---

# Design Master: Strategic Design & Architecture Workflow

This skill provides procedural guidance for applying Domain-Driven Design and Clean Architecture patterns, with emphasis on enforcing domain language consistency and structural boundaries.

## Ubiquitous Language Enforcement (Critical Rule)

**Strict Naming Convention Policy:**
- Establish a common language shared by domain experts and developers
- **RULE**: This language must strictly dictate ALL naming in your codebase
- Class names, variables, and methods must **exactly match** the Ubiquitous Language
- **NO technical jargon padding** (e.g., use `Connection`, not `ConnectionHandler` if domain says "Connection")
- **NO abbreviations** unless the domain uses them
- Code should read like domain documentation

**Example:**
- Domain says "Mail Filter" → Use `MailFilter`, NOT `MailFilterProcessor` or `MFHandler`
- Domain says "SMTP Command" → Use `Command`, NOT `SmtpCommandMessage`

## Strategic Design Workflow

Follow this sequence when designing new systems or bounded contexts:

### Phase 1: Define Domain Boundaries
1. **Identify Core Domain**: What is the primary business problem being solved?
2. **Define Domain Boundary**: Clearly mark what is *inside* (your responsibility) vs. *outside* (external dependencies)
3. **Focus**: Concentrate effort on the core domain; treat everything else as supporting infrastructure

### Phase 2: Identify Bounded Contexts
1. **Define Boundaries**: Identify explicit boundaries where a particular domain model applies
2. **Context Consistency**: Within a Bounded Context, the Ubiquitous Language is strictly consistent
3. **Model Isolation**: A model is ONLY valid within its Bounded Context
4. **No Model Sharing**: Never share domain models across contexts; use DTOs or Anti-Corruption Layers instead

**Key Rule**: If two teams or subsystems use the same word to mean different things, they are in different Bounded Contexts.

### Phase 3: Context Mapping
Define relationships between Bounded Contexts:
- **Anti-Corruption Layer**: Protect your domain from external model changes
- **Open Host Service**: Expose a well-defined protocol/API for others to use
- **Shared Kernel**: Carefully shared subset of the domain model (use sparingly)
- **Customer/Supplier**: Upstream/downstream dependencies

### Phase 4: Tactical Patterns (Implementation)
Within each Bounded Context, identify:
- **Aggregate Roots**: Main entry points to clusters of domain objects
- **Repositories**: Abstractions for aggregate persistence
- **Domain Services**: Operations that don't naturally fit in entities
- **Domain Events**: Capture significant domain occurrences

**Note**: Use standard DDD patterns (Entities, Value Objects, Aggregates). Focus here is on applying them systematically, not defining them.

## Clean Architecture: The Dependency Rule

**Core Principle**: Source code dependencies must ONLY point inward, toward higher-level policies.

**Layer Structure** (innermost to outermost):
1. **Domain Layer**: Pure domain logic (entities, value objects, domain services) - NO dependencies
2. **Application Layer**: Use cases and orchestration - depends ONLY on Domain
3. **Interface Adapters**: Controllers, presenters, gateways - depends ONLY on Application
4. **Infrastructure**: Frameworks, databases, external APIs - depends on Interface Adapters

**Critical Rules**:
- Domain NEVER imports from Application, Adapters, or Infrastructure
- Use Dependency Inversion (interfaces in inner layers, implementations in outer layers)
- Keep Infrastructure layer thin (wiring only)

## Design Checklist

Before implementation, verify:
- [ ] Ubiquitous Language documented and enforced in naming
- [ ] Bounded Context boundaries clearly defined
- [ ] No domain models shared across contexts
- [ ] Context mapping documented (Anti-Corruption Layers identified)
- [ ] Aggregate Roots identified for each context
- [ ] Dependencies flow inward only (Domain → Application → Adapters → Infrastructure)

## Related Skills

- **docs-first**: Visualize designs with Mermaid diagrams before coding
- **canon-tdd**: Implement designs iteratively using TDD
- **rust-master**: Apply these patterns idiomatically in Rust
- **external-integration**: Handle boundaries with external systems