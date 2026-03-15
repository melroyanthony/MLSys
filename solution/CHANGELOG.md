# Changelog

All notable changes to this project are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [1.0.0] - 2026-03-14

### Added

- **Track A: Rust scheduler binary** (`solution/backend/rust/`)
  - `main.rs` — CLI with two modes:
    - Solve: `./mlsys <input.json> <output.json>`
    - Evaluate: `./mlsys evaluate --problem <input.json> --solution <solution.json>`
  - `models.rs` — Core data types: `Problem`, `Op`, `Tensor`, `Solution`,
    `SubgraphDef`, `Granularity`.
  - `parser.rs` — Deserialises problem and solution JSON with strict validation
    (`op_type`, `MatMul` arity, tensor bounds, `Granularity` length).
  - `dag.rs` — DAG construction: topological sort (Kahn's), cycle detection,
    boundary tensor computation, predecessor/successor maps, tensor index bounds
    checking.
  - `latency.rs` — Subgraph latency model: per-op `K_full` scaling for `MatMul`
    (`base_cost * k/K_full`), roofline per step (`max(compute, memory)`),
    intra-subgraph data reuse tracking.
  - `memory.rs` — Working-set calculator and OOM checker. Uses min `K_full` across
    `MatMul` ops for split-K search (safe for mixed-K subgraphs).
  - `evaluate.rs` — Full solution evaluator: validates OOM, op coverage,
    traversal order permutation, and reported-vs-computed latency mismatch.
  - `serializer.rs` — Serialises `Solution` to contest JSON format with
    proper error propagation (no panics).
  - `baseline.rs` — Initial schedule: one subgraph per op at native granularity.
  - `optimizer/fusion.rs` — Greedy chain fusion with boundary output dimension
    consistency check before merging.
  - `optimizer/retention.rs` — Tensor retention across subgraph boundaries.
  - `optimizer/splitk.rs` — Split-K for memory-constrained MatMuls.
  - `optimizer/granularity.rs` — Exhaustive (w, h, k) grid search per subgraph.
  - `optimizer/traversal.rs` — Snake/zig-zag tile ordering for MatMul data reuse.
  - `optimizer/pipeline.rs` — 9-stage pipeline orchestrator:
    1. Baseline
    2. Greedy chain fusion
    3. Retention (pass 1)
    4. Split-K
    5. Granularity search
    6. Retention (pass 2)
    7. Emergency OOM fix
    8. Final latency recalculation
    9. Traversal optimization

- **Track B: Python Gemini agent** (`solution/agent/`)
  - `evaluator.py` — Pure-Python latency model mirroring Rust logic exactly.
    Per-op `K_full` scaling, boundary-output `MatMul` `K_full` for `num_k_steps`,
    `widths`/`heights` length validation.
  - `scheduler.py` — Python optimizer pipeline (baseline, fusion, split-K,
    granularity search, retention, traversal); runs without any API call.
  - `agent.py` — Agent loop: runs local optimizer first (safe fallback),
    then iteratively calls Gemini 2.5 Flash to propose improvements; validates
    each suggestion locally (including traversal order permutation check)
    before accepting; writes best solution within a 9-minute budget.
    Traversal order elements coerced to int from Gemini responses.
  - `prompts/system.md` — System prompt: contest rules, output format,
    latency formula, optimisation objectives.
  - `prompts/examples.md` — Five worked examples from the problem statement.
  - `prompts/strategies.md` — Fusion, retention, split-K, granularity tuning.
  - `requirements.txt` — Single runtime dependency: `google-genai>=1.0.0`.

- **Optimizer stages** (implemented in both tracks)
  - Baseline: guaranteed valid starting schedule.
  - Chain fusion: eliminates intermediate DRAM round-trips for adjacent ops.
  - Tensor retention: avoids redundant write-then-read across subgraph boundaries.
  - Split-K: enables memory-feasible MatMul execution under tight capacity.
  - Granularity search: tunes tile size to balance compute and memory.
  - Traversal optimization: snake/zig-zag tile order for MatMul data reuse.

- **Test suite**
  - 18 Rust unit tests in `src/main.rs`:
    - Example 1 (baseline pointwise chain): strategies A, B, C
    - Example 2 (larger tensors, 256x256): strategies A and B
    - Example 3 (diamond graph): spilling baseline and selective retention
    - Example 4 (MatMul, 128x128): naive spatial tiling
    - Example 5 (chained MatMul): split-K granularity
    - Edge cases: single tiny op, OOM detection, serialization round-trip,
      ephemeral tensor boundary correctness, cyclic DAG rejection
    - All 5 released benchmarks: full pipeline validity check
    - New tests (3): `fused_matmul_pointwise_splitk`, `boundary_pw_input`,
      `mixed_k_two_matmuls`
  - E2E script (`solution/scripts/test-e2e.sh`):
    - Track A build + 5 benchmark validation
    - Track B import verification + 5 benchmark validation (baseline mode)
    - Trap handler for temp directory cleanup on exit/signal
    - Uses `uv run python` with `sys.argv` (no shell interpolation)

- **CI/CD** — GitHub Actions workflow (`.github/workflows/ci.yml`):
  - Rust job: `cargo build --release` + `cargo test`
  - Python job: `uv` setup, dependency install, evaluator smoke test,
    benchmark baseline run with `uv run python`
  - E2E job: both tracks against all 5 benchmarks with JSON validation
  - Cache key uses `Cargo.toml` hash (Cargo.lock is gitignored)

- **Documentation**
  - `solution/README.md` — Project overview, 9-stage pipeline diagram,
    per-step roofline latency formula, benchmark results, quick-start for
    both tracks.
  - `solution/CHANGELOG.md` — This file.
  - `solution/docs/architecture/` — System design (Rust modules), data model
    (C++ to Rust mapping), data flow (9-stage composition), deployment
    topology (Rust binary + Python agent), user journeys, C4 workspace,
    error catalog (Rust error handling), security model.
  - `solution/docs/decisions/` — ADR-001 (Rust + Python language selection),
    ADR-002 (baseline-first development), ADR-003 (greedy fusion over DP),
    ADR-004 (k-dimension search in granularity optimization),
    ADR-005 (closed-form latency evaluation),
    ADR-006 (mixed-K fusion).

- **Benchmark results** (Track A — Rust)

  | Benchmark | Ops | Latency | Subgraphs |
  |-----------|-----|---------|-----------|
  | mlsys-2026-1  | 5   | 262,822    | 4  |
  | mlsys-2026-5  | 19  | 909,261    | 13 |
  | mlsys-2026-9  | 32  | 12,415,140 | 24 |
  | mlsys-2026-13 | 63  | 4,707,779  | 25 |
  | mlsys-2026-17 | 103 | 814,572    | 81 |
