# SDLC Claude Code Configuration

Multi-agent orchestration template for building production-ready full-stack applications from problem statements.

## Architecture

This repo is a **Claude Code plugin** — a `.claude/` configuration that transforms Claude Code into a multi-agent SDLC pipeline.

### Core Components

| Layer | Location | Purpose |
|-------|----------|---------|
| Agents | `.claude/agents/` | Specialized subagents (orchestrator, PM, architect, devs, tester, reviewer, judge) |
| Commands | `.claude/commands/` | User-invocable slash commands (`/orchestrate`, `/tdd`, `/review`, etc.) |
| Skills | `.claude/skills/` | Domain knowledge bases loaded on demand |
| Rules | `.claude/rules/` | Always-loaded coding standards (security, style, git) |
| Hooks | `.claude/hooks/` | Deterministic lifecycle automation (quality gates, format checks) |

### Pipeline: Stages 0-5 (with 2.5 bridge)

```
Problem → Stage 0 (Setup) → Stage 1 (Requirements) → Stage 2 (Architecture)
       → Stage 2.5 (Issues + Branch) ← planning on main | implementation on branch →
       → Stage 3 (Implementation) → Stage 4 (Testing) → Stage 5 (Finalization + PR)
```

Stages 0-2 commit to **main** (planning artifacts). Stage 2.5 creates GitHub issues from MVP scope and switches to a **feature branch**. Stages 3-5 commit to the feature branch, ending with a PR that closes all issues.

## Key Conventions

- **Git root**: Project root, not `solution/`
- **Problem input**: `problem/PROBLEM.md` + supporting files
- **Solution output**: All artifacts go in `solution/`
- **Package manager**: `uv` for Python (never pip), `npm` for Node.js
- **Commits**: Conventional format — `feat(stage-N): description`
- **Quality gates**: Judge validates between stages (≥70% pass required)
- **Model routing**: Opus for architecture/review/security, Sonnet for implementation, Haiku for mechanical tasks

## Available Commands

### Pipeline
- `/orchestrate` — Run full SDLC pipeline (or `/orchestrate N` to resume from stage N)
- `/product-manager` — Stage 1: Requirements with RICE/MoSCoW
- `/architect` — Stage 2: System design, OpenAPI, database schema
- `/backend-dev` — Stage 3: Backend implementation
- `/frontend-dev` — Stage 3: Frontend implementation
- `/tester` — Stage 4: Tests and validation
- `/judge N` — Validate stage N output

### Quality & Review
- `/review` — Staff-level code review (files, dirs, or git diffs)
- `/security-review` — OWASP Top 10 security audit
- `/verify` — 6-phase verification loop (Build → Type → Lint → Test → Security → Diff)
- `/tdd` — Test-Driven Development workflow (RED → GREEN → REFACTOR)
- `/validate-e2e` — Docker Compose E2E happy path test

### Debugging & Investigation
- `/investigate` — Root-cause analysis on bugs, errors, CVEs → fix → test → PR
- `/upgrade` — Safely upgrade dependencies to resolve CVEs or update versions

### Planning & Research
- `/plan` — Structured implementation plan with dependency graph
- `/search-first` — Research existing solutions before building custom code
- `/postman` — Generate Postman collection + environments from OpenAPI spec

### Git & Changeset Tracking
- `/commit` — Conventional commit with semantic tagging and issue linking
- `/create-pr` — PR with Mermaid diagrams, linked issues, auto-labeling
- `/create-issue` — GitHub issue with diagrams via `gh` CLI
- `/resolve-review` — Fetch PR review comments, categorize, fix, commit, push, reply

### Meta-Tooling (Config Management)
- `/validate-config` — Lint `.claude/` for frontmatter, shell compatibility, naming
- `/audit` — Inventory check — compare files vs documented counts
- `/generate-agent` — Scaffold a new agent from template
- `/generate-skill` — Scaffold a new skill from template
- `/generate-command` — Scaffold a new command from template

### Session & Deployment
- `/save-session` — Capture session state for later resumption
- `/resume-session` — Resume from a saved session
- `/cloud-deploy` — Generate IaC for cloud deployment (Terraform/K8s, AWS/GCP/Azure)
- `/readme-generator` — Generate README and CHANGELOG
- `/git-deploy` — Push to GitHub repository

## Agent Roster

| Agent | Model | Role |
|-------|-------|------|
| orchestrator | opus | Pipeline coordinator |
| product-manager | sonnet | Requirements analysis |
| architect | opus | System design |
| backend-dev | sonnet | Backend implementation |
| frontend-dev | sonnet | Frontend implementation |
| tester | sonnet | Testing & validation |
| judge | opus | Quality gate evaluation |
| code-reviewer | opus | Code review |
| security-reviewer | opus | Security audit |
| refactor | sonnet | Code refactoring |
| planner | opus | Task decomposition |
| debugger | opus | Root-cause analysis and investigation |
| cloud-deployer | sonnet | IaC generation (Terraform/K8s) |
| git-deployer | haiku | GitHub deployment |

## Rules (Always Loaded)

- `rules/coding-standards.md` — Function length, naming, composition, language-specific patterns
- `rules/security.md` — OWASP prevention, input validation, secret management
- `rules/git.md` — Conventional commits, atomic changes, branch strategy

## Development Patterns

- **Search-first**: Check stdlib → codebase → packages → build custom (in that order)
- **TDD**: RED → GREEN → REFACTOR for business logic, 80%+ coverage
- **Verification loop**: Build → Type Check → Lint → Test → Security → Diff Review
- **Handoff protocol**: Structured documents between agents with decisions, artifacts, open questions
- **Session persistence**: Save/resume for long-running pipelines with "what didn't work" tracking
- **Autonomous loops**: Self-healing builds, continuous PR loops, parallel worktree execution
