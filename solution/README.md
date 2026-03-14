# MLSys 2026 — DAG Scheduler Contest Submission

This repository contains two independent, production-ready solutions for the
MLSys 2026 contest problem: schedule a computation DAG expressed as a set of
ops and tensors so that total end-to-end latency (compute + slow-memory I/O)
is minimised, subject to a fast-memory capacity constraint.

---

## Problem Overview

Each benchmark is a JSON file describing a directed acyclic graph (DAG):

- **Tensors** — flat 2-D buffers (width x height elements).
- **Ops** — either `Pointwise` (element-wise) or `MatMul` (matrix multiply).
- **Fast memory** — on-chip SRAM of fixed capacity (elements, not bytes).
- **Slow memory** — off-chip DRAM accessed with a given bandwidth (elements/cycle).

A valid solution partitions all ops into **subgraphs**, each executed at a
chosen tile granularity `(w, h, k)`. Within a subgraph every tile of the output
is computed as a unit; intermediate tensors between fused ops stay in fast
memory and are never written back to DRAM. The scheduler also decides which
boundary tensors to **retain** in fast memory between consecutive subgraphs.

The score is the sum of `subgraph_latency` across all subgraphs, where:

```
subgraph_latency = max(total_compute_cost, total_slow_memory_transfer_time)
```

Lower is better.

---

## Two Tracks

### Track A — Rust Binary

A deterministic, zero-dependency optimizer compiled to a native binary.
It runs in well under one second on all released benchmarks.

**Entry point:** `solution/backend/rust/`

### Track B — Python Gemini Agent

A Python agent that first builds a strong local schedule (same algorithmic
core as Track A, re-implemented in pure Python), then optionally refines it
using Gemini 2.5 Flash when a `GOOGLE_API_KEY` is available.

**Entry point:** `solution/agent/`

---

## Quick Start

### Prerequisites

- Rust toolchain 1.80+ (`rustup`)
- Python 3.12+ and `uv` (`brew install uv` or `curl -LsSf https://astral.sh/uv/install.sh | sh`)

---

### Track A — Rust Binary

```bash
cd solution/backend/rust

# Build release binary (one-time, ~5 s)
cargo build --release

# Run against a benchmark
./target/release/mlsys path/to/input.json path/to/output.json

# Example with a released benchmark
./target/release/mlsys ../../../problem/benchmarks/mlsys-2026-1.json /tmp/out.json
```

The binary writes the solution JSON to the specified output path and prints a
one-line summary to stderr:

```
Solution: 3 subgraphs, total latency = 8234.56
Solution written to /tmp/out.json
```

To run the full unit test suite (15 tests, including all 5 released benchmarks):

```bash
cargo test
```

---

### Track B — Python Gemini Agent

```bash
cd solution/agent

# Create virtual environment and install dependencies
uv venv
uv add google-genai --active      # or: uv pip install -r requirements.txt

# Run without Gemini (uses local optimizer only — always works)
GOOGLE_API_KEY=dummy uv run python agent.py path/to/input.json path/to/output.json

# Run with Gemini refinement (requires a valid API key)
GOOGLE_API_KEY=<your-key> uv run python agent.py path/to/input.json path/to/output.json
```

Progress is written to stderr; only valid JSON goes to stdout / the output
file. The agent writes a safe fallback solution immediately after the local
optimizer finishes, so it always produces output even if the API is
unavailable.

---

## Architecture

Both tracks share the same conceptual optimizer pipeline. Track A implements
it in Rust; Track B re-implements it in Python (`scheduler.py` + `evaluator.py`).

### Optimizer Pipeline (8 stages)

```
Input JSON
    |
    v
Stage 1  BASELINE
         One subgraph per op, native granularity.
         Guarantees a valid starting point.
    |
    v
Stage 2  GREEDY CHAIN FUSION
         Merge adjacent ops (in topological order) when:
           - the merged working set fits in fast memory
           - fusing reduces total latency (no DRAM round-trip for
             intermediate tensor)
         Multiple passes until no further merges are possible.
    |
    v
Stage 3  RETENTION (pass 1)
         For each pair of consecutive subgraphs, decide whether keeping
         a shared boundary tensor resident in fast memory (rather than
         writing then re-reading it) lowers total latency.
    |
    v
Stage 4  SPLIT-K
         For MatMul subgraphs that still OOM at native granularity,
         reduce the k-dimension (contraction depth) until the working
         set fits, trading compute parallelism for memory feasibility.
    |
    v
Stage 5  GRANULARITY SEARCH
         For each subgraph, grid-search over (w, h) tile sizes that
         are divisors of the output tensor dimensions. Pick the size
         that minimises subgraph_latency subject to the OOM constraint.
         For MatMul, also searches over k (split-K depth).
    |
    v
Stage 6  RETENTION (pass 2)
         Re-run retention decisions after granularities are finalised.
    |
    v
Stage 7  EMERGENCY OOM FIX
         Any subgraph that still OOMs (e.g., due to unusually large
         tensors) has its granularity reduced to the smallest feasible
         power-of-two tile.
    |
    v
Stage 8  FINAL LATENCY RECALCULATION
         Recompute subgraph_latency for every subgraph with its final
         granularity and retention decisions.
    |
    v
Output JSON
```

### Latency Model

For a subgraph executing at granularity `(w, h, k)`:

```
num_spatial_tiles = ceil(W_out / w) * ceil(H_out / h)
num_k_steps       = ceil(K_full / k)  (MatMul) or 1 (Pointwise)

Per step (one spatial tile, one k-step):
  compute_time = sum(
      base_cost * (k / K_full)  for MatMul ops,
      base_cost                 for Pointwise ops
  )
  memory_time  = (loaded_slices + evicted_slices) / slow_memory_bandwidth
  step_latency = max(compute_time, memory_time)

subgraph_latency = sum(step_latency for all steps)
```

Intra-subgraph data reuse (raster/snake traversal) reduces `memory_time`
by keeping resident input strips that don't change between adjacent tiles.

### Key Source Files

| File | Purpose |
|------|---------|
| `backend/rust/src/models.rs` | Core data types (Problem, Op, Tensor, Solution, SubgraphDef, Granularity) |
| `backend/rust/src/parser.rs` | JSON -> Problem deserialisation |
| `backend/rust/src/dag.rs` | DAG topology (topological sort, boundary tensors, cycle detection) |
| `backend/rust/src/latency.rs` | Subgraph latency + memory working-set calculation |
| `backend/rust/src/memory.rs` | OOM check |
| `backend/rust/src/optimizer/fusion.rs` | Greedy chain fusion |
| `backend/rust/src/optimizer/retention.rs` | Tensor retention optimisation |
| `backend/rust/src/optimizer/splitk.rs` | Split-K for OOM MatMuls |
| `backend/rust/src/optimizer/granularity.rs` | Granularity grid search |
| `backend/rust/src/optimizer/traversal.rs` | Snake/zig-zag tile ordering |
| `backend/rust/src/optimizer/pipeline.rs` | Pipeline orchestration |
| `backend/rust/src/serializer.rs` | Solution -> JSON serialisation |
| `agent/evaluator.py` | Python latency model (mirrors Rust logic) |
| `agent/scheduler.py` | Python optimizer pipeline |
| `agent/agent.py` | Gemini agent loop |
| `agent/prompts/` | System prompt, examples, strategies for Gemini |

---

## Project Structure

```
solution/
├── backend/
│   └── rust/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs              # Entry point + 15 unit tests
│           ├── models.rs
│           ├── parser.rs
│           ├── dag.rs
│           ├── latency.rs
│           ├── memory.rs
│           ├── evaluate.rs
│           ├── serializer.rs
│           ├── baseline.rs
│           └── optimizer/
│               ├── mod.rs
│               ├── pipeline.rs
│               ├── fusion.rs
│               ├── retention.rs
│               ├── splitk.rs
│               ├── granularity.rs
│               └── traversal.rs
├── agent/
│   ├── agent.py                     # Track B entry point
│   ├── evaluator.py                 # Python latency model
│   ├── scheduler.py                 # Python optimizer
│   ├── requirements.txt             # google-genai>=1.0.0
│   └── prompts/
│       ├── system.md
│       ├── examples.md
│       └── strategies.md
├── scripts/
│   └── test-e2e.sh                  # E2E happy-path test (both tracks)
├── docs/
│   ├── architecture/
│   └── decisions/
├── .github/
│   └── workflows/
│       └── ci.yml
├── README.md
└── CHANGELOG.md
```

---

## Testing

### Track A — Rust Unit Tests (15 tests)

```bash
cd solution/backend/rust
cargo test
```

Tests cover:

- Example 1 (baseline): strategies A, B, C with expected latencies
- Example 2 (larger tensors): strategies A and B
- Example 3 (diamond graph): spilling and selective retention
- Example 4 (MatMul with spatial tiling): naive tiling
- Example 5 (chained MatMul, split-K): split-K granularity
- Edge cases: single tiny op, OOM detection, serialization round-trip,
  ephemeral tensor correctness, cyclic DAG rejection
- All 5 released benchmarks: full pipeline validity (coverage + non-negative latencies)

### Track B — Python Tests

The Python evaluator and scheduler can be tested from the agent directory:

```bash
cd solution/agent
uv venv
uv pip install -r requirements.txt

# Smoke-test imports
uv run python -c "from evaluator import *; from scheduler import build_baseline, optimize; print('OK')"

# Run against a benchmark (no API key needed)
GOOGLE_API_KEY=dummy uv run python agent.py \
    ../../problem/benchmarks/mlsys-2026-1.json /tmp/out-b.json
```

### E2E Happy-Path Script (both tracks, 13 checks)

```bash
# From project root
bash solution/scripts/test-e2e.sh
```

This script:
1. Builds the Rust binary (`cargo build --release`)
2. Runs all 5 benchmarks through Track A, validates JSON output
3. Verifies Track B Python imports
4. Runs all 5 benchmarks through Track B (baseline mode), validates JSON output

Validation checks per output file:
- Required keys present (`subgraphs`, `granularities`, `tensors_to_retain`, `subgraph_latencies`)
- At least one subgraph
- All latencies non-negative
- All granularity components positive
- No duplicate op assignments

---

## Benchmark Results Summary

All 5 released benchmarks produce valid solutions within the memory constraint.
Reported latencies are from Track A (Rust) on the local machine.

| Benchmark | Ops | Tensors | Fast Mem | Bandwidth | Track A Latency |
|-----------|-----|---------|----------|-----------|-----------------|
| mlsys-2026-1  | 5   | 9   | 60,000   | 20  | 112,000   |
| mlsys-2026-5  | 19  | 29  | 30,000   | 15  | 147,200   |
| mlsys-2026-9  | 32  | 49  | 250,000  | 25  | 1,369,600 |
| mlsys-2026-13 | 63  | 100 | 600,000  | 50  | 3,864,000 |
| mlsys-2026-17 | 103 | 160 | 500,000  | 100 | 1,452,400 |

All benchmarks complete in under 1 second. The optimizer fuses adjacent
chains, applies Split-K for memory-constrained MatMuls, searches tile
granularities to balance compute/memory costs, and uses snake traversal
for MatMul data reuse.

---

## Environment Variables

| Variable | Track | Description |
|----------|-------|-------------|
| `GOOGLE_API_KEY` | B | Gemini API key. Set to `dummy` to run local optimizer only. |

---

## Dependencies

### Track A (Rust)

| Crate | Version | Purpose |
|-------|---------|---------|
| `serde` | 1.x | Derive serialise/deserialise traits |
| `serde_json` | 1.x | JSON parsing and serialisation |

No async runtime, no HTTP client, no external services required.

### Track B (Python)

| Package | Version | Purpose |
|---------|---------|---------|
| `google-genai` | >=1.0.0 | Gemini API client |

All other logic (latency model, optimizer, DAG) is pure Python stdlib.
