---
description: "Git workflow rules"
globs: ["**/*"]
---

# Git Rules

## Commit Format (choose one style per project)
- **Conventional**: `type(scope): description` — types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert, release
- **Gitmoji** (tiangolo-style): `<emoji> <description>` — e.g., `✨ Add user registration`, `🐛 Fix token expiry`
- Gitmoji map: ✨=feat 🐛=fix 📝=docs ♻️=refactor ⬆️=upgrade 🔧=config 👷=ci 🔖=release 🔒️=security ✅=test ⚡=perf

## Commit Rules
- Imperative mood: "add" not "added" or "adds"
- No period at end of subject line
- Max 72 characters for subject
- Keep commits atomic — one logical change per commit
- Body explains WHY, not WHAT (the diff shows WHAT)
- Reference issues: `Refs #N`, `Closes #N`, `Fixes #N`

## PR Rules
- PR title follows same format as commits
- PRs must have exactly one label: feature, bug, refactor, upgrade, docs, internal, breaking, security
- Include Mermaid diagrams for architecture/data flow changes
- Link related GitHub issues with `Closes #N`

## Branch & Merge
- Create feature branches for non-trivial changes: `feat/issue-N-description`
- Rebase feature branches onto main before merging
- Never force push to main/master

## Security
- Never commit .env files, credentials, or secrets
- Use `git-workflow` skill for full reference
