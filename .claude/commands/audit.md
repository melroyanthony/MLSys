---
description: "Audit .claude/ inventory against documentation — find count mismatches and undocumented components"
allowed-tools: Read, Glob, Grep, Bash
---

# Audit

Compare actual `.claude/` contents against what's documented in README.md and CLAUDE.md.

## Process

1. **Count actual files**:
   - Agents: `ls .claude/agents/*.md | wc -l`
   - Commands: `ls .claude/commands/*.md | wc -l`
   - Skills (by definition file): `find .claude/skills -name "SKILL.md" | wc -l`
   - Rules: `ls .claude/rules/*.md | wc -l`
   - Hook scripts: `find .claude/hooks -name "*.sh" | wc -l`

2. **Extract documented counts** from README.md:
   - Search for "N agents", "N commands", "N slash commands", etc.
   - Compare with actual counts

3. **Extract documented counts** from CLAUDE.md:
   - Search for agent roster table rows
   - Search for command list items
   - Compare with actual counts

4. **Find undocumented components**:
   - Agents in `.claude/agents/` but not in README agent table
   - Commands in `.claude/commands/` but not in README commands section
   - Commands in `.claude/commands/` but not in CLAUDE.md commands section

5. **Find documented but missing components**:
   - Agents listed in README but no file in `.claude/agents/`
   - Commands listed in README but no file in `.claude/commands/`

6. **Check settings.json permissions**:
   - List all `Bash(*)` permissions
   - Flag any that seem unused by current agents/commands

## Output

```markdown
# Audit Report

## Inventory
| Component | Actual | README | CLAUDE.md | Status |
|-----------|--------|--------|-----------|--------|
| Agents | N | N | N | ✅ / ❌ |
| Commands | N | N | N | ✅ / ❌ |
| Skills | N | - | - | - |
| Rules | N | N | - | ✅ / ❌ |
| Hooks | N | N | - | ✅ / ❌ |

## Undocumented
- [component]: exists in .claude/ but not in docs

## Missing
- [component]: documented but file not found

## Recommendation
[What to update to bring docs in sync]
```
