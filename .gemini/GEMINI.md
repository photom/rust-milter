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

## Security & Confidentiality

### 1. Restricted Files & Data
**Strict Prohibition**: Do not read, modify, or expose the following sensitive files/data. If changes are required, immediately contact the user.
- `.env` files
- Files within `src/env`
- Secrets configurations (e.g., `*/config/secrets.*`)
- Private keys and certificates (e.g., `*/.pem`, `*/.key`)
- Any files containing API keys, tokens, or authentication credentials.

### 2. Security Guidelines
- **No Hardcoding**: Never hardcode sensitive information (API keys, passwords, etc.). Use environment variables managed via secure secret managers.
- **Zero-Commit Policy**: Ensure sensitive files are never committed to version control. Maintain a robust `.gitignore`.
- **Input Validation**: Always validate and sanitize user input to prevent injection attacks (SQLi, XSS, etc.).
- **Output Sanitization**: Ensure credentials and secrets are never leaked in logs, console outputs, or error messages.

### 3. Vulnerability Detection & Automation
- **Static Analysis (SAST)**: Utilize standard security scanning tools for the specific programming language (e.g., SonarQube, Snyk, Bandit, ESLint-plugin-security).
- **Dependency Scanning**: Implement automated scanning for third-party libraries to detect and alert on known vulnerabilities (CVEs).
- **Modern Principles**: Apply Zero Trust architectures and Cloud Security Best Practices (Least Privilege) in all design decisions.
- **CI/CD Integration**: Incorporate security automation (SAST/DAST and audit logging) directly into the deployment pipeline.

## Architectural Alignment

- **Zero Trust**: Never trust any entity (internal or external) by default; verify everything.
- **Least Privilege**: Grant the minimum level of access required for a function to perform its task.
