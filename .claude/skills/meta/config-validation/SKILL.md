---
name: config-validation
description: "Validation rules for .claude/ configuration files — frontmatter schemas, shell compatibility, naming conventions, and consistency checks."
---

# Config Validation Rules

## Agent Frontmatter Schema

Required fields:
```yaml
---
name: <string>          # Must match filename without .md
description: <string>   # What the agent does
tools: <list|string>    # Available tools (list or comma-separated)
---
```

Optional fields:
```yaml
model: <opus|sonnet|haiku>  # Model to use (defaults to project setting)
skills: <list|string>       # Skills this agent uses
```

### Validation Rules
- `name` must be kebab-case and match the filename
- `description` must be non-empty
- `tools` must contain only valid tool names: Read, Glob, Grep, Edit, Write, Bash, WebFetch, WebSearch, Agent, AskUserQuestion, Task, NotebookEdit (this list is non-exhaustive — check Claude Code docs for additions)
- If `model` is specified, must be one of: opus, sonnet, haiku

## Command Frontmatter Schema

Required fields:
```yaml
---
description: <string>       # What the command does
allowed-tools: <string>     # Comma-separated tool names
---
```

Optional fields:
```yaml
argument-hint: <string>     # Hint for $ARGUMENTS
```

### Validation Rules
- `description` must be non-empty
- `allowed-tools` must contain only valid tool names
- If command body references `$ARGUMENTS`, `argument-hint` should be present

## Skill Frontmatter Schema

Required fields:
```yaml
---
name: <string>          # Must match parent directory name
description: <string>   # What knowledge this skill provides
---
```

## Rule Frontmatter Schema

Required fields:
```yaml
---
description: <string>   # What this rule enforces
globs: <array>          # File patterns this rule applies to
---
```

### Validation Rules
- `globs` must be a valid JSON array of glob patterns
- Glob patterns must be valid (e.g., `"**/*.py"`, `"**/*"`)

## Shell Script Compatibility (bash 3.2 — macOS default)

### Forbidden Features
| Feature | Bash Version | Portable Alternative |
|---------|-------------|---------------------|
| `declare -A` | 4.0+ | Use indexed arrays + loops |
| `mapfile` / `readarray` | 4.0+ | Use `while read` loop |
| `${var,,}` / `${var^^}` | 4.0+ | Use `tr '[:upper:]' '[:lower:]'` |
| `|&` (pipe stderr) | 4.0+ | Use `2>&1 |` |
| `[[ $var =~ regex ]]` with stored regex | 4.0+ | Inline the regex |
| `${!prefix@}` variable indirection | 4.0+ | Avoid or use eval |
| `coproc` | 4.0+ | Use named pipes |

### Forbidden grep Patterns
| Pattern | Issue | Portable Alternative |
|---------|-------|---------------------|
| `\b` | Not POSIX, backspace on macOS | `grep -w` or `[^[:alnum:]]` boundaries |
| `\s` | Not POSIX | `[[:space:]]` |
| `\w` | Not POSIX | `[[:alnum:]_]` |
| `\d` | Not POSIX | `[0-9]` |

### Required Script Properties
- Shebang: `#!/usr/bin/env bash` or `#!/bin/bash` (both accepted)
- Must exit 0 (hooks are non-blocking)
- Warnings go to stderr (`>&2`)
- Must be executable (`chmod +x`)

## Naming Conventions

| Component | Convention | Example |
|-----------|-----------|---------|
| Agent files | kebab-case.md | `code-reviewer.md` |
| Command files | kebab-case.md | `create-pr.md` |
| Skill directories | kebab-case/ | `api-testing/` |
| Skill files | UPPER_CASE.md | `SKILL.md`, `PATTERNS.md` |
| Rule files | kebab-case.md | `coding-standards.md` |
| Hook scripts | kebab-case.sh | `quality-gate.sh` |
