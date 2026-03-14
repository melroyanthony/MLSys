# Stage 2: Architecture & System Design

## Summary
- **Status**: COMPLETE
- **System Type**: Computational optimization CLI tool (not a web service)
- **Scale**: 5 benchmarks, 2-96 ops, 3-160 tensors per problem
- **Track A**: 16 Rust modules in `src/` + `src/optimizer/`
- **Track B**: 3 Python modules + prompts
- **ADRs**: 3

## Artifacts

| File | Description |
|------|-------------|
| `docs/architecture/system-design.md` | Module decomposition, Rust data model, algorithm pipeline, latency model spec |
| `docs/architecture/database-schema.md` | Data model reference: input/output JSON schemas, C++ to Rust mapping |
| `docs/architecture/data-flow.md` | Pipeline sequence diagram, 9-stage optimizer composition |
| `docs/architecture/user-journeys.md` | 3 user journeys: solve, evaluate, batch run |
| `docs/architecture/workspace.dsl` | C4 model (Structurizr DSL) showing module dependencies |
| `docs/architecture/api-error-catalog.md` | CLI error codes and Rust error handling |
| `docs/architecture/security-model.md` | Threat model (minimal — local CLI tool) |
| `docs/architecture/deployment-topology.md` | File layout, CLI usage, build instructions |
| `docs/decisions/ADR-001-language-selection.md` | Rust for Track A, Python for Track B |
| `docs/decisions/ADR-002-baseline-first.md` | Why correct baseline before optimization |
| `docs/decisions/ADR-003-greedy-fusion.md` | Why greedy fusion over DP/beam search |

## Key Decisions

1. **Rust (Track A) + Python (Track B)**: Rust for performance and static binary; Python for Gemini API integration.
2. **Baseline-first development**: Trivially correct schedule (1 op/subgraph) before any optimizer, validated against PROBLEM.md examples.
3. **Greedy bottom-up fusion**: O(N^2) fusion captures most of the benefit for repeating-block DAG structures seen in benchmarks.
4. **9-stage pipeline**: Baseline → Fusion → Retention → Split-K → Granularity → Retention (pass 2) → Emergency OOM → Final Latency → Traversal.

## Patterns Selected

- **Pipeline pattern**: Fixed sequence of 9 optimizer stages, each refining the schedule
- **Strategy pattern**: Each optimizer stage is independent and can be enabled/disabled
- **Data model mirroring**: Rust structs mirror C++ structs from mlsys.h exactly
- **Regression testing**: All 5 PROBLEM.md examples as ground-truth test cases

## Critical Formulas Documented

- Spatial tile count: `ceil(W/w) * ceil(H/h)`
- K-steps: `ceil(K_full/k)` for MatMul, 1 for Pointwise
- Compute per step: `base_cost * (k/K_full)` for each MatMul (per-op K_full), `base_cost` for Pointwise
- Memory per step: `(elements_in + elements_out) / bandwidth`
- Roofline: `max(compute_time, memory_time)` per step
- Total: sum across all steps and subgraphs

## Benchmark Analysis

| Benchmark | Ops | Tensors | Key Challenge |
|-----------|-----|---------|---------------|
| 1 | 5 | 9 | Mixed MatMul/PW chain, moderate memory |
| 5 | 19 | 29 | Multi-head attention with tight memory (30K) |
| 9 | 32 | 49 | Large tensors (4096x4096), 8x repeating blocks |
| 13 | 63 | 100 | 16x parallel heads, generous memory (600K) |
| 17 | 103 | 160 | Complex attention+MLP, largest graph |

## Ready for Stage 3: Yes
