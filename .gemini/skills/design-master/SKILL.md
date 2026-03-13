---
name: design-master
description: Procedural guidance for architecting and designing software systems using Domain-Driven Design (DDD), Clean Architecture, SOLID, KISS, YAGNI, and DRY principles. Use when designing new systems, refactoring existing architectures, or establishing structural boundaries.
---

# Design Master: DDD, Clean Architecture, and Core Principles

This skill provides procedural guidance for designing robust, maintainable, and scalable software systems by applying Domain-Driven Design (DDD), Clean Architecture, and fundamental design principles (SOLID, KISS, YAGNI, DRY).

## Core Principles

Apply these principles rigorously during the design and architecture phases:

1. **SOLID Principles**:
   - **S**ingle Responsibility: A class/module should have one, and only one, reason to change.
   - **O**pen/Closed: Entities should be open for extension, but closed for modification.
   - **L**iskov Substitution: Objects should be replaceable with instances of their subtypes without altering program correctness.
   - **I**nterface Segregation: Many client-specific interfaces are better than one general-purpose interface.
   - **D**ependency Inversion: Depend upon abstractions, not concretions.

2. **KISS (Keep It Simple, Stupid)**: Favor simple, straightforward designs over complex ones. Complexity must be justified by requirements.

3. **YAGNI (You Aren't Gonna Need It)**: Do not add functionality until it is absolutely necessary. Avoid speculative generality.

4. **DRY (Don't Repeat Yourself)**: Every piece of knowledge must have a single, unambiguous, authoritative representation within a system.

## Domain-Driven Design (DDD) Workflow

DDD provides a structured approach to mapping business domains into software.

1. **Strategic Design (The Big Picture)**:
   - **Ubiquitous Language & Naming Conventions**: Establish a common language shared by domain experts and developers. **Rule**: This language must strictly dictate the naming conventions in your codebase. Class names, variables, and methods must exactly match the Ubiquitous Language without technical jargon padding.
   - **Domain Boundary**: Clearly define what is *inside* your domain and what is *outside*. This helps in focusing on the core problem and ignoring irrelevant details.
   - **Bounded Contexts**: The central pattern of DDD. It is the explicit boundary (both conceptual and often physical, like a module or microservice) within which a particular domain model applies and the Ubiquitous Language is strictly consistent. 
     - A model is only valid within its Bounded Context. 
     - Avoid sharing models across bounded contexts; instead, use DTOs, Anti-Corruption Layers, or Context Mapping to translate them.
   - **Context Mapping**: Define the relationships and integrations between bounded contexts (e.g., Anti-Corruption Layer, Open Host Service, Shared Kernel) across their boundaries.

2. **Tactical Design (The Implementation Details)**:
   - Identify the core domain components (see "DDD Tactical Patterns" below) that will reside within each bounded context.

## DDD Tactical Patterns

Use these patterns to construct your domain model within a Bounded Context:

- **Entities**: Objects that have a distinct identity that runs through time and different states (e.g., `User`, `Order`). Two entities with the same attributes but different IDs are different.
- **Value Objects**: Objects that describe characteristics or attributes but have no conceptual identity (e.g., `Address`, `Money`). They should be immutable. Two value objects with the same attributes are identical.
- **Aggregates**: A cluster of domain objects (Entities and Value Objects) that can be treated as a single unit. Every aggregate has a single **Aggregate Root** (an Entity). External objects can only hold references to the Aggregate Root.
- **Repositories**: Interfaces that abstract the storage and retrieval of Aggregates. They pretend to be an in-memory collection of aggregates.
- **Domain Services**: Operations or actions that belong to the domain but do not naturally fit within an Entity or Value Object. They are stateless.
- **Domain Events**: Objects that capture the memory of something interesting that has occurred in the domain (e.g., `OrderPlacedEvent`). Used to decouple side effects.

## Clean Architecture (Architectural Layers)

Structure your application into concentric layers. The overriding rule is **The Dependency Rule**: *Source code dependencies must only point inward, toward higher-level policies.*

1. **Domain Layer (Entities & Enterprise Rules)**:
   - The innermost layer. Contains the DDD Entities, Value Objects, Domain Events, and Domain Services.
   - It depends on absolutely nothing else in the system (no frameworks, no databases).

2. **Application Layer (Use Cases & Application Rules)**:
   - Orchestrates the flow of data to and from the domain entities, and directs those entities to use their critical business rules to achieve the goals of the use case.
   - Contains application services, input/output boundary interfaces (ports).
   - Depends only on the Domain Layer.

3. **Interface Adapters (Presentation & Controllers)**:
   - Converts data from the format most convenient for the Application and Domain layers, to the format most convenient for some external agency such as the Web or a database.
   - Contains Controllers, Presenters, Gateways, and implementations of Repositories.
   - Depends on the Application Layer.

4. **Infrastructure / Frameworks & Drivers**:
   - The outermost layer. Contains details like the database, the web framework, UI, external APIs, etc.
   - Keep this layer thin. It wires up the application.
   - Depends on the Interface Adapters layer.

## Related Skills

- For the visual representation of these designs without writing implementation code, trigger the **docs-first** skill.
- For the implementation phase applying these designs iteratively, trigger the **canon-tdd** skill.
- For applying these design patterns idiomaticaly in Rust, refer to **rust-master**.
- For architectural boundaries involving external APIs or SaaS, see **external-integration**.