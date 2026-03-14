---
name: tdd
description: |
  Test-Driven Development workflow: RED → GREEN → REFACTOR.
  Write failing tests first, make them pass with minimal code, then refactor.
  Enforces 80%+ coverage. Triggers on: "TDD", "test first", "red green refactor", "write tests first".
allowed-tools: Read, Grep, Glob, Write, Edit, Bash
---

# Test-Driven Development (TDD)

## The TDD Cycle

### 1. RED — Write a Failing Test
- Write ONE test that describes the desired behavior
- Run it — it MUST fail (if it passes, the test is wrong or the feature already exists)
- The test should be specific and describe expected input/output
- Name the test descriptively: `test_should_return_404_when_user_not_found`

### 2. GREEN — Make It Pass
- Write the MINIMUM code to make the test pass
- Do not optimize, do not refactor, do not add extras
- Resist the urge to write "good" code — just make it work
- Run the test — it MUST pass

### 3. REFACTOR — Clean Up
- Now improve the code without changing behavior
- Remove duplication, improve naming, extract helpers
- Run ALL tests after each refactoring step — they MUST still pass
- Only refactor when tests are green

## Rules
1. **Never write production code without a failing test first**
2. **Write only enough test to fail** (compilation failure counts)
3. **Write only enough code to pass the failing test**
4. **Refactor only when all tests pass**
5. **Each cycle should take 2-5 minutes** (if longer, the step is too big)

## Coverage Targets

| Code Category | Minimum Coverage |
|---------------|-----------------|
| Business logic | 80% |
| API endpoints | 90% |
| Auth/Security | 100% |
| Financial/Payment | 100% |
| Utility/Helper | 70% |
| UI Components | 60% |

## Testing Patterns

### Python (pytest)

```python
# RED: Write the test first
def test_create_user_returns_user_with_id():
    user = create_user(name="Alice", email="alice@example.com")
    assert user.id is not None
    assert user.name == "Alice"
    assert user.email == "alice@example.com"

# GREEN: Minimal implementation
def create_user(name: str, email: str) -> User:
    return User(id=uuid4(), name=name, email=email)
```

### TypeScript (Vitest)

```typescript
// RED: Write the test first
test('createUser returns user with id', () => {
  const user = createUser({ name: 'Alice', email: 'alice@example.com' });
  expect(user.id).toBeDefined();
  expect(user.name).toBe('Alice');
  expect(user.email).toBe('alice@example.com');
});

// GREEN: Minimal implementation
function createUser(input: CreateUserInput): User {
  return { id: crypto.randomUUID(), ...input };
}
```

## Anti-Patterns to Avoid
- Writing tests after code (that's "test-after", not TDD)
- Writing multiple tests before making any pass
- Making large jumps in complexity between cycles
- Skipping the refactor step
- Testing implementation details instead of behavior
- Writing tests that depend on execution order

## When to Use TDD
- **Always**: Business logic, data transformations, API endpoints, validation rules
- **Sometimes**: UI components (use for complex interactions, skip for simple rendering)
- **Rarely**: Configuration, simple CRUD wrappers, generated code
