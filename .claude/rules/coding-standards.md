---
description: "Always-loaded coding standards for all languages"
globs: ["**/*.py", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"]
---

# Coding Standards

## General
- Functions should do ONE thing and do it well
- Maximum function length: 30 lines (excluding tests)
- Maximum file length: 300 lines (split if larger)
- Prefer composition over inheritance
- Use meaningful names: variables describe what, functions describe action
- No magic numbers — use named constants
- Error handling at system boundaries, trust internal code

## Python
- Use type hints for all function signatures
- Use `async def` for I/O-bound operations
- Use `uv` for package management (never pip directly)
- Follow PEP 8 naming: snake_case for functions/variables, PascalCase for classes
- Use Pydantic for validation at API boundaries
- Use `pathlib.Path` over `os.path`

## TypeScript
- Use strict mode (`"strict": true` in tsconfig)
- Prefer `interface` over `type` for object shapes
- Use `const` by default, `let` only when mutation is needed
- Avoid `any` — use `unknown` and narrow with type guards
- Use barrel exports sparingly (causes tree-shaking issues)

## Testing
- Test behavior, not implementation
- One assertion per test (conceptually)
- Use descriptive test names: "should [expected behavior] when [condition]"
- AAA pattern: Arrange, Act, Assert
- No logic in tests (no if/else, no loops)
