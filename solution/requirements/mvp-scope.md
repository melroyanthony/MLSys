# MVP Scope Definition

## Impact Map

Goal: Minimize total latency across all 5 benchmark graphs while producing valid, evaluatable
JSON schedules.

```
Goal: Lowest total latency on MLSys-2026 benchmarks
├── Contest Judges
│   └── Impact: Receive a valid JSON solution file for each benchmark
│       ├── Deliverable: Problem parser + solution serializer (F-01, F-11)
│       └── Deliverable: Baseline scheduler that always produces a correct output (F-04)
│
├── Latency Optimizer
│   └── Impact: Reduce latency vs. naive baseline
│       ├── Deliverable: Op grouping / chain fusion (F-05)
│       ├── Deliverable: Tensor retention across subgraphs (F-07)
│       ├── Deliverable: Split-K for memory-constrained MatMuls (F-08)
│       └── Deliverable: Granularity search for OOM avoidance and compute/memory balance (F-06)
│
└── Developer
    └── Impact: Verify correctness quickly and iterate safely
        ├── Deliverable: Latency model with regression tests (F-02, F-13)
        ├── Deliverable: Working-set calculator with OOM guard (F-03)
        └── Deliverable: Benchmark runner (F-12)
```

---

## Value-Effort Matrix Summary

| Category | Features |
|----------|---------|
| **Quick Wins** (high value, low effort) | F-14 topological sort, F-01 parser, F-11 serializer, F-02 latency model, F-04 baseline scheduler |
| **Big Bets** (high value, high effort) | F-05 op grouping, F-08 split-K, F-06 granularity search, F-15 advanced optimizer |
| **Fill-ins** (low value, low effort) | F-10 traversal order optimization |
| **Deferred** (lower priority) | F-09 recomputation, F-15 DP/beam optimizer |

---

## Included Features (ordered by dependency then priority)

| # | Feature | Acceptance Criteria | Est. (h) | Depends On |
|---|---------|--------------------|----|-----------|
| 1 | **F-14: Topological sort** | Given any valid DAG, returns operations in a valid linearized order (tested on examples 1–5) | 1 | none |
| 2 | **F-01: Problem JSON parser** | Reads all 5 benchmark files + example_problem.json without error; reconstructs `Problem` struct with correct tensor/op counts and hardware params | 2 | none |
| 3 | **F-02: Latency model** | Passes all 5 worked-example test cases from PROBLEM.md with latencies matching to 0.1 precision; handles compute-bound, memory-bound, and tiled (multi-step) cases correctly | 4 | F-01 |
| 4 | **F-03: Working-set calculator** | Given a subgraph definition + resident tensor set + granularity, returns working-set size and raises OOM flag if > `fast_memory_capacity`; verified against all 5 examples | 3 | F-01, F-02 |
| 5 | **F-13: Regression tests** | Test suite covers all 5 PROBLEM.md examples (Strategies A/B/C where given); all tests pass before optimizer code is written | 3 | F-02, F-03 |
| 6 | **F-11: Solution JSON serializer** | Writes well-formed JSON matching the output schema; round-trips through a JSON validator; `null` traversal_orders serialize correctly | 2 | none |
| 7 | **F-04: Baseline scheduler** | Produces one valid subgraph per operation; uses native granularity `[128, 128, K_full]`; `tensors_to_retain = []` for all; latency values match model; no OOM on any benchmark | 2 | F-01, F-02, F-03, F-11, F-14 |
| 8 | **F-12: Benchmark runner** | CLI that accepts `--problem FILE --solution FILE`, calls evaluate logic, prints total latency and pass/fail | 3 | F-01, F-11 |
| 9 | **F-05: Op grouping / chain fusion** | Greedy bottom-up fusion: group adjacent ops in topological order if merged working set fits in fast memory; verify latency improves vs. baseline on all 5 benchmarks | 6 | F-14, F-03, F-02 |
| 10 | **F-07: Tensor retention** | After each subgraph, determine which output tensors are consumed by the immediately following subgraph and have sufficient residual capacity; retain them; verify improvement on Example 3C pattern | 4 | F-05, F-03 |
| 11 | **F-08: Split-K** | For MatMul subgraphs where full-k working set exceeds capacity, search for the largest `k` divisor that fits; model accumulator as resident across k-steps; verify Example 5B latency | 5 | F-05, F-03, F-02 |
| 12 | **F-06: Granularity search** | For each subgraph, try candidate `[w, h]` values (powers of 2 up to tensor dimensions); **for MatMul subgraphs, also search `k` from `K_full` down to 1 in powers of 2 (Issue #15 fix) — k must not be hardcoded to 1**; select the `[w, h, k]` combination that minimises subgraph latency within the OOM constraint; larger k values are preferred as they reduce per-step memory traffic; verify Example 1C pattern and that k > 1 is chosen for MatMul ops where the memory budget allows | 8 | F-05, F-03, F-02 |

**Total MVP Estimated Effort: 43 hours**

---

## Acceptance Criteria

- [ ] All 5 PROBLEM.md example test cases pass with latencies matching to within 0.1 of the
  stated values
- [ ] All 5 benchmark JSON files are parsed without error
- [ ] A valid solution JSON is produced for every benchmark (no OOM, all ops covered)
- [ ] Total latency on every benchmark is strictly lower than the naive baseline (1 op per
  subgraph, native granularity, no retention)
- [ ] The `subgraph_latencies` values in every output JSON match the latency model to within
  floating-point tolerance (validated by `Evaluate()` or the Python re-implementation)
- [ ] No solution contains a working set exceeding `fast_memory_capacity` for any benchmark
- [ ] For MatMul subgraphs, the chosen `k` is the largest power-of-2 divisor of `K_full` that
  keeps the working set within `fast_memory_capacity` (k is never 1 unless all larger values
  violate OOM)

---

## User Journey (Happy Path)

```
1. Scheduler reads problem JSON         → Parses Problem struct with tensors, ops, hw params
2. Topological sort                     → Linearized op execution order
3. Baseline schedule generated          → Valid JSON, all ops covered, no OOM
4. Greedy chain fusion applied          → Adjacent ops merged where memory allows
5. Tensor retention decided             → Downstream-needed tensors flagged as resident
6. Split-K applied to MatMul subgraphs  → k reduced to fit tight memory budgets
7. Granularity search per subgraph      → Best [w, h, k] selected per latency model;
                                           k searched from K_full downward for MatMul ops
8. Latency calculated for each subgraph → subgraph_latencies list populated
9. Solution JSON written                → Ready for Evaluate() call
10. Benchmark runner reports total      → Score vs. baseline shown; validated correct
```

---

## Out of Scope (with justification)

| Item | Why excluded | When to reconsider |
|------|--------------|--------------------|
| ~~Traversal order optimization (F-10)~~ | ~~~8% marginal gain~~ **IMPLEMENTED** as Stage 9 (snake/zig-zag traversal) | N/A — implemented |
| Recomputation / graph-rewrite (F-09) | Higher complexity, benefit depends on diamond graph frequency; selective residency (F-07) covers most of the same gain | If benchmarks 1 or 17 show large latency gaps attributable to shared intermediates |
| Advanced optimizer: DP/beam search (F-15) | 16h estimated effort, uncertain gain over greedy given contest time constraints | If greedy fusion produces solutions >20% above known-optimal reference |
| Multi-device / parallel subgraph execution | Explicitly excluded by the problem's strict serialization model | Never (hard problem constraint) |
| ~~C++ reimplementation~~ | ~~Python is sufficient~~ **SUPERSEDED**: Track A implemented in Rust for performance and static binary requirement | N/A — Rust chosen per ADR-001 |

---

## Estimated Effort by Stage

| Stage | Activities | Est. (h) |
|-------|-----------|----------|
| Stage 1 — Requirements | This document set | 0.5 |
| Stage 2 — Architecture | Data model design, module structure, latency model spec | 2 |
| Stage 3 — Implementation | F-14, F-01, F-02, F-03, F-04, F-11, F-12, F-05, F-07, F-08, F-06 | 43 |
| Stage 4 — Testing | Regression tests (F-13), benchmark runs, latency verification | 5 |
| Stage 5 — Finalization | Code review, documentation, solution JSON submission | 2 |
| **Total** | | **52.5** |

---

## Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Latency model does not match `Evaluate()` due to undocumented edge cases | M | H | Implement F-13 regression tests against all 5 worked examples before writing optimizer |
| Greedy fusion produces suboptimal groupings leaving significant latency on the table | H | M | Instrument the optimizer to report latency breakdown per subgraph; reserve time for targeted tuning on worst benchmarks |
| Split-K search space is too large for exhaustive search on benchmark 17 (95+ ops, large tensors) | M | M | Limit k candidates to powers of 2; use binary search for the largest k that fits |
| Memory model for retained tensors across subgraph boundaries is mis-specified (e.g., capacity accounting for loaded inputs vs. outputs) | M | H | Validate against Example 3C (selective residency) and Example 5B (split-K with accumulator) before enabling retention |
| Benchmark 17 (160 tensors, 95+ ops, 500K fast memory) has complex topology that greedy fusion mishandles | M | M | Analyze graph structure in architecture stage; design fusion rules for the attention-like repeating pattern observed in benchmarks 9, 13, 17 |
| Working-set formula for subgraphs containing both MatMul and Pointwise ops is incorrectly specified | L | H | Cross-check against Example 5B which has exactly this combination; add a dedicated test case |
| Python runtime too slow for benchmark 17 | L | M | Profile early; use NumPy-free pure Python for the scheduler logic (only arithmetic, no array ops needed) |
| Granularity search defaults to k=1 for MatMul ops (Issue #15) | H | H | Search k from K_full downward; prefer largest k that satisfies OOM; add regression test asserting k > 1 on any benchmark where K_full > 1 and memory allows |
