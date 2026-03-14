---
name: debugging
description: "Root-cause analysis methodology, debugging techniques, CVE triage workflow, and regression prevention patterns. Use when investigating bugs, errors, incidents, or security vulnerabilities."
---

# Debugging & Investigation

## RCA Methodology

### 5 Whys
The simplest and most effective RCA technique. Keep asking "why" until you reach the true root cause (usually 3-5 levels deep).

**Rules:**
- Each "why" must be supported by evidence (not speculation)
- Stop when you reach something you can fix
- If the chain branches, follow the most impactful path

**Example:**
1. Why did the API return 500? → Unhandled null pointer in UserService
2. Why was the user null? → Database query returned no rows
3. Why were there no rows? → User was soft-deleted but the query didn't filter deleted users
4. **ROOT CAUSE**: Missing `WHERE deleted_at IS NULL` in the user lookup query

### Fault Tree Analysis
For complex issues with multiple contributing factors:

```
[System Failure]
├── [Hardware/Infra]
│   ├── Resource exhaustion
│   └── Network partition
├── [Software]
│   ├── Code defect ← most common
│   ├── Configuration error
│   └── Dependency bug
└── [Process]
    ├── Missing test coverage
    └── Deployment error
```

### Git Bisect
For regressions — find the exact commit that introduced the bug:
```bash
git bisect start
git bisect bad HEAD           # Current version is broken
git bisect good v1.2.0        # This version worked
# Git checks out a middle commit — test it
git bisect good               # or git bisect bad
# Repeat until the first bad commit is found
git bisect reset              # Cleanup
```

## Debugging Techniques

### Read the Error First
Seriously. Read the full error message and stack trace before touching code. 80% of bugs are obvious from the error.

### Reproduce Before Fixing
Never fix a bug you can't reproduce. If you can't reproduce it:
- Check environment differences (dev vs prod)
- Check data differences (test data vs real data)
- Check timing (race conditions, timeouts)
- Check load (only fails under concurrent requests)

### Binary Search Isolation
When the codebase is large, narrow down:
1. Is it frontend or backend?
2. Is it in the request handler or the service layer?
3. Is it in our code or a dependency?
4. Is it in this function or that function?

### Rubber Duck Debugging
Explain the code line-by-line to someone (or yourself). The act of explaining often reveals the bug.

## CVE Triage Workflow

### Step 1: Is It Applicable?
```
CVE reported for package X
├── Do we use package X? → Check pyproject.toml / package.json
│   └── No → Not affected, close
├── Do we use the affected version? → Check lock file
│   └── No → Not affected, close
├── Do we use the affected feature/code path?
│   └── No → Low risk, upgrade when convenient
└── Yes to all → Affected, proceed to Step 2
```

### Step 2: Assess Severity
| CVSS Score | Severity | Action |
|-----------|----------|--------|
| 9.0-10.0 | Critical | Fix immediately, hotfix if needed |
| 7.0-8.9 | High | Fix within current sprint |
| 4.0-6.9 | Medium | Fix within current release |
| 0.1-3.9 | Low | Fix when convenient |

### Step 3: Upgrade Path
1. Check if a patched version exists
2. Check for breaking changes between current and patched version
3. If major version upgrade required, check migration guide
4. If no patch exists, check for workarounds

### Step 4: Verify
1. Upgrade the dependency
2. Run full test suite
3. Run security audit to confirm CVE resolved
4. Check for new vulnerabilities introduced by the upgrade

## Regression Prevention

### After Every Bug Fix, Add:

1. **A test that reproduces the bug** (fails before fix, passes after)
2. **A guard in the code** (assertion, validation, type check)
3. **A monitoring check** (metric, log alert, health check) — for production bugs

### Test Naming for Bug Fixes
```python
def test_user_lookup_excludes_soft_deleted_users():
    """Regression test for #42: API returned 500 for soft-deleted users."""
    ...
```

```typescript
test('user lookup excludes soft-deleted users (fix #42)', () => {
  ...
});
```

## Common Bug Patterns

| Pattern | Symptom | Root Cause | Fix |
|---------|---------|-----------|-----|
| Null pointer | TypeError / NoneType | Missing null check at boundary | Validate inputs, use Optional types |
| Race condition | Intermittent failures | Shared mutable state | Locks, transactions, or immutable data |
| N+1 query | Slow under load | Lazy loading in a loop | Eager loading / joinedload |
| Off-by-one | Wrong count / missing item | Loop boundary error | Use range checks, write boundary tests |
| Stale cache | Shows old data | Cache not invalidated on write | Invalidate on mutation, use TTL |
| SQL injection | Data breach / errors | String concatenation in queries | Parameterized queries |
| Memory leak | OOM over time | Unreleased resources | Context managers, finalizers |
| Timezone | Wrong times displayed | Mixing naive and aware datetimes | Always use UTC internally |
