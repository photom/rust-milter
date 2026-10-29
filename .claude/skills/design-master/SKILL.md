---
name: design-master
description: Strategic design with Domain-Driven Design (DDD) and Clean Architecture, with strict Ubiquitous Language naming and Bounded Context isolation. Use when designing a new system or context, or when checking a design's boundaries and dependencies.
---

# Design Master

Apply DDD and Clean Architecture with two priorities: names follow the domain language, and boundaries stay intact.

## Ubiquitous Language

Domain experts and developers share one vocabulary, and that vocabulary decides every name in the code.

- Type, variable, and method names must match the Ubiquitous Language exactly.
- Do not pad names with technical words. If the domain says "Connection", write `Connection` and leave out `ConnectionHandler`.
- Do not abbreviate unless the domain does.
- Code should read like domain documentation.

Examples:

- Domain term "Mail Filter": use `MailFilter`. Avoid `MailFilterProcessor` and `MFHandler`.
- Domain term "SMTP Command": use `Command`. Avoid `SmtpCommandMessage`.

## Strategic design workflow

Work through these phases in order when you design a new system or Bounded Context.

### 1. Define the domain boundary

1. Name the core domain: the main business problem the system solves.
2. Mark what lies inside your responsibility and what is an external dependency.
3. Spend most of the effort on the core domain and treat the rest as supporting infrastructure.

### 2. Identify Bounded Contexts

1. Find the explicit boundaries where a given domain model applies.
2. Inside a Bounded Context, the Ubiquitous Language has one meaning per term.
3. A model is valid only inside its own Bounded Context.
4. Never share domain models across contexts. Translate with DTOs or an Anti-Corruption Layer.

If two teams or subsystems use the same word for different things, they belong to different Bounded Contexts.

### 3. Map the contexts

Define how Bounded Contexts relate:

- Anti-Corruption Layer: shields your domain from changes in an external model.
- Open Host Service: a published protocol or API that other contexts call.
- Shared Kernel: a small part of the model that two contexts share. Use it sparingly.
- Customer/Supplier: an upstream context that a downstream context depends on.

### 4. Apply tactical patterns

Inside each Bounded Context, identify:

- Aggregate Roots, the entry points to clusters of domain objects
- Repositories, which abstract aggregate persistence
- Domain Services, for operations that do not fit in an entity
- Domain Events, which record significant domain occurrences

Use the standard DDD building blocks (Entities, Value Objects, Aggregates). This phase is about applying them consistently; their definitions are out of scope here.

## Clean Architecture: the Dependency Rule

Source code dependencies point inward only, toward higher-level policy.

Layers from innermost to outermost:

1. Domain: entities, value objects, and domain services. No dependencies.
2. Application: use cases and orchestration. Depends only on Domain.
3. Interface Adapters: controllers, presenters, and gateways. Depends only on Application.
4. Infrastructure: frameworks, databases, and external APIs. Depends on Interface Adapters.

Rules:

- Domain never imports from Application, Interface Adapters, or Infrastructure.
- Use dependency inversion: define interfaces in inner layers and implement them in outer layers.
- Keep the Infrastructure layer thin. It only wires things together.

## Design checklist

Before implementation, confirm:

- [ ] The Ubiquitous Language is documented and the names follow it.
- [ ] Bounded Context boundaries are defined.
- [ ] No domain model is shared across contexts.
- [ ] The context map is documented, including any Anti-Corruption Layers.
- [ ] Each context has its Aggregate Roots identified.
- [ ] Dependencies point inward only: Infrastructure depends on Interface Adapters, Interface Adapters on Application, and Application on Domain.

## Related skills

- docs-first: draw the design as Mermaid diagrams before coding.
- canon-tdd: implement the design test by test.
- rust-master: apply these patterns in idiomatic Rust.
- external-integration: handle boundaries with external systems.
