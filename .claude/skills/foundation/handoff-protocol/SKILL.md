---
name: handoff-protocol
description: |
  Structured handoff protocol for inter-agent communication. Defines the contract for how agents
  pass context, decisions, and artifacts between pipeline stages.
  Triggers on: "handoff", "inter-agent", "stage transition", "pass context", "agent contract".
allowed-tools: Read, Grep, Glob, Write, Edit, Bash
---

# Inter-Agent Handoff Protocol

## Handoff Document Format

Every agent must produce a handoff document when completing its stage:

```markdown
# Handoff: {source_agent} → {target_agent}
## Stage: {stage_number} — {stage_name}
## Status: COMPLETE | PARTIAL | BLOCKED
## Timestamp: {ISO 8601}

### Context
Brief summary of what was accomplished and the overall approach taken.

### Key Decisions
| Decision | Choice | Rationale | Alternatives Considered |
|----------|--------|-----------|------------------------|
| {what} | {chosen option} | {why} | {other options and why not} |

### Artifacts Produced
| File | Purpose | Status |
|------|---------|--------|
| {path} | {description} | {complete/partial/placeholder} |

### Validation Results
- {check}: {PASS/FAIL} — {details}

### Open Questions
- {question}: {context and impact if unresolved}

### Recommendations for Next Agent
- {specific actionable recommendation}

### Risks & Concerns
- {risk}: {severity} — {mitigation suggestion}
```

## Handoff Validation Rules

Before accepting a handoff, the receiving agent MUST verify:

1. **All required artifacts exist** and are non-empty
2. **Status is COMPLETE** (PARTIAL handoffs require orchestrator approval)
3. **No CRITICAL open questions** that block the next stage
4. **Validation results show no failures** on required criteria

## Stage-Specific Handoff Requirements

### Stage 1 → Stage 2 (PM → Architect)
Required artifacts:
- `requirements.md` with numbered requirements
- `mvp-scope.md` with acceptance criteria
- RICE scores for all features
- MoSCoW categorization

### Stage 2 → Stage 3 (Architect → Developers)
Required artifacts:
- `openapi.yaml` with all MVP endpoints
- `database-schema.md` with tables, relationships, indexes
- `system-design.md` with component diagram
- At least 2 ADRs (tech stack + key design decision)

### Stage 3 → Stage 4 (Developers → Tester)
Required artifacts:
- Working backend with all MVP endpoints
- Working frontend with all MVP pages
- `docker-compose.yml` that builds successfully
- Both Dockerfiles building without errors

### Stage 4 → Stage 5 (Tester → Finalization)
Required artifacts:
- All tests passing (unit + E2E)
- `stage-4-validation.md` with test report
- Docker Compose stack starts and stays healthy
- E2E happy path script executes successfully

## Handoff Best Practices
- **Be explicit about what's incomplete**: Don't hide partial work
- **Include exact file paths**: Not "the API file" but `solution/backend/app/api/routes.py`
- **Document failed approaches**: What was tried and why it didn't work
- **Provide runnable commands**: Not "run tests" but `cd solution/backend && uv run pytest -q`
- **Flag breaking changes**: If a decision changes the API contract, call it out prominently
