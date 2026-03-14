---
description: "Research existing solutions before writing custom code"
allowed-tools: Read, Glob, Grep, Bash, WebFetch, WebSearch, Agent
---

# Search-First

Research existing solutions before implementing custom code.

## Input
$ARGUMENTS — Description of the functionality needed.

## Process

1. **Load search-first skill** from `.claude/skills/foundation/search-first/SKILL.md`
2. **Search the existing codebase**: Look for similar functionality already implemented
3. **Search package registries**:
   - Python: Search PyPI for relevant packages
   - Node.js: Search npm for relevant packages
4. **Search GitHub**: Look for well-maintained open-source implementations
5. **Evaluate options** using the decision matrix from the skill:
   - Adopt (use as-is) vs Extend (wrap/adapt) vs Build (custom)
6. **Present findings**:
   - Options found with pros/cons
   - Recommendation with justification
   - If building custom: outline the implementation approach

## Output
Research report with recommendation: Adopt, Extend, or Build.
