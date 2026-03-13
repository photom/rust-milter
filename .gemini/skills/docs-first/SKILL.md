---
name: docs-first
description: Documentation first policy for designing software systems using visual diagrams (Mermaid) and descriptive narratives. Use when beginning a new feature or system design to establish high-level architecture before writing implementation code.
---

# Documentation First Policy

This skill enforces a "Documentation First" approach. Before any implementation code is written, the system's architecture, component interactions, and data structures must be thoroughly designed and documented using visual diagrams and narratives.

## Core Policy

1. **No Implementation Code First**: Strictly avoid writing implementation-specific programming code during the design phase. Focus entirely on concepts, boundaries, and data flow.
2. **Visual Communication**: Use Mermaid.js diagrams as the primary tool for representing architecture and flows.
3. **Descriptive Narratives**: Accompany all diagrams with clear, concise narratives explaining the *intent*, *responsibilities*, and *trade-offs* of the design.

## Required Documentation Artifacts

For any significant feature or system design, produce the following:

### 1. High-Level Architecture (C4 Model Context/Container Level)
Use Mermaid to create system context or container diagrams showing how the new system/feature fits into the broader landscape, its primary users, and external dependencies.

**Example Tool**: Mermaid `graph TD` or C4 PlantUML equivalents in Mermaid.

### 2. Component Interactions (Sequence Diagrams)
Document the expected flow of control and data between major components for key use cases.

**Mermaid Syntax**: `sequenceDiagram`
- Identify participants (actors, services, databases).
- Detail synchronous and asynchronous messages.

### 3. Data Structures & Domain Models (Class/Entity Diagrams)
Define the core entities, their attributes, and relationships without specifying language-level implementation details (like specific ORM annotations).

**Mermaid Syntax**: `classDiagram` or `erDiagram`
- Define Aggregates and Entities (referencing the Ubiquitous Language from DDD).
- Show cardinalities and relationships (1:1, 1:N, M:N).

### 4. State Transitions (State Diagrams)
If an entity has a complex lifecycle, document its states and the events that trigger transitions.

**Mermaid Syntax**: `stateDiagram-v2`

## Workflow

1. **Understand Requirements**: Gather and clarify functional and non-functional requirements.
2. **Draft Visuals**: Create Mermaid diagrams representing the architecture, interactions, and data.
3. **Write Narratives**: Explain the diagrams. Justify the architectural decisions.
4. **Review & Iterate**: Ensure the design meets the requirements and adheres to architectural principles before moving to implementation.

## Related Skills

- The designs produced here must adhere to the principles outlined in the **design-master** skill (Clean Architecture, SOLID).
- Once the documentation is approved, implementation proceeds using the **canon-tdd** skill.
- For authoring user-facing technical documentation (Tutorials, How-Tos, Reference, Explanation), trigger the **diataxis** skill.
- For diagramming systems that integrate with third-party APIs or external domains, refer to **external-integration**.