# MoSCoW Prioritization

## Must Have (Critical — system produces no valid output without these)

- [ ] **F-14: Topological sort of the DAG**
  - **Why Must**: All scheduling decisions depend on a valid execution order. No schedule can
    be generated without it.

- [ ] **F-01: Problem JSON parser**
  - **Why Must**: The scheduler cannot operate without reading the input. Every benchmark
    requires this.

- [ ] **F-11: Solution JSON serializer**
  - **Why Must**: The evaluator (`Evaluate()`) reads a JSON file. Without serialization,
    no score can be produced.

- [ ] **F-02: Latency model (roofline)**
  - **Why Must**: The `subgraph_latencies` field in the output is required and validated by
    `Evaluate()`. Incorrect latencies make the solution invalid. The latency model is also
    needed for the optimizer to compare schedule alternatives.

- [ ] **F-03: Working-set calculator and OOM guard**
  - **Why Must**: Any granularity or grouping choice that exceeds `fast_memory_capacity`
    causes an OOM crash. The scheduler must check this constraint before emitting any
    subgraph.

- [ ] **F-04: Baseline scheduler (1 op/subgraph, native granularity, no retention)**
  - **Why Must**: A trivially correct schedule is needed (a) to guarantee a valid submission
    exists for all benchmarks, and (b) as a correctness baseline to test the latency model
    against before adding optimizations.

- [ ] **F-13: Regression tests for the 5 worked examples**
  - **Why Must**: The five examples in PROBLEM.md are the only ground-truth latency
    calculations provided. These tests are the only way to validate the latency model
    implementation before running on benchmarks.

---

## Should Have (Important — significant latency reduction, system works without them but scores poorly)

- [ ] **F-05: Op grouping / chain fusion**
  - **Why not Must**: The baseline scheduler works without it. But fusion is the single
    largest latency lever: Example 1B shows 2x improvement from grouping two ops. Without
    fusion, scores will be far below optimal on every benchmark.
  - **What works without it**: A valid (but unoptimized) submission exists.

- [ ] **F-07: Tensor retention across subgraph boundaries**
  - **Why not Must**: Can always evict everything. But retention eliminates load-evict cycles:
    Example 3C shows Subgraph 0 latency halved (from 3,276.8 to 1,638.4) by keeping Tensor 1
    resident.
  - **What works without it**: Valid submission, higher latency.

- [ ] **F-08: Split-K (small-k MatMul accumulation)**
  - **Why not Must**: For graphs without memory-constrained chained MatMuls, split-K is
    optional. However, Example 5 shows it is the only strategy that avoids OOM for chained
    MatMuls when three full tensors exceed `fast_memory_capacity`. Benchmarks 1, 9, 13, and
    17 all contain MatMul ops with potentially tight memory.
  - **What works without it**: Fall back to not fusing those MatMuls (valid but slower).

- [ ] **F-06: Granularity search (selecting w, h, k)**
  - **Why not Must**: Defaulting to native granularity `[128, 128, K_full]` is valid. But
    the right sub-native granularity can switch bottleneck from memory-bound to compute-bound
    and enable fusing ops that would otherwise OOM.
  - **What works without it**: Valid submission using only native granularity.
  - **k search requirement (Issue #15)**: The granularity search must include the `k`
    dimension for MatMul subgraphs. The search sweeps `k` from `K_full` down to 1 in powers
    of 2 and selects the `(w, h, k)` that minimizes total subgraph latency within the OOM
    constraint. Larger `k` is preferred as a tie-breaker when latencies are equal, because
    `k = 1` is pathologically bad — it produces `K_full` micro-steps each loading tiny strips,
    increasing memory traffic by a factor of `K_full` relative to a single full-K step.

- [ ] **F-12: Benchmark runner (reads problem + solution, calls Evaluate)**
  - **Why not Must**: Could manually verify each output. But without a runner, iteration speed
    on all 5 benchmarks is very slow and error-prone.
  - **What works without it**: Manual JSON inspection.

---

## Could Have (Nice to have — additional latency reductions, add if time permits)

- [ ] **F-09: Recomputation (op included in multiple subgraphs)**
  - Why: Avoids materializing a shared intermediate tensor in slow memory (diamond graphs).
    Example 3B and 3C demonstrate up to 60% improvement over naive spilling. Benchmark 1 has
    diamond topology; benchmark 17's residual connections also benefit.
  - Why not Should: Recomputation increases compute cost; whether it's net-positive depends
    on the graph structure. Selective-residency (Should Have) often achieves similar gains
    with less complexity.

- [ ] **F-10: Traversal order optimization (snake/zig-zag)**
  - Why: Reduces MatMul input-strip revisits, yielding ~8% latency reduction (Example 4B).
  - Why not Should: Marginal gain (~8%) on a single subgraph, and only beneficial when spatial
    tiling is used. Higher priorities offer more return per hour.

- [ ] **F-15: Advanced optimizer strategy (DP or beam search over grouping choices)**
  - Why: A greedy pass may miss globally optimal groupings. DP over DAG subsets could find
    the provably optimal schedule for smaller graphs.
  - Why not Should: Extremely high implementation effort (16h estimated). Greedy fusion with
    retention (Should Haves) already captures most of the gain. Reserved as a stretch goal.

---

## Won't Have (Explicit exclusions for this iteration)

- **Parallel subgraph execution**: The problem explicitly states strict serialization between
  subgraphs. No concurrent execution model is needed or valid.
  — *When to reconsider*: Never, for this problem specification.

- **Double-buffering modeling**: The problem states the hardware manages physical
  double-buffering transparently; the scheduler treats `fast_memory_capacity` as the logical
  usable space.
  — *When to reconsider*: If the problem specification changes to expose physical overhead.

- **Support for op types beyond MatMul and Pointwise**: All 5 benchmark files use only these
  two types.
  — *When to reconsider*: If the contest adds new op types.

- **General DAG grouping (non-chain subgraphs)**: The problem's subgraph execution model
  requires a linear execution order within a subgraph (operations feed into each other in
  sequence). Arbitrary DAG subsets cannot be made ephemeral in the general case.
  — *When to reconsider*: If the hardware model is extended to support multi-input subgraphs
    with non-ephemeral intermediates.

- **Online/streaming scheduling**: All benchmark inputs are fully specified upfront. No
  streaming or partial-graph scheduling is needed.

- **Multi-device or distributed execution**: The model is a single accelerator with one fast
  memory region.

- **Exact arithmetic / symbolic verification of latency**: The problem uses floating-point
  latency values (doubles). Symbolic or exact integer arithmetic is not required.
