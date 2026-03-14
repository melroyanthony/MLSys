# Changelog

All notable changes to this project are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [1.0.0] - 2026-03-14

### Added

- **Track A: Rust scheduler binary** (`solution/backend/rust/`)
  - `main.rs` — CLI entry point; reads input JSON, runs pipeline, writes
    output JSON; prints subgraph count and total latency to stderr.
  - `models.rs` — Core data types: `Problem`, `Op`, `Tensor`, `Solution`,
    `SubgraphDef`, `Granularity`.
  - `parser.rs` — Deserialises the contest input JSON format into `Problem`.
  - `dag.rs` — DAG construction: topological sort, cycle detection, boundary
    tensor computation, predecessor/successor maps.
  - `latency.rs` — Subgraph latency model: tile-level compute cost, slow-memory
    transfer time, working-set tracking, roofline calculation.
  - `memory.rs` — Fast-memory OOM check for a given subgraph and granularity.
  - `evaluate.rs` — Full solution evaluator; also exposed via CLI evaluate
    subcommand (`./mlsys evaluate --problem <f> --solution <f>`).
  - `serializer.rs` — Serialises `Solution` to the contest output JSON format.
  - `baseline.rs` — Initial schedule: one subgraph per op at native granularity.
  - `optimizer/fusion.rs` — Greedy chain fusion: merges adjacent ops when
    fusing reduces latency and the working set fits in fast memory.
  - `optimizer/retention.rs` — Tensor retention: decides whether keeping a
    boundary tensor resident in fast memory lowers total latency.
  - `optimizer/splitk.rs` — Split-K: reduces MatMul k-dimension for subgraphs
    that OOM at native granularity.
  - `optimizer/granularity.rs` — Granularity grid search: finds the (w, h, k)
    that minimises subgraph latency subject to memory feasibility.
  - `optimizer/traversal.rs` — Traversal order optimization: compares raster
    vs snake (zig-zag) tile order for MatMul subgraphs, picks lower latency.
  - `optimizer/pipeline.rs` — 9-stage pipeline orchestrator:
    baseline → fusion → retention → split-K → granularity search →
    retention (pass 2) → emergency OOM fix → final latency recalculation →
    traversal optimization.

- **Track B: Python Gemini agent** (`solution/agent/`)
  - `evaluator.py` — Pure-Python latency model; mirrors the Rust latency
    logic exactly (used for local validation of Gemini suggestions).
  - `scheduler.py` — Python optimizer pipeline (baseline, fusion, split-K,
    granularity search, retention); runs without any API call.
  - `agent.py` — Agent loop: runs local optimizer first (safe fallback),
    then iteratively calls Gemini 2.5 Flash to propose improvements; validates
    each suggestion locally before accepting it; writes best solution found
    within a 9-minute budget.
  - `prompts/system.md` — System prompt: contest rules, output format,
    latency formula, optimisation objectives.
  - `prompts/examples.md` — Five worked examples from the problem statement
    with annotated strategies and expected latencies.
  - `prompts/strategies.md` — Optimisation strategy guide: fusion heuristics,
    retention decision rules, split-K guidance, granularity tuning.
  - `requirements.txt` — Single runtime dependency: `google-genai>=1.0.0`.

- **Optimizer stages** (implemented in both tracks)
  - Baseline: guaranteed valid starting schedule.
  - Chain fusion: eliminates intermediate DRAM round-trips for adjacent ops.
  - Tensor retention: avoids redundant write-then-read across subgraph boundaries.
  - Split-K: enables memory-feasible MatMul execution under tight capacity.
  - Granularity search: tunes tile size to balance compute and memory at
    the roofline equilibrium point.
  - Traversal optimization: snake/zig-zag tile order for MatMul data reuse.

- **Test suite**
  - 15 Rust unit tests in `src/main.rs`:
    - Example 1 (baseline pointwise chain): strategies A, B, C
    - Example 2 (larger tensors, 256x256): strategies A and B
    - Example 3 (diamond graph): spilling baseline and selective retention
    - Example 4 (MatMul, 128x128): naive spatial tiling
    - Example 5 (chained MatMul): split-K granularity
    - Edge cases: single tiny op, OOM detection, serialization round-trip,
      ephemeral tensor boundary correctness, cyclic DAG rejection
    - All 5 released benchmarks: full pipeline produces valid, fully-covering,
      non-negative-latency solutions
  - 13 E2E checks via `solution/scripts/test-e2e.sh`:
    - Track A build verification
    - Track A: 5 benchmarks validated (JSON structure, coverage, latencies)
    - Track B: evaluator and scheduler import verification
    - Track B: 5 benchmarks validated in baseline mode (no API key required)

- **CI/CD** — GitHub Actions workflow (`solution/.github/workflows/ci.yml`):
  - Rust job: `cargo build --release` + `cargo test` on ubuntu-latest
  - Python job: `uv` setup, dependency install, evaluator smoke test,
    benchmark baseline run
  - E2E job: builds Rust binary, runs all 5 benchmarks through both tracks,
    validates output JSON

- **Documentation**
  - `solution/README.md` — Project overview, quick-start instructions,
    architecture diagram, full pipeline description, test instructions,
    benchmark results summary.
  - `solution/CHANGELOG.md` — This file.
  - `solution/docs/architecture/` — Architecture decision records.
  - `solution/docs/decisions/` — Implementation decision notes.
