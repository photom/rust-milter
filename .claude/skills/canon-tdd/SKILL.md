---
name: canon-tdd
description: Procedural guidance for applying Kent Beck's Canon Test-Driven Development (TDD) workflow. Use when implementing features, fixing bugs, or ensuring high test coverage and code quality through an iterative Red-Green-Refactor cycle.
---

# Canon TDD: Kent Beck's Test-Driven Development

This skill provides procedural guidance for implementing software features and fixes using Kent Beck's canonical Test-Driven Development methodology.

## Phase 0: The Test List (Test Plans)

**The most important part of Kent Beck's TDD process is writing the Test List first.** Before writing any code, you must brainstorm and document a list of all the tests you can think of that will fully specify the required behavior.

1. **Write the Test List**: Brainstorm all variations of the feature, edge cases, and expected failures. Write these down as a simple checklist.
2. **Select the Easiest Test**: Pick a test from the list that will be the easiest to implement or that will teach you the most about the problem. 
3. **Keep the List Updated**: As you discover new scenarios or edge cases during implementation, immediately add them to your Test List. Do not get sidetracked—put them on the list for later.

## Phase 1: The TDD Cycle (Red-Green-Refactor)

Once you have your Test List and have selected your first test, you must strictly adhere to this iterative cycle for all implementation tasks:

1. **Red (Write a failing test)**:
   - Identify the smallest possible next behavior required (based on your selected test from the Test List).
   - Write exactly one test for that behavior.
   - Run the test and verify that it fails (compilation failure or assertion failure). Do not proceed until you see a clear, expected failure.

2. **Green (Make it pass)**:
   - Write the simplest, most direct code possible to make the test pass.
   - Hardcoding or returning constant values is acceptable and encouraged at this stage if it satisfies the test.
   - Your only goal is to turn the test from Red to Green. Do not over-engineer.

3. **Refactor (Clean up)**:
   - With the tests passing (Green state), improve the code structure without changing its behavior.
   - Remove duplication (DRY).
   - Improve names to reflect the Ubiquitous Language.
   - Extract methods or classes to adhere to SOLID principles.
   - Ensure all tests still pass after refactoring.

4. **Cross it off**: Cross the completed test off your Test List and repeat the cycle with the next test.

## Core TDD Principles

- **Test Behavior, Not Implementation**: Tests should verify *what* the code does, not *how* it does it. Avoid over-mocking internal details.
- **Small Steps**: Keep the feedback loop extremely short. If a test takes more than a few minutes to write or pass, the step is too large. Break it down.
- **Listen to the Tests**: If a test is difficult to write or setup, it is a design smell indicating high coupling or low cohesion.
- **Never Write Production Code Without a Failing Test**: Production code is only written to satisfy a failing test.

## 🧪 Testing Strategy & Execution

### 1. Development Methodology
- **Assert-First**: Design tests by starting with the expected outcome and working backward to the implementation.
- **Prioritization**:
  - Prioritize unit tests for pure functions.
  - Use in-memory implementations for repository testing to ensure speed and isolation.
- **Design for Testability**: Integrate hooks and dependency injection into the architecture.

### 2. Comprehensive Test Suite
- **Unit Testing**: Provide unit tests for all public functions. Verify execution after every task completion.
- **Linting**: Use Linter tools for continuous static code analysis.
- **E2E Testing**: Beyond happy paths, comprehensively cover edge cases and attack vectors:
  - Error handling (Abnormal cases) and Boundary values.
  - Path Traversal and URL Encoding (`%xx` format).
  - Non-existent or unauthorized methods.
  - Special characters and invalid input payloads.

### 3. Automation & CI/CD Integration
- **Metrics**: Integrate measurement of test coverage and code complexity into the CI pipeline.
- **Security Scanning**: Automate SAST/DAST and dependency vulnerability scans.
- **Maintenance**: Enable automated package updates and vulnerability alerts within CI/CD.
- **Reporting**: Automatically generate and visualize test results and coverage reports as documentation.

## 🛠️ Failure Protocol (When Tests Fail)

### Scenario A: Adding New Features
1. Verify if the global test suite passes.
2. Specifically test the modified script or module after the update.

### Scenario B: Fixing Regressions/Bugs
1. **Identify**: Run the specific module test. Reference the implementation against the failing test case.
2. **Analyze**: Execute tests one by one. Analyze the failure step-by-step rather than applying "blind" fixes.
3. **Fix & Verify**: 
   - Apply the fix. Use print/log debugging to trace execution if necessary.
   - Confirm the module test passes.
   - **Crucial**: Remove all temporary debug prints once the fix is verified.
4. **Global Sync**: Run the entire test suite. Do not proceed to the next module until all regressions are resolved.

## Related Skills

- TDD operates within the structural boundaries established by the **design-master** skill (DDD, Clean Architecture).
- TDD follows the high-level architecture documented by the **docs-first** skill.
- For specific implementation patterns and unit testing in Rust, refer to **rust-master**.
- For testing external APIs and handling mocked network responses, use **external-integration**.