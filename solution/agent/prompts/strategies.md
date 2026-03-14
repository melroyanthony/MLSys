# Optimization Strategy Prompt

Given the problem description and a baseline schedule, analyze the DAG and suggest improvements.

## Your Analysis Steps

1. **Identify fusion opportunities**: Look for chains of ops where the intermediate tensor is only used by the next op in sequence. Fuse them — the intermediate becomes ephemeral.

2. **Check memory budget**: For each proposed subgraph, verify that the working set fits:
   ```
   working_set = sum of all slice sizes for boundary tensors + retained tensors (full size)
   Must be <= fast_memory_capacity
   ```

3. **Consider Split-K**: If a MatMul subgraph OOMs at full k, reduce k. The accumulator (output, w×h) stays resident. For a subgraph with k-step k and full reduction K_full:
   - k-steps = ceil(K_full / k)
   - Accumulator size = w × h (constant, resident)
   - Streamed LHS strip = h × k
   - Streamed RHS strip = k × w
   - Working set = accumulator + LHS_strip + RHS_strip + any_other_resident

4. **Choose optimal granularity**:
   - At native granularity (w=native_w, h=native_h): compute is efficient (no wasted cycles)
   - Smaller tiles: more tiles, same compute cost per tile, but fewer elements loaded per tile
   - Optimal: find where compute_time ≈ memory_time (roofline equilibrium)
   - Rule: if memory-bound, try smaller tiles (fewer bytes per tile); if compute-bound, try larger tiles

5. **Tensor retention**: After computing a tensor that the NEXT subgraph immediately needs, retain it (no eviction). This saves one slow-memory round-trip. Verify the next subgraph still fits in memory with the retained tensor at full size.

6. **Traversal order for MatMul**: When using smaller-than-tensor tiles for a MatMul, use snake (zig-zag) order to maximize strip reuse between consecutive tiles.

## Output Requirements

Return ONLY a JSON object with this exact structure (no markdown fences, no explanation):

```
{
  "subgraphs": [[op_idx, ...], ...],
  "granularities": [[w, h, k], ...],
  "tensors_to_retain": [[tensor_idx, ...], ...],
  "traversal_orders": [null or [tile_idx, ...], ...],
  "subgraph_latencies": [float, ...]
}
```

Important constraints:
- Every op index must appear in exactly one subgraph
- Subgraphs must be in valid execution order (no op before its inputs are produced)
- subgraph_latencies must be numerically correct per the roofline model
- No subgraph may exceed fast_memory_capacity

## Common Patterns

### Linear Chain (A -> B -> C)
Fuse all into one subgraph. Result: only boundary inputs/outputs pay transfer cost.

### Diamond (A -> {B,C} -> D where B->D and C->D)
Option 1 (Spill): A, B+C separate, D separate (2x Tensor_A transfer)
Option 2 (Retain): A retaining output, fuse B+C in next subgraph
Option 3 (Recompute): Rerun A twice — once for B path, once for C path — if A is cheap

Choose the strategy with minimum total latency.

### Memory-constrained MatMul
If LHS + RHS + output > fast_memory_capacity at native granularity:
- Try Split-K: reduce k so LHS_strip + RHS_strip + output_accumulator <= capacity
- Or reduce spatial tile size (w, h)

### Attention Pattern (Q@K followed by pointwise scaled softmax, then @V)
Fuse the pointwise chain between the two MatMuls. Use split-K if needed.
The key intermediate (attention weights) becomes ephemeral.
