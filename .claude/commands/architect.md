---
description: Design system architecture with scalability patterns, API specs, and database schema (Stage 2)
allowed-tools: Read, Glob, Grep, Write, WebFetch
---

# Solutions Architect

Transform MVP requirements into actionable technical specifications.

## Before You Start

**Read the system design knowledge base (in order):**
```
.claude/skills/foundation/system-design/APPROACH.md   # Start here - methodology
.claude/skills/foundation/system-design/SKILL.md      # Core concepts
.claude/skills/foundation/system-design/PATTERNS.md   # Implementation patterns
```

These contain:
- Design methodology (3-phase framework, anti-patterns to avoid)
- Scalability patterns (caching, sharding, replication)
- Trade-off frameworks (CAP, latency vs throughput)
- Implementation patterns (repository, service layer, etc.)
- ADR templates

---

## Stage 2 Flow

### Step 1: Analyze Scale Requirements (2 min)

Read MVP scope and estimate:
```
Users: [expected count]
Requests/day: [N users × M requests/user]
Peak RPS: [daily / 86400 × 10]
Data volume: [records × avg size]
Read/Write ratio: [estimate]
```

**Decision**: Single server or distributed?
- < 100 RPS: Single server ✓
- 100-1000 RPS: Consider caching
- > 1000 RPS: Need load balancing

### Step 2: Identify Trade-offs (2 min)

| Question | Options | Choose |
|----------|---------|--------|
| Can data be stale? | CP (consistency) / AP (availability) | |
| Interactive or batch? | Low latency / High throughput | |
| Complex or simple? | Feature-rich / MVP only | |

**Default for challenges**: CP, Low latency, MVP only

### Step 3: Select Patterns (3 min)

Based on requirements, check applicable:

**Data Access:**
- [x] Repository pattern (always)
- [ ] Caching (read-heavy, > 10:1 ratio)
- [x] Pagination (list endpoints)
- [ ] Soft delete (audit requirements)

**Resilience:**
- [x] Health checks (always)
- [ ] Timeouts (external APIs)
- [ ] Circuit breaker (unreliable dependencies)

**Security:**
- [ ] JWT authentication
- [ ] RBAC authorization
- [x] Input validation (Pydantic)

---

## Output Artifacts

Create in `solution/docs/`:

### 1. architecture/system-design.md
```markdown
# System Design Overview

## Scale Estimates
- Users: [N]
- RPS: [estimate]
- Data: [estimate]

## Patterns Selected
- [Pattern 1]: [reason]
- [Pattern 2]: [reason]

## Trade-offs
- Consistency over availability: [reason]
- [Other decisions]
```

### 2. architecture/workspace.dsl (C4)

Context and Container diagrams.

### 3. architecture/openapi.yaml

Complete API specification with:
- Health check endpoint
- CRUD for each resource
- Pagination parameters
- Error responses

### 4. architecture/database-schema.md

Tables, indexes, relationships, query patterns.

### 5. decisions/ADR-*.md (2-3 required)

Document key decisions:
- Database choice
- API design
- Caching strategy (if applicable)
- Security model

---

## Design Checklist

Before completing:
- [ ] Scale estimates documented
- [ ] Key trade-offs decided
- [ ] Patterns selected and documented
- [ ] All MVP features have API endpoints
- [ ] Database indexes match query patterns
- [ ] Health check endpoint defined
- [ ] Error responses standardized
- [ ] ADRs for major decisions

---

## Time Budget

| Task | Time |
|------|------|
| Scale analysis | 2 min |
| Trade-offs | 2 min |
| Pattern selection | 3 min |
| System design doc | 3 min |
| C4 diagram | 5 min |
| OpenAPI spec | 10 min |
| Database schema | 5 min |
| ADRs | 5 min |
| **Total** | **35 min** |

---

## Handoff Summary

```
🏗️ Architecture Complete

Scale: [N] users, [M] RPS estimated
Patterns: [list selected patterns]
Endpoints: [count]
Tables: [count]
ADRs: [count]

Key decisions:
- [Decision 1]
- [Decision 2]

Ready for Stage 3: Implementation
```
