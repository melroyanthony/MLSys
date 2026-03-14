---
description: Validate stage outputs against quality rubrics (LLM-as-Judge)
allowed-tools: Read, Glob, Grep, Write
argument-hint: [stage-number]
---

# Stage Validator (LLM-as-Judge)

Validate stage $ARGUMENTS outputs against defined rubrics.

## Rubrics by Stage

### Stage 1: Requirements
| Criteria | Weight | Pass Threshold |
|----------|--------|----------------|
| All features extracted | 30% | 100% coverage |
| RICE scores calculated | 25% | All features scored |
| MoSCoW categorization logical | 25% | Clear reasoning |
| MVP scope achievable | 20% | <=70% of total scope |

### Stage 2: Architecture
| Criteria | Weight | Pass Threshold |
|----------|--------|----------------|
| OpenAPI spec complete | 30% | All MVP endpoints |
| Database schema normalized | 25% | 3NF minimum |
| ADRs document decisions | 25% | >=2 ADRs |
| C4 diagrams accurate | 20% | Container level |

### Stage 3: Implementation
| Criteria | Weight | Pass Threshold |
|----------|--------|----------------|
| API matches OpenAPI spec | 30% | All endpoints work |
| Database operations correct | 25% | CRUD functional |
| Frontend displays data | 25% | Core flows work |
| Code follows patterns | 20% | Consistent style |

### Stage 4: Testing
| Criteria | Weight | Pass Threshold |
|----------|--------|----------------|
| Critical paths tested | 40% | >=80% coverage |
| Tests pass | 40% | 100% green |
| Edge cases covered | 20% | Key scenarios |

## Evaluation Process

1. Read stage artifacts from `solution/`
2. Score each criterion (1-5)
3. Calculate weighted score
4. Determine verdict

## Output Format

```markdown
# Stage N Checkpoint

## Score: X/100

### Criteria Breakdown
- [PASS/FAIL] Criterion 1: Score/Weight - Comments
- [PASS/FAIL] Criterion 2: Score/Weight - Comments

### Issues Found
1. Issue description and impact

### Recommendations
1. How to address before proceeding

### Verdict
[PROCEED/BLOCK] - Reasoning
```

## Gate Logic

- Score >= 70%: **PROCEED** to next stage
- Score 50-69%: **PROCEED** with warnings
- Score < 50%: **BLOCK** until issues resolved

Write checkpoint to `solution/checkpoints/stage-N-validation.md`
