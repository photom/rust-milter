---
name: diataxis
description: Write technical documentation with the Diátaxis framework (tutorials, how-to guides, reference, explanation). Use when structuring, writing, or reviewing user-facing documentation, manuals, or READMEs.
---

# Diátaxis

Diátaxis sorts documentation into four types. Each type answers a different reader need at a different moment.

## The four types

Every document belongs to exactly one type. Do not mix them.

### Tutorials (learning)

- Goal: take a newcomer to basic competence through a guided lesson that succeeds.
- Focus: the learner's path.
- Rules:
  - Write a strict sequence of steps.
  - End with a meaningful result.
  - Leave out explanations, alternatives, and deep detail. Keep the path short and safe.

### How-to guides (tasks)

- Goal: show a reader who already has basic competence how to solve a specific problem.
- Focus: the task.
- Rules:
  - Write practical, ordered steps.
  - Assume the reader knows what they want to achieve.
  - Say how to do it. Leave the reasons to an explanation page.

### Reference (information)

- Goal: give accurate and complete technical information.
- Focus: the machinery, such as code, APIs, and configuration.
- Rules:
  - Organize it systematically, for example alphabetically or by component.
  - Keep it terse. No tutorials and no explanations.
  - Describe what things are: APIs, commands, configuration, schemas.

### Explanation (understanding)

- Goal: give background and context, and show how the system works inside.
- Focus: concepts.
- Rules:
  - Write discursively and analytically.
  - Cover why the system is the way it is and how it works under the hood.
  - Discuss design decisions, architecture, and history where they help.

## Workflow

1. Work out what the reader is trying to do: learn, solve a problem, look something up, or understand.
2. Pick the matching type.
3. Follow that type's rules. Keep tutorial steps out of reference pages, and keep explanation out of how-to guides.

## Related skills

- docs-first covers architecture design and internal component documentation before implementation.
- humanizer and stop-slop apply to all prose you write here.
