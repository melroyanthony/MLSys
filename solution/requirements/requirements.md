# Requirements

## Problem Summary

Design a scheduler that accepts a Directed Acyclic Graph (DAG) of operations (MatMul and
Pointwise) plus hardware parameters, then produces an execution schedule that minimizes total
latency while satisfying a strict fast-memory working-set capacity constraint. The hardware
models a 3-tier memory hierarchy (slow memory, fast memory, ephemeral in-subgraph data).
Performance is determined by a roofline model: each subgraph's latency is the bottleneck of
compute time vs. memory transfer time, and subgraphs execute strictly serially.

## Stakeholders

- **Primary**: MLSys 2026 contest judges — evaluate schedule quality against reference solutions
  on 5 benchmark graphs (IDs 1, 5, 9, 13, 17)
- **Secondary**: Researchers reproducing or extending the approach post-contest

---

## Functional Requirements

### Input Parsing

- **FR-001**: Parse the JSON problem file into typed in-memory structures matching the `Problem`
  struct defined in `mlsys.h` (`tensors`, `ops`, `fast_memory_capacity`,
  `slow_memory_bandwidth`, `native_granularity`).
- **FR-002**: Derive graph inputs (tensors with no producing op) and graph outputs (tensors with
  no consuming op) automatically from the JSON topology.
- **FR-003**: Support both `"MatMul"` and `"Pointwise"` op types. For MatMul, the first input
  tensor is LHS and the second is RHS (order matters for granularity slicing).
- **FR-004**: Handle benchmarks with up to 100+ operations and 160+ tensors (benchmark 17 has
  160 tensors and 95+ ops) within a single invocation.

### DAG Validation

- **FR-005**: Validate that the operation graph is a DAG (no cycles). Reject cyclic inputs as
  malformed.
- **FR-006**: Verify that every operation in the schedule appears in at least one subgraph, and
  that every operation is covered exactly (duplicate coverage is permitted only via explicit
  recomputation paths).

### Subgraph Grouping

- **FR-007**: Group one or more operations into a subgraph. Operations in a subgraph must form
  a connected, topologically ordered chain (no cross-subgraph data dependencies within the
  group that would require intermediate materialization in fast memory).
- **FR-008**: Support recomputation: an operation may appear in more than one subgraph, causing
  it to be executed multiple times to avoid materializing its output in slow memory.
- **FR-009**: Support selective residency: after a subgraph completes, any subset of its output
  tensors may be listed in `tensors_to_retain` to remain in fast memory for subsequent
  subgraphs; all others are evicted to slow memory.

### Granularity Selection

- **FR-010**: For each subgraph, choose a granularity `[w, h, k]` where `w` and `h` are
  positive integers and `k` is a positive integer.
- **FR-011**: Enforce that `w` and `h` divide the output tensor dimensions without remainder,
  OR correctly model the hardware padding cost when `w` or `h` is smaller than
  `native_granularity` dimensions.
- **FR-012**: For MatMul in a subgraph, the LHS input slice is `(h x k)` and the RHS input
  slice is `(k x w)`. For Pointwise, `k` is treated as 1.
- **FR-013**: Implement split-K: when `k` is less than the full reduction dimension of a
  MatMul, the scheduler must model output-stationary accumulation over multiple k-steps, with
  the accumulator held in fast memory between steps.

### Working-Set Constraint

- **FR-014**: For every subgraph, calculate the working set as the sum of all input slices plus
  the output slice required simultaneously in fast memory during one execution iteration. This
  must not exceed `fast_memory_capacity`.
- **FR-015**: Tensors listed in `tensors_to_retain` from a previous subgraph count against the
  working-set budget of the next subgraph (they are already resident).
- **FR-016**: Intermediate tensors that are ephemeral within a subgraph (produced and consumed
  within the same subgraph) consume zero fast-memory capacity and zero transfer time.
- **FR-017**: Reject (or avoid generating) any granularity choice that causes an OOM — i.e.,
  working set > `fast_memory_capacity`.

### Latency Calculation

- **FR-018**: For each subgraph, calculate `ComputeTime` as the sum of `base_cost` values of
  all grouped operations, multiplied by the number of spatial tiles (determined by how many
  times `[w, h]` tiles the output tensor) and the number of k-steps.
- **FR-019**: Compute hardware padding: if `w < native_granularity.width` or
  `h < native_granularity.height`, the compute cost per tile is the same as for the full
  native tile (no fractional compute savings in spatial dimensions). Reduction dimension `k`
  is streamed and does scale proportionally.
- **FR-020**: For each subgraph execution step, calculate `MemoryTime` as:
  `(bytes_transferred_in + bytes_transferred_out) / slow_memory_bandwidth`. Bytes transferred
  are only for tensors loaded from slow memory (not for retained or ephemeral tensors) and
  tensors evicted to slow memory.
- **FR-021**: For each execution step, subgraph latency contribution is
  `max(ComputeTime_per_step, MemoryTime_per_step)`. The total subgraph latency is the sum
  across all steps (all spatial tiles x k-steps).
- **FR-022**: The caller-provided `subgraph_latencies` list in the output JSON must match the
  latency calculated by `Evaluate()` in `mlsys.h`. Solutions with incorrect latency values
  are invalid.

### Traversal Order

- **FR-023**: Support specifying a custom traversal order (permutation of tile indices) per
  subgraph. The default (null) is raster/row-major order.
- **FR-024**: Implement traversal-order optimization for MatMul: a snake/zig-zag pattern
  reduces the number of input-strip reloads (revisits) by keeping a row or column strip
  resident across adjacent tiles.
- **FR-025**: The traversal order must be a valid permutation of `[0, num_tiles)`.

### Output Serialization

- **FR-026**: Produce a valid JSON output matching the `Solution` struct: parallel lists for
  `subgraphs`, `granularities`, `tensors_to_retain`, `traversal_orders`,
  `subgraph_latencies`.
- **FR-027**: `tensors_to_retain[k]` lists tensor indices (not op indices) to keep resident
  after subgraph `k`. Only inter-subgraph retention needs to be listed; intra-subgraph reuse
  is managed by the hardware automatically.

### Benchmarking and Evaluation

- **FR-028**: Expose a benchmark runner that, given a problem JSON and a solution JSON,
  invokes `Evaluate()` and reports the total latency and any constraint violations.
- **FR-029**: Produce solutions for all five provided benchmark files:
  `mlsys-2026-1.json`, `mlsys-2026-5.json`, `mlsys-2026-9.json`,
  `mlsys-2026-13.json`, `mlsys-2026-17.json`.

---

## Non-Functional Requirements

- **NFR-001 Correctness**: The solution must pass `Evaluate()` with zero constraint violations
  (no OOM, no uncovered operations, no invalid traversal orders).
- **NFR-002 Optimality direction**: The scheduler must produce lower total latency than a naive
  baseline (all-separate subgraphs, full native granularity, no retention). The target is to
  exploit grouping, split-K, residency, and traversal order to the maximum extent feasible
  within the time budget.
- **NFR-003 Runtime**: The scheduler must produce a complete, valid solution for each benchmark
  within a practical time limit (target: under 5 minutes per benchmark on a standard developer
  machine). Benchmarks have up to 100 operations; exhaustive search is infeasible.
- **NFR-004 Reproducibility**: Given the same input JSON, the scheduler must produce the same
  output JSON deterministically (no random seeding without explicit control).
- **NFR-005 Testability**: The implementation must include unit tests for latency calculation,
  working-set calculation, and granularity validation against the five worked examples in
  PROBLEM.md.
- **NFR-006 Language**: Dual-track implementation — Rust (Track A: compiled binary) and Python
  (Track B: Gemini agent) using typed data structures aligned with `mlsys.h`. The `mlsys.h`
  C++ header is the authoritative interface contract.

---

## Hidden Requirements (Discovered through critical evaluation)

- **HR-001 Topological sort**: The scheduler needs a topological sort of the DAG to determine
  valid subgraph orderings. This is not stated but required for correctness.
- **HR-002 Working-set calculator**: A standalone function that, given a subgraph definition
  and a set of currently-resident tensors, computes the working set size and checks OOM. This
  is needed before the optimizer can safely propose any grouping.
- **HR-003 Reachability / dominance analysis**: To decide which operations can be grouped,
  the scheduler needs to know whether all inputs to a group are either graph inputs, ephemeral
  within the group, or already resident in fast memory.
- **HR-004 Latency model validation**: The five examples in PROBLEM.md provide ground truth
  latency values. These must be used as regression tests to verify the latency model
  implementation before any optimizer runs.
- **HR-005 Baseline solution generator**: A trivially correct schedule (one op per subgraph,
  full native granularity, no retention) is needed as a correctness baseline and a performance
  lower bound for benchmarking.
- **HR-006 Solution file writer**: Serializing the `Solution` struct to the required JSON
  output format.

---

## Constraints

- **Time**: Contest deadline (exact duration unspecified — assumed to be a multi-hour to
  multi-day hackathon based on problem complexity)
- **Technology**: `mlsys.h` defines the authoritative C++ data structures; the evaluator
  (`Evaluate()`) is the ground truth. The Python implementation must produce JSON that
  `ReadSolution()` and `Evaluate()` can consume.
- **Memory hierarchy**: Three tiers only — slow memory (infinite, bandwidth-limited), fast
  memory (finite capacity, zero-latency access), and ephemeral (zero capacity, zero latency,
  intra-subgraph only).
- **Op types**: Only `"MatMul"` and `"Pointwise"` are defined in the benchmark files.
- **Granularity**: `native_granularity` is always `[128, 128]` across all benchmarks. `k`
  must divide the MatMul reduction dimension evenly (or the model must handle remainder steps).
- **Benchmark scale**: Graphs range from 2 ops / 3 tensors (example) to 95+ ops / 160+
  tensors (benchmark 17). Tensor dimensions range from 128x128 to 4096x4096.
- **Fast memory capacity**: Ranges from 20,000 (example) to 600,000 (benchmark 13) scalar
  units across the problem set.
- **Slow memory bandwidth**: Ranges from 10 to 100 across benchmarks.

---

## Assumptions

| ID | Assumption | Evidence | Risk if Wrong |
|----|------------|----------|---------------|
| A-001 | A scalar unit in the capacity / size model equals one element (no dtype factor) | Consistent with example calculations: 128x128 = 16,384 elements = 16,384 capacity units | Latency calculations would be off by a constant factor |
| A-002 | `k` must divide the MatMul reduction dimension evenly | Implied by the split-K example using k=32 into K=128 (4 equal steps) | Remainder steps need special handling in latency model |
| A-003 | Tensor sizes are given in elements (width x height), not bytes | All example calculations use element counts directly with bandwidth in elements/time | Off-by-dtype-size factor in memory time |
| A-004 | Operations can only be grouped if they form a directed path (chain) in the DAG, or more precisely a connected subgraph where all intermediate tensors can be treated as ephemeral | Subgraph grouping examples show linear chains and diamond recomputation | Incorrect grouping may violate execution semantics |
| A-005 | The evaluation is scored purely by total latency — lower is better — with no tie-breaking criteria stated | Problem statement says "minimizing total latency" | If partial scores exist per benchmark, ranking strategy differs |
| A-006 | All five benchmark files are scored equally; the contest score is the sum (or average) of latencies across benchmarks | Not stated; inferred from contest structure | May need to prioritize harder/larger benchmarks |
| A-007 | `tensors_to_retain` can include graph input tensors that were loaded during a subgraph | Note in problem: "tensors_to_retain[k] specifies which output tensors (or loaded inputs)" — parenthetical explicitly covers this | Retention of reused inputs (like Tensor0 in benchmark 1) would require separate loading |
| A-008 | Track A is implemented in Rust (compiled binary); Track B is Python (Gemini agent). Both include a local evaluator matching C++ `Evaluate()` | The contest allows C++/Rust/etc. for Track A and Python for Track B | Both tracks must independently produce valid solutions |
