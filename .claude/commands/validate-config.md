---
description: "Validate .claude/ configuration for correctness — frontmatter, shell compatibility, naming, orphaned files"
allowed-tools: Read, Glob, Grep, Bash
---

# Validate Config

Lint the `.claude/` directory for correctness and consistency.

## Process

1. **Scan all agents** in `.claude/agents/*.md`:
   - Verify YAML frontmatter has required fields: `name`, `description`, `tools`
   - Verify `name` matches filename (e.g., `code-reviewer.md` has `name: code-reviewer`)
   - Verify `tools` field is a valid format (list or comma-separated)
   - Flag agents without `model` field (optional but recommended)

2. **Scan all commands** in `.claude/commands/*.md`:
   - Verify YAML frontmatter has required fields: `description`
   - Verify `allowed-tools` is present and non-empty
   - Flag commands that reference agents not in `.claude/agents/`

3. **Scan skill definition files** in `.claude/skills/**/SKILL.md` only:
   - Verify YAML frontmatter has required fields: `name`, `description`
   - Verify `name` matches parent directory name
   - Skip supplemental docs (APPROACH.md, PATTERNS.md, MODELS.md, templates, etc.) — these don't require frontmatter

4. **Scan all rules** in `.claude/rules/*.md`:
   - Verify YAML frontmatter has `description` and `globs`
   - Verify `globs` is a valid JSON array

5. **Scan all hook scripts** in `.claude/hooks/scripts/*.sh`:
   - Verify shebang line (`#!/usr/bin/env bash` or `#!/bin/bash`)
   - Check for bash 4+ features:
     - `declare -A` (associative arrays)
     - `mapfile` or `readarray`
     - `${var,,}` or `${var^^}` (case modification)
     - `|&` (pipe stderr)
     - `&>` in non-redirect context
   - Verify scripts end with `exit 0`
   - Verify scripts are executable (`-x` permission)
   - Check for non-portable grep patterns: `\b`, `\s`, `\w` (should use `-w`, `[[:space:]]`, `[[:alnum:]]`)

6. **Check hooks config** in `.claude/settings.json`:
   - Verify `hooks` key exists
   - Verify each hook command references a script that exists
   - Verify matchers are valid (Edit, Write, Bash, etc.)

7. **Check for orphaned files**:
   - Agents not referenced by any command
   - Skills not referenced by any agent or command
   - Hook scripts not referenced in settings.json

## Output

```markdown
# Config Validation Report

## Summary
- Agents: N scanned, N valid, N issues
- Commands: N scanned, N valid, N issues
- Skills: N scanned, N valid, N issues
- Rules: N scanned, N valid, N issues
- Hooks: N scanned, N valid, N issues

## Issues Found

### CRITICAL
- [file]: [issue description]

### WARNING
- [file]: [issue description]

### INFO
- [file]: [suggestion]

## Result: PASS / FAIL
```
