---
name: diataxis
description: Procedural guidance for writing technical documentation using the Diátaxis framework. Use when structuring, writing, or reviewing user-facing documentation, manuals, or readmes.
---

# Diátaxis: A Systematic Framework for Technical Documentation

This skill provides procedural guidance for organizing and writing technical documentation according to the Diátaxis framework. Diátaxis identifies four distinct types of documentation, each serving a specific user need at a specific time.

## The Four Documentation Types

Every piece of documentation must fit into exactly one of these four categories. Do not mix them.

### 1. Tutorials (Learning-Oriented)
- **Goal**: Allow a newcomer to achieve basic competence. Provide a guided, successful learning experience.
- **Focus**: The user's journey.
- **Rules**:
  - Must be strictly a step-by-step sequence of actions.
  - Must produce a meaningful result.
  - Omit explanations, alternatives, or deep dives. Keep it straightforward and safe.

### 2. How-To Guides (Problem-Oriented)
- **Goal**: Show a user who already has some basic competence how to solve a specific problem or complete a specific task.
- **Focus**: The task.
- **Rules**:
  - Must be practical and step-by-step.
  - Assume the user knows what they want to achieve.
  - Do not explain *why* it works; just explain *how* to do it.

### 3. Reference (Information-Oriented)
- **Goal**: Provide accurate and complete technical information.
- **Focus**: The machinery/code.
- **Rules**:
  - Must be structured systematically (e.g., alphabetical, by component).
  - Must be austere and to the point. No tutorials, no explanations.
  - Describes the "what": APIs, commands, configurations, schemas.

### 4. Explanation (Understanding-Oriented)
- **Goal**: Clarify, illuminate, and explain the background, context, or inner workings of the system.
- **Focus**: Concepts.
- **Rules**:
  - Broad, discursive, and analytical.
  - Discusses the "why" and "how it works under the hood".
  - May discuss design decisions, architecture, and historical context.

## Workflow

1. **Identify the Need**: Determine what the user is trying to accomplish (Learning, Solving, Finding info, or Understanding).
2. **Select the Type**: Choose the appropriate Diátaxis category.
3. **Write with Discipline**: Stick strictly to the rules of that category. Do not blend tutorials into reference material or explanations into how-to guides.

## Related Skills

- For the initial architectural design and internal component documentation before implementation, trigger the **docs-first** skill.