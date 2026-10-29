---
name: docs-first
description: Design a feature or system in documentation first, with Mermaid diagrams and short narratives, before writing implementation code. Use when starting a new feature or system design.
---

# Docs first

Design the architecture, component interactions, and data structures in documents before anyone writes implementation code. Use diagrams and short narratives.

## Policy

1. Write no implementation code during design. Work on concepts, boundaries, and data flow.
2. Draw the architecture and flows as Mermaid diagrams.
3. Give every diagram a short narrative that states its intent, the responsibilities it assigns, and the trade-offs you accepted.

## Required artifacts

Produce these for any significant feature or system design.

### 1. High-level architecture (C4 context or container level)

Show where the new system or feature sits: its main users and its external dependencies.

Mermaid syntax: `flowchart TD` (the older `graph` keyword is only an alias), or Mermaid's native C4 diagrams (`C4Context`, `C4Container`) in place of C4-PlantUML. Mermaid still marks its C4 support as experimental, so keep those diagrams simple.

### 2. Component interactions (sequence diagrams)

Show the flow of control and data between the major components for each main use case.

Mermaid syntax: `sequenceDiagram`

- Name the participants: actors, services, databases.
- Show synchronous and asynchronous messages.

### 3. Data structures and domain models (class or entity diagrams)

Define the core entities, their attributes, and their relationships. Leave out language-level details such as ORM annotations.

Mermaid syntax: `classDiagram` or `erDiagram`

- Define Aggregates and Entities with names from the DDD Ubiquitous Language.
- Show cardinality (1:1, 1:N, M:N).

### 4. State transitions (state diagrams)

If an entity has a complex lifecycle, document its states and the events that move it between them.

Mermaid syntax: `stateDiagram-v2`

## Workflow

1. Gather the functional and non-functional requirements and resolve open questions.
2. Draw Mermaid diagrams for the architecture, interactions, and data.
3. Write the narratives and justify the architectural decisions.
4. Check the design against the requirements and the architecture principles before implementation starts.

## Related skills

- design-master: the design must follow Clean Architecture and SOLID.
- canon-tdd: implementation starts after the documents are approved.
- diataxis: user-facing documentation (tutorials, how-to guides, reference, explanation).
- external-integration: diagrams for systems that call third-party APIs or external domains.
- humanizer and stop-slop: apply both to every narrative.
