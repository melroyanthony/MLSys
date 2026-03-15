# ADR-005: Closed-Form Latency Computation for Granularity Search

## Status
Accepted

## Context
After PR #20 (correct k-step model via ADR-004), the granularity search became a performance bottleneck on larger benchmarks. The root cause is the interaction of three factors:

1. **Cubic search space**: The search iterates O(log W * log H * log K_full) candidate (w, h, k) triples per subgraph.
2. **Tile-by-tile simulation**: Each candidate evaluation calls `subgraph_latency()`, which iterates over ALL `num_spatial_tiles * num_k_steps` steps to sum per-step roofline costs.
3. **K_full consistency invariant**: Enforcing that all MatMuls in a subgraph share the same K_full causes more subgraph splits, increasing the number of subgraphs that each require a full granularity search.

For benchmark 17 (96 ops, output dimensions up to 2048, K_full up to 2048), a single subgraph evaluation can iterate thousands of tiles times dozens of k-steps, and this is repeated for hundreds of (w, h, k) candidates. The total wall-clock time exceeded acceptable limits.

### Analysis of the Simulation

In raster-order traversal (the default before traversal optimization), the per-step memory traffic follows a predictable pattern within each row of spatial tiles:

- **First tile of each row** (column index 0): Must load all input strips fresh from slow memory. For a fused MatMul+Pointwise subgraph, this includes the LHS strip (`h * k`), the RHS strip (`k * w`), any Pointwise input strips (`w * h`), and the output/accumulator (`w * h`).
- **Subsequent tiles in the same row** (column index > 0): The LHS strip is reused (same row, same height slice). Only the RHS strip and Pointwise inputs must be reloaded. The output slice changes but is the same size.
- **All tiles**: Each tile has `num_k_steps` k-steps. On the first k-step the full load is incurred; subsequent k-steps only reload the LHS and RHS strips (the accumulator stays resident). On the last k-step, the output is evicted if not retained.

Since all tiles of the same type (first-in-row vs. subsequent-in-row) have identical memory and compute costs, the total latency can be computed as a closed-form expression without iterating individual tiles.

## Decision
Replace the tile-by-tile simulation in the granularity search's candidate evaluation with a closed-form latency calculation.

### Formula

For a subgraph with `num_rows = ceil(H_out / h)` rows and `num_cols = ceil(W_out / w)` columns:

```
total_latency = num_rows * (
    first_tile_latency(w, h, k)
    + (num_cols - 1) * subsequent_tile_latency(w, h, k)
)
```

Where each tile latency is itself a sum over k-steps. In split-K, the first k-step differs from interior k-steps (it includes full-load + Pointwise input loads), so the per-tile formula distinguishes three k-step types:

```
tile_latency = first_k_step_latency + max(0, num_k_steps - 2) * interior_k_step_latency + last_k_step_latency
```

- **first_k_step**: loads full_load (LHS) + pw_load + k_strip; compute = matmul only
- **interior_k_steps**: loads k_strip only; compute = matmul only
- **last_k_step**: loads k_strip + evicts output; compute = matmul + pointwise

Each `k_step_latency = max(compute_time, memory_time)` per the roofline model, using the same formulas as the simulation but computed once per tile type rather than per tile instance.

### Additional Optimization: Early Termination

Before computing the full closed-form latency for a candidate (w, h, k), check whether the working set fits in fast memory. If not, skip immediately. This prunes the majority of candidates for memory-constrained subgraphs and avoids even the O(1) closed-form computation for infeasible candidates.

### Retained Behavior

The closed-form computation must produce **exactly the same numerical result** as the tile-by-tile simulation for raster-order traversal. It is not an approximation -- it exploits the regularity of the raster pattern to avoid redundant computation. For non-raster traversal orders (snake/zig-zag), the tile-by-tile simulation is retained because the reuse pattern depends on the specific tile sequence.

## Consequences

### Positive
- Candidate evaluation becomes O(1) regardless of tile count, reducing the granularity search from O(candidates * tiles * k_steps) to O(candidates)
- All 5 benchmarks complete granularity search in well under 1 second each
- No loss of accuracy -- closed-form produces identical results to the simulation for regular traversal orders
- Simpler reasoning about performance: search time depends only on the number of candidates, not on tensor dimensions

### Negative
- Two code paths (closed-form for raster order, simulation for snake/custom traversal) could diverge if the latency model changes
- Closed-form derivation requires careful accounting of edge cases (single-column subgraphs, single k-step, Pointwise-only subgraphs)

### Mitigations
- Correctness validated via unit tests covering all 5 PROBLEM.md examples (both strategies) and fused MatMul+Pointwise split-K scenarios. The closed-form and simulation paths are cross-checked by running both tracks (Rust closed-form and Python) and comparing outputs
- Pointwise-only subgraphs (k=1, no MatMul reuse patterns) use a simplified formula: `num_tiles * max(compute, memory)` since all tiles are identical
- The closed-form replaces the simulation for all raster-order evaluation (both search and evaluate subcommand). Snake/custom traversal orders retain the tile-by-tile simulation since their memory pattern is non-uniform
