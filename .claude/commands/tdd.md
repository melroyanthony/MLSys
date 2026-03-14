---
description: "Run TDD workflow: RED → GREEN → REFACTOR for a specific feature or module"
allowed-tools: Read, Write, Edit, Glob, Grep, Bash, Agent
---

# TDD Workflow

Execute Test-Driven Development for the specified scope.

## Input
$ARGUMENTS — Feature description or module path to apply TDD workflow to.

## Process

1. **Load TDD skill** from `.claude/skills/foundation/tdd/SKILL.md`
2. **Understand the requirement**: Read the feature description or relevant code
3. **Identify test file location**: Follow project conventions for test placement
4. **RED Phase**:
   - Write a failing test that describes the expected behavior
   - Run the test to confirm it fails
   - Show the failure output
5. **GREEN Phase**:
   - Write the minimum code to make the test pass
   - Run the test to confirm it passes
   - Show the pass output
6. **REFACTOR Phase**:
   - Review the implementation for code smells
   - Refactor while keeping tests green
   - Run all tests to confirm nothing broke
7. **Repeat** for each behavior in the feature
8. **Coverage check**: Run coverage report and verify thresholds from TDD skill

## Output
- Implementation code with matching tests
- Coverage report
- Summary of tests written and behaviors covered

## Working Directory
Use `solution/` as the base directory for all file operations.
