# Stage 1: Requirements Analysis

## Summary
- **Status**: COMPLETE
- **Documents Created**: 4 (requirements.md, rice-scores.md, moscow.md, mvp-scope.md)
- **Features Identified**: 15
- **MVP Features**: 7 Must-Have + 5 Should-Have = 12 features in scope

## Artifacts

| File | Description |
|------|-------------|
| `requirements/requirements.md` | 29 functional requirements, 6 NFRs, 6 hidden requirements, 8 assumptions |
| `requirements/rice-scores.md` | RICE prioritization of all 15 features with scoring rationale |
| `requirements/moscow.md` | MoSCoW categorization with explicit Won't-Have justifications |
| `requirements/mvp-scope.md` | MVP definition, dependency graph, acceptance criteria, risk register |

## Key Decisions

- **Baseline first**: Generating a trivially correct schedule (one op per subgraph, native
  granularity) is a Must-Have; it guarantees a valid submission exists even if optimizations
  fail.
- **Regression tests before optimizer**: The 5 worked examples in PROBLEM.md must pass as
  tests before any optimization code is written. This is the only latency ground truth.
- **Greedy fusion as MVP optimizer**: Full DP/beam-search optimization is deferred (Won't Have
  for MVP). Greedy bottom-up chain fusion + tensor retention + split-K covers the primary
  latency reduction levers with manageable implementation effort.
- **Python implementation**: Rapid prototyping in Python; C++ `Evaluate()` in `mlsys.h` is
  the authoritative evaluator and will be used as the external validator.
- **Traversal order deferred**: ~8% marginal gain (per Example 4B); deferred to Could-Have.
- **Recomputation deferred**: Could-Have; selective residency (F-07) achieves similar gains
  in most graphs without the complexity of graph rewriting.

## Risks Identified

- Latency model may diverge from `Evaluate()` on edge cases (accumulator accounting, padding
  model) — mitigated by comprehensive regression tests on all 5 examples.
- Greedy fusion may miss optimal groupings on complex graphs (benchmarks 13 and 17 have 48+
  and 95+ operations) — mitigated by benchmarking against baseline and targeted tuning.
- Working-set accounting for retained tensors and split-K accumulators is subtle — explicit
  test cases from Examples 3C and 5B provide ground truth.
- Benchmark 17 topology (transformer-like with repeating attention blocks) may require
  pattern-specific fusion heuristics rather than generic greedy fusion.

## Ready for Stage 2: Yes
