# RICE Prioritization

## Scoring Methodology

- **Reach** (1–10): How many benchmark problems does this feature directly impact? All 5
  benchmarks = 10. Features that affect only edge-case graphs score lower.
- **Impact** (0.25 / 0.5 / 1 / 2 / 3): Effect on total latency reduction vs. naive baseline.
  3 = transformative (>50% reduction possible). 2 = high (20–50%). 1 = medium (5–20%).
  0.5 = low (<5%). 0.25 = cosmetic.
- **Confidence** (50% / 80% / 100%): How certain are we that this feature delivers the
  claimed impact? Based on worked examples in PROBLEM.md.
- **Effort** (person-hours): Estimated implementation time including tests.

**RICE Score = (Reach x Impact x Confidence) / Effort**

---

## Feature Scores

| Feature | Reach | Impact | Confidence | Effort (h) | Score | Priority |
|---------|-------|--------|------------|------------|-------|----------|
| F-01: Problem JSON parser | 10 | 3 | 100% | 2 | 15.0 | 1 |
| F-02: Latency model (roofline) | 10 | 3 | 100% | 4 | 7.5 | 2 |
| F-03: Working-set calculator + OOM guard | 10 | 3 | 100% | 3 | 10.0 | 3 |
| F-04: Baseline scheduler (1 op/subgraph, native gran) | 10 | 1 | 100% | 2 | 5.0 | 4 |
| F-05: Op grouping / chain fusion | 10 | 3 | 100% | 6 | 5.0 | 5 |
| F-06: Granularity search (w, h, k selection) | 10 | 2 | 80% | 8 | 2.0 | 6 |
| F-07: Tensor retention across subgraphs | 10 | 2 | 100% | 4 | 5.0 | 7 |
| F-08: Split-K (small-k MatMul accumulation) | 10 | 2 | 100% | 5 | 4.0 | 8 |
| F-09: Recomputation (op in multiple subgraphs) | 8 | 2 | 80% | 5 | 2.6 | 9 |
| F-10: Traversal order optimization (snake/zig-zag) | 7 | 1 | 80% | 5 | 1.1 | 10 |
| F-11: Solution JSON serializer | 10 | 3 | 100% | 2 | 15.0 | 1 (tie) |
| F-12: Benchmark runner + Evaluate() integration | 10 | 1 | 100% | 3 | 3.3 | 11 |
| F-13: Example-based regression tests (5 examples) | 10 | 2 | 100% | 3 | 6.7 | 12 |
| F-14: Topological sort (DAG traversal) | 10 | 3 | 100% | 1 | 30.0 | 0 (unblocks all) |
| F-15: Optimizer / search strategy (greedy/DP/heuristic) | 10 | 3 | 80% | 16 | 1.5 | 13 |

---

## Sorted by Score (descending)

| Rank | Feature | Score | Notes |
|------|---------|-------|-------|
| 1 | F-14: Topological sort | 30.0 | Lowest effort, unblocks everything |
| 2 | F-01: Problem JSON parser | 15.0 | Gate to all other work |
| 2 | F-11: Solution JSON serializer | 15.0 | Gate to evaluation |
| 3 | F-02: Latency model (roofline) | 7.5 | Core correctness |
| 4 | F-13: Regression tests | 6.7 | Validate latency model early |
| 5 | F-03: Working-set calculator | 10.0 | Prevents invalid schedules |
| 6 | F-05: Op grouping / chain fusion | 5.0 | Primary latency reduction lever |
| 6 | F-07: Tensor retention | 5.0 | Avoids double-transfer cost |
| 7 | F-04: Baseline scheduler | 5.0 | Correctness baseline |
| 8 | F-08: Split-K | 4.0 | Enables fusing memory-constrained MatMuls |
| 9 | F-12: Benchmark runner | 3.3 | Enables scoring |
| 10 | F-09: Recomputation | 2.6 | Useful for diamond graphs (benchmarks 1, 9) |
| 11 | F-06: Granularity search | 2.0 | Needed but high effort |
| 12 | F-15: Optimizer strategy | 1.5 | High effort, uncertain return |
| 13 | F-10: Traversal order | 1.1 | Marginal gain (~8% per example 4B) |

---

## Scoring Rationale

- **F-14 (Topological sort)**: 1 hour of well-understood algorithm work that is a hard
  prerequisite for subgraph ordering, reachability analysis, and the scheduler loop. Highest
  RICE because effort is minimal and impact is total.

- **F-01 / F-11 (Parser + Serializer)**: Without these two, nothing can be evaluated. Both
  are low-effort, deterministic I/O tasks with 100% confidence and total reach.

- **F-02 (Latency model)**: The roofline formula (`max(compute, memory)` per step, summed
  over steps) is fully specified in the problem with five concrete examples to validate
  against. 4 hours covers implementation + test-case verification.

- **F-03 (Working-set calculator)**: Prevents OOM, which is the only hard constraint in the
  problem. Any schedule that violates this is invalid. Must be implemented before any
  optimizer attempts subgraph fusion.

- **F-05 (Op grouping)**: The single largest latency lever. Example 1B vs 1A shows a 2x
  improvement just from grouping two pointwise ops. Example 3C (selective residency) achieves
  a 60% reduction. The fundamental correctness requirement is that grouped ops form a valid
  chain respecting the DAG.

- **F-07 (Tensor retention)**: Keeping tensors resident across subgraphs eliminates an
  entire load-evict cycle. Example 3C shows Subgraph 0 latency dropping from 3,276.8 to
  1,638.4 by retaining Tensor 1. High impact, medium effort.

- **F-08 (Split-K)**: Required for benchmark 5 (chained MatMuls with tight memory). Without
  split-K, many fused MatMul subgraphs OOM. Example 5B demonstrates the technique.

- **F-09 (Recomputation)**: Useful specifically when a tensor is consumed by two branches
  (diamond graphs). Example 3B and 3C show the recomputation trade-off. Benchmark 1 has
  diamond topology. 80% confidence because the benefit vs. cost depends heavily on graph
  structure.

- **F-06 (Granularity search)**: Choosing sub-native spatial granularity can switch the
  bottleneck from memory-bound to compute-bound, sometimes reducing total latency (Example
  1C). The search space grows quadratically in divisors of tensor dimensions; a heuristic
  approach (try powers of 2, choose minimum working-set granularity) is needed.

- **F-15 (Optimizer strategy)**: The hardest feature with the most uncertain payoff. A greedy
  bottom-up fusion pass is the MVP. DP or beam search could squeeze more performance but at
  substantial implementation cost. Scored 80% confidence at 16 hours — this is the "Big Bet."

- **F-10 (Traversal order)**: Example 4B shows only ~8% improvement over raster order.
  Useful but lowest priority given the small marginal gain.
