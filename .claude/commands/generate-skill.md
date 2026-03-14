---
description: "Scaffold a new skill knowledge base from template"
allowed-tools: Read, Write, Glob, Grep, AskUserQuestion
---

# Generate Skill

Create a new skill file with proper frontmatter and structure.

## Input
$ARGUMENTS — Skill name and domain. E.g., "caching — Redis and in-memory caching patterns"

## Process

1. **Parse input**: Extract skill name (kebab-case) and domain from $ARGUMENTS
2. **If no input**, ask user:
   - "What should this skill be named? (kebab-case, e.g., caching)"
   - "What domain knowledge should it cover? (one sentence)"
   - "Is this a foundation skill (reusable domain knowledge) or meta skill (orchestration/process)?"
3. **Determine location**:
   - Foundation → `.claude/skills/foundation/<name>/SKILL.md`
   - Meta → `.claude/skills/meta/<name>/SKILL.md`
4. **Check for conflicts**: Verify directory doesn't already exist
5. **Generate file**:

```markdown
---
name: <name>
description: "<domain description>"
---

# <Name>

## Overview
[What this skill covers and when to use it]

## Key Concepts
- [Concept 1]: [explanation]
- [Concept 2]: [explanation]

## Patterns

### Pattern 1: [Name]
[Description and example]

### Pattern 2: [Name]
[Description and example]

## Best Practices
- [Practice 1]
- [Practice 2]

## Anti-Patterns
- [What to avoid and why]

## Quick Reference
| Scenario | Approach |
|----------|----------|
| [When X] | [Do Y] |
```

6. **Confirm creation** and show the file path.
