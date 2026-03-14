# Deployment Topology

This is a local CLI tool. There is no server, container, or cloud deployment.

## Tracks

There are two independent implementations with separate entry points.

---

## Track A: Rust Binary

### Build

```bash
cd solution/backend/rust
cargo build --release
# Produces: solution/backend/rust/target/release/mlsys
```

The release profile is configured with `opt-level = 3`, `lto = true`, and `codegen-units = 1` for a fully optimized, statically linked binary.

### CLI Interface

**Solve mode** — read a problem JSON and write an optimized solution JSON:

```bash
./mlsys <input.json> <output.json>
```

**Evaluate mode** — validate an existing solution JSON against a problem JSON:

```bash
./mlsys evaluate --problem <input.json> --solution <solution.json>
```

### Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `serde` | 1.x | Derive macros for serialization |
| `serde_json` | 1.x | JSON parsing and serialization |

No external services, no database, no network calls.

### Environment Variables

None required. Track A is self-contained.

---

## Track B: Python Agent

### Setup

```bash
cd solution/agent
uv venv
uv add google-genai --active
```

### CLI Interface

```bash
uv run python agent.py <input.json> <output.json>
```

The agent:
1. Parses the problem JSON
2. Generates a locally-optimized baseline schedule (no API call)
3. Writes the baseline to the output file immediately (safe fallback)
4. Calls Gemini to reason about further optimizations (if `GOOGLE_API_KEY` is set)
5. Validates each Gemini suggestion locally
6. Writes the best valid solution found within the 9-minute time budget

### Dependencies

| Package | Purpose |
|---------|---------|
| `google-genai` | Gemini API client |

### Environment Variables

| Variable | Required | Purpose |
|----------|----------|---------|
| `GOOGLE_API_KEY` | Yes (for Gemini) | Authenticates calls to the Gemini API. If absent or set to `"dummy"`, the agent skips Gemini rounds and outputs the local baseline. |

---

## Project File Layout

```
solution/
    backend/
        rust/
            Cargo.toml              # serde, serde_json dependencies
            src/
                main.rs             # CLI entry point (solve + evaluate subcommands)
                models.rs           # Rust structs (Problem, Tensor, Op, etc.)
                parser.rs           # JSON -> Problem
                serializer.rs       # Solution -> JSON
                dag.rs              # DAG utilities (Kahn's topo sort, adjacency)
                latency.rs          # Roofline latency model
                memory.rs           # Working-set calculator, OOM checker
                baseline.rs         # Naive 1-op-per-subgraph schedule
                evaluate.rs         # Solution evaluator (evaluate subcommand)
                optimizer/
                    mod.rs
                    pipeline.rs     # 9-stage optimizer orchestration
                    fusion.rs       # Greedy chain fusion
                    retention.rs    # Tensor retention decisions
                    splitk.rs       # Split-K for MatMul OOM relief
                    granularity.rs  # w/h/k search
                    traversal.rs    # Snake/zig-zag tile order
            target/
                release/
                    mlsys           # Compiled binary (after cargo build --release)
    agent/
        agent.py                    # Track B entry point
        evaluator.py                # Local latency/OOM evaluator (Python)
        scheduler.py                # Local optimizer (Python baseline + optimize)
        prompts/
            system.md
            examples.md
            strategies.md
        requirements.txt            # google-genai>=1.0.0

problem/
    PROBLEM.md
    mlsys.h
    example_problem.json
    benchmarks/
        mlsys-2026-1.json
        mlsys-2026-5.json
        mlsys-2026-9.json
        mlsys-2026-13.json
        mlsys-2026-17.json
```

## "Production" Context

In the contest context, "production" means:
1. Solutions are submitted as JSON files
2. The contest infrastructure runs the C++ `Evaluate()` function on our JSON output
3. Track A (`mlsys` binary) runs locally on the contestant's machine; Track B (`agent.py`) runs locally and calls the Gemini API
