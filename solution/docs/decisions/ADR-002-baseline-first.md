# ADR-002: Implement Correct Baseline Before Optimization

## Status
Accepted

## Context
The scheduler must produce valid solutions (no OOM, all ops covered, correct latencies) before it can produce *optimized* solutions. The contest evaluator rejects invalid solutions entirely, so correctness is a hard prerequisite for any score.

The problem has complex interactions between:
- Working-set calculation (depends on op types, granularity, tensor sizes, retained tensors)
- Latency model (roofline per step, tiling, split-K accumulation, intra-subgraph data reuse)
- Constraint validation (OOM, traversal order permutations, complete op coverage)

Getting any one of these wrong invalidates the entire solution.

## Decision
Implement a **trivially correct baseline scheduler** as the first milestone, before writing any optimization code:

1. **One operation per subgraph**, ordered by topological sort
2. **Native granularity** `[128, 128, K_full]` for each subgraph (or the tensor dimension if smaller)
3. **No tensor retention** (`tensors_to_retain = []` for all)
4. **No traversal optimization** (`traversal_orders = null` for all)
5. **If native granularity OOMs**: Fall back to smaller spatial tiles (halving w or h) until the working set fits

This baseline is guaranteed to produce a valid schedule for any problem. It serves as:
- A **correctness reference** for the latency model (compare against PROBLEM.md examples)
- A **performance lower bound** (any optimizer must beat this)
- A **safety fallback** if an optimizer stage produces an invalid result

## Consequences

### Positive
- **Early validation**: Can test the full pipeline (parse -> schedule -> serialize -> evaluate) within hours
- **Regression tests**: The 5 PROBLEM.md examples provide exact latency values for baseline strategies
- **Incremental development**: Each optimizer stage is layered on top of a known-good schedule
- **Risk reduction**: Even if no optimizer finishes in time, we submit a valid solution

### Negative
- **Time cost**: ~4 hours of implementation before any optimization work begins
- **Potentially poor scores initially**: The baseline produces worst-case latencies for all benchmarks

### Neutral
- Standard software engineering practice: "make it work, make it right, make it fast"
- The baseline scheduler is small (~100 lines) and serves as the foundation for the optimizer pipeline
