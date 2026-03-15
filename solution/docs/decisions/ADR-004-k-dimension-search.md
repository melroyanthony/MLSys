# ADR-004: Full k-Dimension Search in Granularity Optimization

## Status
Accepted (partially superseded by ADR-006)

**Note:** ADR-006 (Mixed-K Fusion) superseded the `min(K_full)` cap used in the
initial granularity search. The current implementation uses `K_max = max(K_full)`
across all MatMuls in the subgraph as the upper bound for k candidates. See ADR-006
for the mixed-K execution model that makes this correct.

## Context
The initial granularity search only varied (w, h) spatially and used k=1 (or the k value inherited from the split-K stage). This produced pathologically bad schedules for MatMul-heavy subgraphs where k=1 creates K_full k-steps per spatial tile, each loading tiny input strips.

Benchmark analysis showed:
- Benchmark 1 (K_full=512): k=1 means 512 k-steps per tile, each loading h*1 + 1*w = 544 elements
- Benchmark 13 (K_full=4096): k=1 means 4096 k-steps per tile, each loading h*1 + 1*w = 136 elements
- Total memory traffic was orders of magnitude higher than necessary because each k-step reloaded input strips from slow memory

The root cause was that the search evaluated candidates but k=1 minimizes the per-step working set (smallest slices). The search was not properly accounting for the multiplicative k-step count and the repeated strip reloading that comes with smaller k values.

## Decision
Search k from max(K_full across all MatMuls in the subgraph) down to 1 in powers of 2, jointly with (w, h) spatial candidates. The full search space becomes:

```
candidates = w_values x h_values x k_values
```

Where:
- `w_values`: powers of 2 up to output width (as before)
- `h_values`: powers of 2 up to output height (as before)
- `k_values`: K_max, K_max/2, K_max/4, ..., 1 where K_max = max(K_full for each MatMul in the subgraph)

For each (w, h, k) candidate:
1. Check the working set fits in fast memory (OOM constraint)
2. Compute total subgraph latency as the sum of per-step roofline costs across all tiles and k-steps
3. Select the (w, h, k) triple that minimizes total subgraph latency

Using max(K_full) across MatMuls as the upper bound (K_max) drives the subgraph for the full extent of the largest reduction dimension. MatMuls with smaller K_full values become inactive once they finish their k-steps, contributing zero compute and memory on subsequent steps. See ADR-006 for the mixed-K execution model. Note: k candidates are powers of 2, so `ceil(K_full / k)` correctly handles cases where k does not evenly divide K_full — the last k-step simply processes the remainder.

## Consequences

### Positive
- Dramatically reduces memory traffic for MatMul subgraphs by using larger k values that load bigger input strips per step but require fewer total steps
- Allows finding the roofline-optimal operating point per subgraph, balancing compute cost per step against memory traffic per step
- Expected 10-100x latency improvement on MatMul-heavy benchmarks

### Negative
- Search space increases by a factor of ~log2(K_full) per subgraph
- For K_full=4096, this adds ~12 k candidates per (w, h) combination

### Mitigations
- Power-of-2 candidates limit search space to O(log K_full) in the k dimension
- Early termination when working set exceeds capacity prunes infeasible candidates
- Total search time remains well under 1 second for all benchmarks even with the expanded search space
