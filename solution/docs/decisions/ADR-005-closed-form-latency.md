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

Where each tile latency is itself a sum over k-steps, but since all k-steps within the same tile type have the same cost (except the last k-step which includes Pointwise compute and output eviction), this further simplifies to:

```
tile_latency = (num_k_steps - 1) * interior_k_step_latency + last_k_step_latency
```

Each `k_step_latency = max(compute_time, memory_time)` per the roofline model, using the same formulas as the simulation but computed once per tile type rather than per tile instance.

### Additional Optimization: Early Termination

Before computing the full closed-form latency for a candidate (w, h, k), check whether the working set fits in fast memory. If not, skip immediately. This prunes the majority of candidates for memory-constrained subgraphs and avoids even the O(1) closed-form computation for infeasible candidates.

### Retained Behavior

The closed-form computation must produce **exactly the same numerical result** as the tile-by-tile simulation for raster-order traversal. It is not an approximation -- it exploits the regularity of the raster pattern to avoid redundant computation. For non-raster traversal orders (snake/zig-zag), the simulation may still be needed if the reuse pattern is irregular, but in practice snake order has its own closed-form (every tile reuses one strip).

## Consequences

### Positive
- Candidate evaluation becomes O(1) regardless of tile count, reducing the granularity search from O(candidates * tiles * k_steps) to O(candidates)
- All 5 benchmarks complete granularity search in well under 1 second each
- No loss of accuracy -- closed-form produces identical results to the simulation for regular traversal orders
- Simpler reasoning about performance: search time depends only on the number of candidates, not on tensor dimensions

### Negative
- Two code paths (closed-form for search, simulation for final evaluation) could diverge if the latency model changes
- Closed-form derivation requires careful accounting of edge cases (single-column subgraphs, single k-step, Pointwise-only subgraphs)

### Mitigations
- Add a debug assertion that compares closed-form result against simulation result for each candidate during testing
- Pointwise-only subgraphs (k=1, no MatMul reuse patterns) use a simplified formula: `num_tiles * max(compute, memory)` since all tiles are identical
- Keep the simulation as the reference implementation for the `evaluate` subcommand; only the search inner loop uses the closed-form
