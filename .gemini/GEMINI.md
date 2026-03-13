---
name: core-principles
description: Foundational rules and best practices for development workflows. Use to enforce instructions, transparency, autonomous problem solving, respect for existing codebase, and failure protocols (like the 2-Strike Rule) during any task.
---

# Core Instruction Principles & Best Practices

This skill outlines the foundational rules and principles to govern behavior, problem-solving strategies, and coding practices throughout the development lifecycle.

## 1. Following Instructions & Transparency

- **Execution**: Strictly adhere to requirements and provided instructions throughout the development process.
- **Reporting**: Proactively report progress. If issues or blockers arise, notify the user immediately rather than proceeding blindly.

## 2. Autonomous Problem Solving

- **Analysis**: When errors occur, autonomously analyze the root cause and propose a solution.
- **Decision Making**: If multiple approaches exist, present the options and clearly state the recommended path.
- **Scope**: Identify if a problem lies outside the source code (e.g., environment, configuration, or logic flaws) and report it accordingly.

## 3. Respect for Existing Codebase

- **Consistency**: Follow the existing code style, naming conventions, and architectural patterns.
- **Refactoring**: Do not perform major architectural changes without explaining the rationale and seeking approval first.

## 4. Failure Protocol (2-Strike Rule)

- **Stagnation Prevention**: If a fix or test fails twice consecutively, stop and summarize the current situation.
- **Pivot**: Instead of repeating failed logic, re-evaluate the strategy and propose a new hypothesis or solution.

## Best Practices

- **Iterative Development**: Start small and expand incrementally.
- **Pragmatism**: Avoid over-abstraction; prioritize readability and maintainability.
- **Type Safety**: Prioritize robust type definitions over complex runtime logic.
- **Adaptive Complexity**: Scale the sophistication of the approach based on the complexity of the task.
- **Verification**: Always confirm that commands were executed as expected (check logs/output).
- **TDD**: Follow Test-Driven Development cycles to ensure reliability (see the **canon-tdd** skill).