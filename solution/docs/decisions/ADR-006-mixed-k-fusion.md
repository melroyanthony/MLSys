# ADR-006: Mixed-K Fusion (Relax K_full Consistency Constraint)

## Status

Accepted

## Context

The previous design enforced a **K_full consistency invariant**: all MatMul ops within a single subgraph had to share the same K_full (full reduction dimension). This invariant was enforced during fusion (ops with different K_full were rejected from merging) and validated during evaluation. The rationale was simplicity: a single K_full value gives the subgraph a single, well-defined k-step loop count (`ceil(K_full / k)`).

However, the problem statement does not require this constraint. From PROBLEM.md: "This single configuration creates a unified execution grid that every operation in the subgraph must conform to." The granularity `(w, h, k)` applies uniformly, but each MatMul individually processes its dot product in `ceil(K_full_op / k)` k-steps. Different MatMuls naturally have different K_full values based on their input tensor dimensions.

### Impact of the constraint

The K_full consistency constraint prevented fusing adjacent op chains when their MatMuls had different reduction dimensions. This forced DRAM boundaries between subgraphs that could otherwise share ephemeral intermediates, creating two unnecessary transfers per boundary (eviction from subgraph A + loading into subgraph B).

Profiling on benchmarks 1 and 9 showed that these artificial DRAM boundaries account for approximately **30% of total latency**. Benchmark 9 in particular has 8 repeating MatMul+Pointwise blocks with mixed tensor sizes (1024-4096), where adjacent blocks frequently have MatMuls with different K_full values.

## Decision

**Remove the K_full consistency invariant.** Allow MatMul ops with different K_full values to coexist in the same subgraph.

### Mixed-K Execution Model

For a subgraph with MatMuls having K_full values {K1, K2, ..., Kn} and granularity k:

1. **Total k-steps**: `num_k_steps = ceil(max(K1, ..., Kn) / k)`
2. **Per-op activity**: MatMul_i is active on step `s` if `s < ceil(Ki / k)`, inactive otherwise
3. **Inactive ops contribute zero**: No compute cost, no memory traffic (input strips not loaded)
4. **Pointwise ops**: Execute on the last k-step only (unchanged from uniform-K)
5. **OOM check**: Uses worst-case step (step 0, when all MatMuls are active)

### Example

Subgraph with MatMul_A (K=1024) and MatMul_B (K=4096), k=128:
- MatMul_A active for 8 steps, MatMul_B active for 32 steps
- Total: 32 k-steps per spatial tile
- Steps 0-7: both active (full compute + full memory)
- Steps 8-31: only MatMul_B active (reduced compute + reduced memory)

### Changes Required

1. **Fusion stage**: Remove the K_full equality check from the merge eligibility filter. The cost-based criterion (`latency_fused < latency_split`) remains the gatekeeper.
2. **Granularity search**: Use `K_max = max(K_full_op)` for the k candidate range. Each candidate is evaluated using the mixed-K latency model.
3. **Closed-form latency evaluator**: Extend to handle per-step variation in active ops. Group consecutive steps with the same active set into phases; compute each phase in O(1). Total cost per candidate: O(distinct_K_values).
4. **Working-set calculator**: No change needed -- already computes worst-case (all ops active).
5. **Evaluator/validator**: Remove the K_full consistency assertion.

## Consequences

### Positive

- **30% latency improvement** on benchmarks 1 and 9 by eliminating artificial DRAM boundaries
- **Better fusion opportunities**: The cost-based criterion still prevents harmful fusions, but now has access to a larger candidate space
- **Correct modeling**: Matches the problem statement's per-op k-step semantics

### Negative

- **More complex latency model**: The closed-form evaluator must handle phased execution with varying active op sets. This increases per-candidate evaluation from O(1) to O(distinct_K_values), though in practice distinct_K is small (2-4 values)
- **Harder to reason about**: Uniform-K subgraphs have a clean, regular execution pattern. Mixed-K introduces step-varying behavior that is harder to debug and validate
- **Working-set underestimation risk**: If the worst-case step is not step 0 (e.g., if retained tensors from a finished MatMul interact unexpectedly with later steps), the OOM check could be incorrect. Mitigation: the working set at step 0 is always the maximum because it has the most active input strips.

### Neutral

- The uniform-K case is a special case of mixed-K (all ops have the same K_full), so no regression on existing behavior
- The cost-based fusion criterion provides a safety net: even if mixed-K is allowed, a fusion only happens when it provably reduces latency

## References

- Issue #22: Relax K_full consistency to allow mixed-K fusion
- Issue #16: Cost-based fusion (prerequisite -- provides the safety net)
- ADR-005: Closed-form latency evaluation (must be extended for phased computation)
