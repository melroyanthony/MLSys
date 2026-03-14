# Stage 2: Architecture & System Design

## Summary
- **Status**: COMPLETE
- **System Type**: Computational optimization CLI tool (not a web service)
- **Scale**: 5 benchmarks, 2-96 ops, 3-160 tensors per problem
- **Modules**: 13 Python modules in 2 packages
- **ADRs**: 3

## Artifacts

| File | Description |
|------|-------------|
| `docs/architecture/system-design.md` | Module decomposition, data model, algorithm pipeline, latency model spec, all key formulas |
| `docs/architecture/database-schema.md` | Data model reference: input/output JSON schemas, C++ to Python mapping, internal structures |
| `docs/architecture/data-flow.md` | Pipeline sequence diagram, per-subgraph latency flowchart, working-set check flow |
| `docs/architecture/user-journeys.md` | 3 user journeys: solve, evaluate, batch run |
| `docs/architecture/workspace.dsl` | C4 model (Structurizr DSL) showing module dependencies |
| `docs/architecture/api-error-catalog.md` | CLI error codes and exception catalog |
| `docs/architecture/security-model.md` | Threat model (minimal -- local CLI tool) |
| `docs/architecture/deployment-topology.md` | File layout, CLI usage, dependencies |
| `docs/decisions/ADR-001-language-python.md` | Why Python for the scheduler |
| `docs/decisions/ADR-002-baseline-first.md` | Why correct baseline before optimization |
| `docs/decisions/ADR-003-greedy-fusion.md` | Why greedy fusion over DP/beam search |

## Key Decisions

1. **Python + pure stdlib**: No external dependencies beyond pytest. Fast iteration, sufficient performance for N<=96 ops.
2. **Baseline-first development**: Trivially correct schedule (1 op/subgraph) before any optimizer, validated against PROBLEM.md examples.
3. **Greedy bottom-up fusion**: O(N^2) fusion captures most of the benefit for repeating-block DAG structures seen in benchmarks.
4. **Pipeline architecture**: Parse -> DAG -> Baseline -> Fusion -> Retention -> Split-K -> Granularity -> Serialize. Each stage is a pure function that only improves the schedule.

## Patterns Selected

- **Pipeline pattern**: Fixed sequence of optimizer stages, each refining the schedule
- **Strategy pattern**: Each optimizer stage is independent and can be enabled/disabled
- **Data model mirroring**: Python dataclasses mirror C++ structs from mlsys.h exactly
- **Regression testing**: All 5 PROBLEM.md examples as ground-truth test cases

## Critical Formulas Documented

- Spatial tile count: `ceil(W/w) * ceil(H/h)`
- K-steps: `ceil(K_full/k)` for MatMul, 1 for Pointwise
- Compute per step: `base_cost` (spatial padding has no discount) with `k/K_full` scaling for MatMul
- Memory per step: `(bytes_in + bytes_out) / bandwidth`
- Roofline: `max(compute_time, memory_time)` per step
- Total: sum across all steps and subgraphs

## Benchmark Analysis

| Benchmark | Ops | Tensors | Key Challenge |
|-----------|-----|---------|---------------|
| 1 | 5 | 9 | Mixed MatMul/PW chain, moderate memory |
| 5 | 19 | 29 | Multi-head attention with tight memory (30K) |
| 9 | 32 | 49 | Large tensors (4096x4096), 8x repeating blocks |
| 13 | 63 | 96 | 16x parallel heads, generous memory (600K) |
| 17 | 96 | 160 | Complex attention+MLP, largest graph |

## Ready for Stage 3: Yes
