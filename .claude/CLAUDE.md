# Working rules

These rules apply to every task in this repository.

## 1. Instructions and reporting

- Follow the requirements and instructions you are given for the whole task.
- Report progress as you go. When you hit a problem or a blocker, tell the user at once and do not push on without them.

## 2. Problem solving

- When an error occurs, find the root cause yourself and propose a fix.
- When several approaches are possible, present them and say which one you recommend.
- If the problem lies outside the source code, for example in the environment, the configuration, or the requirements, say so.

## 3. Existing code

- Follow the existing code style, naming conventions, and architecture.
- Before a major architectural change, explain why and get approval.

## 4. Two-strike rule

- If a fix or a test fails twice in a row, stop and summarize where things stand.
- Do not retry the same logic a third time. Re-examine the approach and propose a new hypothesis or solution.

## Practices

- Start small and grow the change step by step.
- Avoid over-abstraction. Readable, maintainable code comes first.
- Prefer strong type definitions to complex runtime checks.
- Match the sophistication of the approach to the size of the task.
- Check the output or logs to confirm each command did what you expected.
- Follow test-driven development (see the canon-tdd skill).
- Apply the humanizer and stop-slop skills whenever you write or revise prose: skills, documentation, README files, commit messages, and PR descriptions. Keep code, identifiers, commands, and technical meaning unchanged.

## Documentation

- Maintain all documentation under Diátaxis (see the diataxis skill). Each document is exactly one type: tutorial (`docs/tutorials/`), how-to guide (`docs/how-to/`), reference (`docs/reference/`), or explanation (`docs/explanation/`). Do not mix types in one document; link between them instead.
- Record architecture decisions as ADRs in `docs/adr/`, outside the Diátaxis structure (see `docs/adr/README.md`). Do not rewrite an accepted ADR; supersede it with a new one.
- Write test scenarios in `docs/testing/`, outside the Diátaxis structure, as Given/When/Then tables with IDs that link to spec rules (see `docs/testing/README.md`). Add a scenario before writing its test.
- Write diagrams in Mermaid. Do not draw ASCII diagrams.
- Do not write code in documentation, except example code.
- Put example code only in tutorials (`docs/tutorials/`) and how-to guides (`docs/how-to/`). Reference documents and the README link to them instead.

## Security and confidentiality

### Restricted files and data

Do not read, modify, or expose the following. If a task requires changing one of them, contact the user first.

- `.env` files
- files under `src/env`
- secrets configuration, such as `*/config/secrets.*`
- private keys and certificates, such as `*/.pem` and `*/.key`
- any file that contains API keys, tokens, or credentials

### Security rules

- Do not hardcode secrets such as API keys or passwords. Read them from environment variables managed by a secret manager.
- Never commit sensitive files. Keep `.gitignore` up to date.
- Validate and sanitize all user input to prevent injection attacks such as SQL injection and XSS.
- Keep credentials and secrets out of logs, console output, and error messages.

### Vulnerability detection

- Run the standard static analysis (SAST) tools for the language, for example SonarQube, Snyk, Bandit, or ESLint-plugin-security.
- Scan third-party dependencies automatically and alert on known vulnerabilities (CVEs).
- Design with Zero Trust and least privilege, following cloud security best practices.
- Run the security automation (SAST, DAST, and audit logging) in the CI/CD pipeline.

## Architecture

- Zero Trust: trust no entity, internal or external, by default. Verify everything.
- Least privilege: grant each function only the access it needs.

## Skills

Skills do not load automatically. Invoke one when its domain knowledge is needed:

- `/canon-tdd`: Kent Beck's TDD workflow (red, green, refactor)
- `/rust-master`: Rust conventions, async, Tokio, concurrency
- `/procedural-rust`: the procedural style of ADRs 0008 to 0011, and how to check and fix code
- `/milter-protocol`: the Sendmail/Postfix Milter protocol, SMTP stages, RFCs
- `/design-master`: DDD, Clean Architecture, SOLID, system design
- `/docs-first`: documentation-first design with Mermaid diagrams
- `/diataxis`: technical documentation with the Diátaxis framework
- `/external-integration`: reliability patterns for external APIs and SaaS
- `/logging-principles`: structured logging, telemetry, data privacy
- `/humanizer`: remove AI writing patterns from prose
- `/stop-slop`: quick checks against AI writing patterns
