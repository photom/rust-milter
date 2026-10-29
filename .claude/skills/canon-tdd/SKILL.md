---
name: canon-tdd
description: Kent Beck's Canon TDD workflow (test list, then red-green-refactor). Use when implementing a feature, fixing a bug, or adding tests.
---

# Canon TDD

Follow Kent Beck's Canon Test-Driven Development for every feature and fix.

## Start with a test list

Write the test list before any code. Kent Beck treats this as the most important step.

1. List every behavior the change needs: variations, edge cases, and expected failures. A plain checklist is enough.
2. Pick the test that is easiest to implement or that will teach you the most about the problem.
3. When you discover a new case during implementation, add it to the list and keep working on the current test.

## Red, green, refactor

Repeat this cycle for each test on the list.

1. Red. Write one test for the smallest next behavior. Run it and confirm it fails with the failure you expect, either a compile error or an assertion. Do not continue until you see that failure.
2. Green. Write the simplest code that makes the test pass. Hardcoded values and constants are fine at this stage. Do not design ahead of the tests.
3. Refactor. With all tests passing, improve the structure without changing behavior: remove duplication, rename things to match the Ubiquitous Language, and extract functions or types where SOLID calls for it. Run the tests again afterwards.
4. Cross the test off the list and pick the next one.

## Principles

- Tests check what the code does. They should survive a rewrite of how it does it, so avoid mocking internal details.
- Keep steps small. If a test takes more than a few minutes to write or to pass, split it.
- A test that is hard to write or set up points to a design problem, usually high coupling or low cohesion.
- Write production code only to make a failing test pass.

## Test strategy

### Method

- Write the assertion first, then work backward to the setup and the implementation.
- Prefer unit tests for pure functions.
- Use in-memory implementations to test repositories quickly and in isolation.
- Design for testability: inject dependencies and leave hooks where tests need them.

### Coverage

- Write unit tests for every public function, and run them after each task.
- Run a linter continuously.
- End-to-end tests must cover more than the happy path. Include error handling, boundary values, path traversal and URL encoding (`%xx`), unsupported or unauthorized methods, special characters, and invalid payloads.

### CI/CD

- Measure test coverage and code complexity in the CI pipeline.
- Automate SAST, DAST, and dependency vulnerability scans.
- Enable automated dependency updates and vulnerability alerts.
- Have CI generate test result and coverage reports and publish them as documentation.

## When tests fail

### After adding a feature

1. Run the whole test suite.
2. Run the tests for the module you changed.

### When fixing a regression or bug

1. Run the failing module's tests and compare the implementation with the failing case.
2. Run the tests one at a time and trace the failure step by step. Do not guess at fixes.
3. Apply the fix. Add print or log statements to trace execution if you need them. Once the module's tests pass, remove every temporary debug statement.
4. Run the whole suite. Do not move to the next module while any regression remains.

## Related skills

- design-master sets the structural boundaries (DDD, Clean Architecture) that TDD works within.
- docs-first documents the architecture that the tests implement.
- rust-master covers Rust implementation patterns and unit testing.
- external-integration covers tests for external APIs and mocked network responses.
