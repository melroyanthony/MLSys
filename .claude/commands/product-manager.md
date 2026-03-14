---
description: Analyze requirements with RICE/MoSCoW prioritization (Stage 1)
allowed-tools: Read, Glob, Grep, Write, WebFetch
---

# Product Manager

Convert problem statements into prioritized requirements.

## Problem Statement Discovery

**First, locate the problem statement:**

1. **Check for PROBLEM.md**:
```bash
ls -la PROBLEM.md problem.md PROBLEM.txt problem.txt 2>/dev/null
```

2. **If found**: Read the file, confirm with user: "Found PROBLEM.md. Proceed?"

3. **If NOT found**: Prompt user with options:
   - "I'll paste it now" → Wait for input
   - "It's in a different file" → Ask for path
   - "Create PROBLEM.md for me" → Create template

---

## Input
Once problem statement is located:
- Problem statement content
- Time budget for the challenge

## Frameworks

### RICE Scoring
- **Reach**: 1-10 (users impacted)
- **Impact**: 0.25-3 (minimal to massive)
- **Confidence**: 50%, 80%, 100%
- **Effort**: person-hours (1, 2, 4, 8, 16)
- **Score** = (Reach x Impact x Confidence) / Effort

### MoSCoW
- **Must Have**: Core functionality
- **Should Have**: Important but not critical
- **Could Have**: Nice to have
- **Won't Have**: Out of scope

## Output Artifacts

Create in `solution/requirements/`:

### 1. requirements.md
```markdown
# Requirements
## Problem Summary
## Functional Requirements
## Non-Functional Requirements
## Constraints
```

### 2. rice-scores.md
| Feature | Reach | Impact | Confidence | Effort | Score | Priority |

### 3. moscow.md
Categorized feature list with checkboxes.

### 4. mvp-scope.md
```markdown
# MVP Scope
## Included Features
## Acceptance Criteria
## Out of Scope
## Success Metrics
```

## Decision Rules
- MVP should be <=70% of total scope
- Must Haves achievable in 60% of time budget
- When in doubt, cut scope

## Time Budget
- 5 min: Parse and extract requirements
- 5 min: RICE scoring
- 5 min: MoSCoW categorization
- 5 min: MVP scope definition

## Handoff Summary
```
Requirements Complete

Features identified: [N]
MVP features: [M]
Estimated effort: [X hours]

Ready for Stage 2: Architecture
```
