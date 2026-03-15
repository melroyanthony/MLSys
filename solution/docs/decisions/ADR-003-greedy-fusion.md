# ADR-003: Greedy Bottom-Up Fusion Over DP/Beam Search

## Status
Accepted

## Context
Operation grouping (fusion) is the highest-impact optimization in the scheduler. Grouping adjacent ops into a single subgraph makes intermediate tensors ephemeral (zero memory, zero transfer cost), which can dramatically reduce latency (2x improvement shown in Example 1B).

The grouping problem is combinatorial: for N ops, the number of possible partitions into subgraphs is the Bell number B(N), which grows super-exponentially. For benchmark 17 (103 ops), exhaustive search is infeasible.

### Alternatives Considered

1. **Greedy bottom-up fusion**: Walk ops in topological order, try merging each op with its predecessor subgraph if the merged working set fits. O(N^2) worst case.

2. **Dynamic programming on DAG partitions**: Optimal but requires O(2^N) states for general DAGs. Can be reduced to O(N^2) for chain DAGs with optimal substructure, but the benchmarks have branching/merging topologies (attention heads, skip connections).

3. **Beam search**: Maintain top-K candidate partitions, expand greedily. Better than pure greedy but O(K * N^2) with uncertain quality guarantees.

4. **ILP/constraint solver**: Formulate as integer program. Optimal but requires a solver dependency and may be slow for N=103.

## Decision
Use **greedy bottom-up fusion** with the following rules:

1. Start with one subgraph per op (baseline schedule)
2. Walk ops in topological order
3. For each op, consider merging it into each of its immediate predecessor's subgraphs
4. Accept the merge if:
   a. The merged subgraph's working set fits in fast memory at the current granularity
   b. All intermediate tensors between the merged ops become ephemeral (no external consumers that are not also in the subgraph)
   c. The merged subgraph latency is less than or equal to the sum of the separate latencies
5. If multiple predecessor merges are possible, choose the one with the lowest merged latency
6. Repeat until no more beneficial merges are found

### Fusion Constraint: Connected Subgraph

A merge is only valid if the resulting subgraph is a **connected directed subDAG** where:
- There is a topological ordering within the subgraph
- All intermediate tensors (produced by one op in the subgraph, consumed by another op in the same subgraph) are ephemeral
- A tensor consumed by an op outside the subgraph must be a boundary output

## Consequences

### Positive
- **Simple to implement**: implemented in Rust (Track A) and Python (Track B), well within the time budget
- **Fast to execute**: O(N^2) worst case, sub-second for N=96
- **Deterministic**: Same input always produces the same output
- **Incremental**: Each merge is independently validated -- no risk of cascading failures
- **Good enough**: For the linear and repeating block structures in the benchmarks, greedy fusion captures most of the benefit (chains fuse naturally)

### Negative
- **Suboptimal for complex DAGs**: May miss globally optimal groupings where splitting one subgraph enables better grouping elsewhere
- **Order-dependent**: Results depend on topological ordering tie-breaking
- **No recomputation awareness**: Greedy fusion does not consider recomputing an op in multiple subgraphs to avoid materializing an intermediate (this is deferred to a future optimizer stage)

### Mitigations
- **Post-fusion refinement**: After greedy fusion, the granularity search and split-K stages can further improve each subgraph independently
- **Multiple topo orderings**: If time permits, try multiple valid topological orderings and keep the best fusion result
- **Benchmark analysis**: The benchmarks show repeating block patterns (attention heads), where greedy fusion on a good topological order captures near-optimal groupings

### Neutral
- Greedy fusion is the standard approach in production ML compilers (XLA, TVM, Triton) for operation grouping

---

## Enhancement: Cost-Based Fusion with Epsilon Tolerance

Since the initial ADR was written, the merge criterion was strengthened (Issue #16).
The pure feasibility check (merge if working set fits) was replaced with a
**cost-based merge criterion**: merge subgraphs A and B only when
`latency(A+B, best_gran_fused) < latency(A, best_gran_A) + latency(B, best_gran_B)`.

An epsilon tolerance is applied so that merges with negligible latency difference
(within floating-point rounding) are accepted, avoiding churn on borderline cases.

This prevents fusions where forcing a shared granularity on the merged subgraph
degrades latency more than the DRAM savings from making intermediate tensors
ephemeral. The decision to merge is now based on measured benefit rather than
assumed benefit.
