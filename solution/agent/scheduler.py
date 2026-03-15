"""
Local scheduler: baseline generation + greedy optimization pipeline.

This runs entirely locally (no API calls) and produces a valid, optimized
schedule that the agent can then try to improve further with Gemini.
"""

from __future__ import annotations

import math
from copy import deepcopy
from typing import Optional

from evaluator import (
    Granularity,
    Op,
    Problem,
    Solution,
    SubgraphDef,
    Tensor,
    check_oom,
    compute_subgraph_latency,
    compute_working_set,
    evaluate,
    get_graph_inputs,
    get_graph_outputs,
    topological_sort,
    _k_full_for_op,
    _output_tensor_for_subgraph,
)


# ---------------------------------------------------------------------------
# Baseline: one op per subgraph, native granularity, no retention
# ---------------------------------------------------------------------------


def build_baseline(problem: Problem) -> Solution:
    """
    Trivially correct schedule: one subgraph per op in topological order,
    native granularity, no retained tensors.
    """
    topo_order = topological_sort(problem)
    native_w, native_h = problem.native_granularity
    subgraphs: list[SubgraphDef] = []

    for op_idx in topo_order:
        op = problem.ops[op_idx]

        # Determine k for this subgraph
        if op.op_type == "MatMul":
            k_full = _k_full_for_op(op, problem)
        else:
            k_full = 1

        gran = Granularity(native_w, native_h, k_full)

        # Check OOM; if fails, try split-k
        if not check_oom([op_idx], gran, problem):
            gran = _find_safe_k([op_idx], Granularity(native_w, native_h, k_full), problem, set())
            if gran is None:
                # Try smaller spatial tiles too
                gran = _find_safe_granularity([op_idx], problem, set())
                if gran is None:
                    # Last resort: use smallest possible
                    gran = Granularity(native_w, native_h, 1)

        latency = compute_subgraph_latency([op_idx], gran, problem)
        sg = SubgraphDef(
            ops=[op_idx],
            granularity=gran,
            tensors_to_retain=[],
            traversal_order=None,
            subgraph_latency=latency,
        )
        subgraphs.append(sg)

    return Solution(subgraphs=subgraphs)


# ---------------------------------------------------------------------------
# Greedy chain fusion
# ---------------------------------------------------------------------------


def _can_fuse(
    ops_a: list[int],
    ops_b: list[int],
    gran: Granularity,
    problem: Problem,
    retained: set[int],
) -> bool:
    """Check whether merging two consecutive subgraphs avoids OOM."""
    merged = ops_a + ops_b
    return check_oom(merged, gran, problem, retained)


def _find_safe_k(
    ops: list[int],
    gran: Granularity,
    problem: Problem,
    retained: set[int],
) -> Optional[Granularity]:
    """
    Find the largest power-of-2 k that keeps the working set in budget.
    Returns None if even k=1 OOMs.
    """
    native_w, native_h = problem.native_granularity

    # Only applies when there's a MatMul in ops
    matmul_ops = [op_idx for op_idx in ops if problem.ops[op_idx].op_type == "MatMul"]
    if not matmul_ops:
        return gran

    k_full = _k_full_for_op(problem.ops[matmul_ops[0]], problem)

    # Try k values from k_full down to 1 (powers of 2)
    k_candidates = []
    k = k_full
    while k >= 1:
        k_candidates.append(k)
        if k == 1:
            break
        k = k // 2

    for k_try in k_candidates:
        g = Granularity(gran.w, gran.h, k_try)
        if check_oom(ops, g, problem, retained):
            return g

    return None


def _find_safe_granularity(
    ops: list[int],
    problem: Problem,
    retained: set[int],
) -> Optional[Granularity]:
    """
    Search (w, h, k) space to find a granularity that fits in fast memory
    and minimises latency.
    """
    native_w, native_h = problem.native_granularity

    # Determine output tensor for tile count
    out_tensor = _output_tensor_for_subgraph(ops, problem)
    W_out = out_tensor.width
    H_out = out_tensor.height

    matmul_ops = [op_idx for op_idx in ops if problem.ops[op_idx].op_type == "MatMul"]
    if matmul_ops:
        k_full = _k_full_for_op(problem.ops[matmul_ops[0]], problem)
    else:
        k_full = 1

    # Candidate spatial dimensions: powers of 2 from 1 up to tensor dims.
    # Include sub-native values (hardware pads — same compute cost but smaller
    # working set). Larger values preferred (fewer tiles = less overhead).
    def spatial_candidates(tensor_dim: int, native_dim: int) -> list[int]:
        cands = set()
        # Powers of 2 from 1 to tensor_dim
        s = 1
        while s <= tensor_dim:
            cands.add(s)
            s *= 2
        # Ensure native_dim is in candidates
        cands.add(native_dim)
        return sorted(cands, reverse=True)  # largest first

    w_cands = spatial_candidates(W_out, native_w)
    h_cands = spatial_candidates(H_out, native_h)

    k_cands = []
    k = k_full
    while k >= 1:
        k_cands.append(k)
        if k == 1:
            break
        k = k // 2

    best_gran = None
    best_latency = float("inf")

    for w in w_cands:
        for h in h_cands:
            for k_try in k_cands:
                g = Granularity(w, h, k_try)
                if check_oom(ops, g, problem, retained):
                    lat = compute_subgraph_latency(ops, g, problem, retained,
                                                   tensors_to_retain_after=set())
                    if lat < best_latency:
                        best_latency = lat
                        best_gran = g
                    break  # k_cands sorted largest first; first fit is best k

    return best_gran


# ---------------------------------------------------------------------------
# DRAM boundary cost helper
# ---------------------------------------------------------------------------


def _boundary_dram_cost(
    ops_a: list[int],
    ops_b: list[int],
    problem: Problem,
) -> float:
    """
    Compute the DRAM round-trip cost for tensors at the boundary between
    two adjacent subgraphs.

    Boundary tensors are those produced by ops_a and consumed by ops_b.
    Each must be fully materialized in DRAM (write from A + read into B),
    so cost = 2 * full_tensor_size / bandwidth.
    """
    produced_by_a: set[int] = set()
    for op_idx in ops_a:
        produced_by_a.update(problem.ops[op_idx].outputs)

    consumed_by_b: set[int] = set()
    for op_idx in ops_b:
        consumed_by_b.update(problem.ops[op_idx].inputs)

    boundary = produced_by_a & consumed_by_b
    bw = problem.slow_memory_bandwidth

    cost = 0.0
    for t_idx in boundary:
        t = problem.tensors[t_idx]
        cost += 2.0 * (t.width * t.height) / bw
    return cost


# ---------------------------------------------------------------------------
# Full greedy optimization pipeline
# ---------------------------------------------------------------------------


def optimize(problem: Problem) -> Solution:
    """
    Multi-stage greedy optimizer:
    1. Topological sort
    2. Baseline (1 op per subgraph)
    3. Greedy chain fusion
    4. Granularity search per subgraph
    5. Tensor retention (inter-subgraph)
    6. Traversal order (snake pattern for MatMul with h>1 tile)
    Returns best Solution found.
    """
    topo_order = topological_sort(problem)
    native_w, native_h = problem.native_granularity

    # ------------------------------------------------------------------ #
    # Step 1: Start with one-op subgraphs in topological order            #
    # ------------------------------------------------------------------ #
    sg_ops: list[list[int]] = [[op_idx] for op_idx in topo_order]

    # ------------------------------------------------------------------ #
    # Step 2: Greedy cost-based forward fusion                             #
    # ------------------------------------------------------------------ #
    # We merge sg[i] and sg[i+1] if:
    #   1. The merged subgraph fits in memory (with some valid granularity).
    #   2. lat_fused < lat_a + lat_b + dram_boundary_cost
    changed = True
    while changed:
        changed = False
        new_sg_ops = []
        i = 0
        while i < len(sg_ops):
            if i + 1 < len(sg_ops):
                merged = sg_ops[i] + sg_ops[i + 1]

                # Find a feasible granularity for the merged subgraph.
                g_merged = _find_safe_granularity(merged, problem, set())
                if g_merged is None:
                    # Try just split-k with native spatial dims.
                    matmul_in_merged = [o for o in merged
                                        if problem.ops[o].op_type == "MatMul"]
                    if matmul_in_merged:
                        k_full = _k_full_for_op(
                            problem.ops[matmul_in_merged[0]], problem
                        )
                    else:
                        k_full = 1
                    g_native = Granularity(native_w, native_h, k_full)
                    g_merged = _find_safe_k(merged, g_native, problem, set())

                if g_merged is not None:
                    # Cost comparison: fused vs. separate + DRAM boundary.
                    lat_fused = compute_subgraph_latency(
                        merged, g_merged, problem, set(), tensors_to_retain_after=set()
                    )

                    g_a = _find_safe_granularity(sg_ops[i], problem, set())
                    if g_a is None:
                        matmul_in_a = [o for o in sg_ops[i]
                                       if problem.ops[o].op_type == "MatMul"]
                        k_full_a = _k_full_for_op(problem.ops[matmul_in_a[0]], problem) \
                            if matmul_in_a else 1
                        g_a = Granularity(native_w, native_h, k_full_a)

                    g_b = _find_safe_granularity(sg_ops[i + 1], problem, set())
                    if g_b is None:
                        matmul_in_b = [o for o in sg_ops[i + 1]
                                       if problem.ops[o].op_type == "MatMul"]
                        k_full_b = _k_full_for_op(problem.ops[matmul_in_b[0]], problem) \
                            if matmul_in_b else 1
                        g_b = Granularity(native_w, native_h, k_full_b)

                    lat_a = compute_subgraph_latency(
                        sg_ops[i], g_a, problem, set(), tensors_to_retain_after=set()
                    )
                    lat_b = compute_subgraph_latency(
                        sg_ops[i + 1], g_b, problem, set(), tensors_to_retain_after=set()
                    )
                    # lat_a already includes evicting boundary outputs;
                    # lat_b already includes loading them. No separate boundary cost.
                    if lat_fused < lat_a + lat_b:
                        new_sg_ops.append(merged)
                        i += 2
                        changed = True
                        continue

            new_sg_ops.append(sg_ops[i])
            i += 1

        sg_ops = new_sg_ops

    # ------------------------------------------------------------------ #
    # Step 3: Per-subgraph granularity search                              #
    # ------------------------------------------------------------------ #
    retained: set[int] = set()
    final_subgraphs: list[SubgraphDef] = []

    for ops in sg_ops:
        best_gran = _find_safe_granularity(ops, problem, retained)
        if best_gran is None:
            # Fallback: native with k=1
            best_gran = Granularity(native_w, native_h, 1)

        # Step 4: Traversal order optimization (snake pattern for MatMul)
        traversal = _optimize_traversal(ops, best_gran, problem, retained)

        lat = compute_subgraph_latency(
            ops, best_gran, problem, retained, traversal,
            tensors_to_retain_after=set(),
        )
        sg = SubgraphDef(
            ops=ops,
            granularity=best_gran,
            tensors_to_retain=[],
            traversal_order=traversal,
            subgraph_latency=lat,
        )
        final_subgraphs.append(sg)

        # Clear retained for next subgraph (retention is applied later)
        retained = set()

    # ------------------------------------------------------------------ #
    # Step 5: Tensor retention decisions                                   #
    # ------------------------------------------------------------------ #
    final_subgraphs = _apply_retention(final_subgraphs, problem)

    return Solution(subgraphs=final_subgraphs)


def _optimize_traversal(
    ops: list[int],
    gran: Granularity,
    problem: Problem,
    retained: set[int],
) -> Optional[list[int]]:
    """
    For MatMul subgraphs with >1 spatial tile, try a snake traversal order
    and return it if it improves latency over raster order.
    """
    matmul_ops = [op_idx for op_idx in ops if problem.ops[op_idx].op_type == "MatMul"]
    if not matmul_ops:
        return None  # No benefit for pointwise-only

    out_tensor = _output_tensor_for_subgraph(ops, problem)
    num_tiles_w = math.ceil(out_tensor.width / gran.w)
    num_tiles_h = math.ceil(out_tensor.height / gran.h)
    num_tiles = num_tiles_w * num_tiles_h

    if num_tiles <= 1:
        return None

    # Build snake traversal: row 0 left-to-right, row 1 right-to-left, etc.
    snake_order = []
    for row in range(num_tiles_h):
        col_range = range(num_tiles_w) if row % 2 == 0 else range(num_tiles_w - 1, -1, -1)
        for col in col_range:
            snake_order.append(row * num_tiles_w + col)

    raster_latency = compute_subgraph_latency(ops, gran, problem, retained, None)
    snake_latency = compute_subgraph_latency(ops, gran, problem, retained, snake_order)

    if snake_latency < raster_latency:
        return snake_order
    return None


def _apply_retention(
    subgraphs: list[SubgraphDef],
    problem: Problem,
) -> list[SubgraphDef]:
    """
    For each subgraph boundary, decide which output tensors to retain
    in fast memory for the next subgraph, if they are needed and capacity allows.
    """
    result = deepcopy(subgraphs)
    n = len(result)

    for i in range(n - 1):
        sg_curr = result[i]
        sg_next = result[i + 1]

        # Tensors produced by current subgraph
        produced_curr: set[int] = set()
        for op_idx in sg_curr.ops:
            produced_curr.update(problem.ops[op_idx].outputs)

        # Tensors consumed by next subgraph (boundary inputs of next)
        consumed_next: set[int] = set()
        for op_idx in sg_next.ops:
            consumed_next.update(problem.ops[op_idx].inputs)

        # Produced inside next (ephemeral candidates)
        produced_next: set[int] = set()
        for op_idx in sg_next.ops:
            produced_next.update(problem.ops[op_idx].outputs)
        ephemeral_next = produced_next & consumed_next

        # Candidates: tensors produced by curr AND needed by next as boundary inputs
        candidates = produced_curr & (consumed_next - ephemeral_next)

        if not candidates:
            continue

        # Try retaining each candidate and check if next subgraph still fits
        to_retain: list[int] = []
        test_retained: set[int] = set()

        for t_idx in sorted(candidates):
            test_set = test_retained | {t_idx}
            if check_oom(sg_next.ops, sg_next.granularity, problem, test_set):
                to_retain.append(t_idx)
                test_retained.add(t_idx)

        if to_retain:
            sg_curr.tensors_to_retain = to_retain

            # Recalculate latency for current subgraph — retained outputs
            # are NOT evicted, so mem_out is reduced.
            # The retained_tensors set for this subgraph is whatever was
            # retained FROM the previous subgraph (we'll propagate properly below).
            prev_retained = set(result[i - 1].tensors_to_retain) if i > 0 else set()
            lat = compute_subgraph_latency(
                sg_curr.ops, sg_curr.granularity, problem, prev_retained,
                sg_curr.traversal_order,
                tensors_to_retain_after=set(to_retain),
            )
            sg_curr.subgraph_latency = lat

            # Recalculate latency for next subgraph with retained tensors as inputs
            lat_next = compute_subgraph_latency(
                sg_next.ops, sg_next.granularity, problem,
                set(to_retain), sg_next.traversal_order,
                tensors_to_retain_after=set(sg_next.tensors_to_retain),
            )
            sg_next.subgraph_latency = lat_next

    return result


# ---------------------------------------------------------------------------
# Verify all worked examples from PROBLEM.md
# ---------------------------------------------------------------------------


def _make_example1_problem() -> Problem:
    from evaluator import parse_problem
    return parse_problem({
        "widths": [128, 128, 128],
        "heights": [128, 128, 128],
        "inputs": [[0], [1]],
        "outputs": [[1], [2]],
        "base_costs": [1000, 100],
        "op_types": ["Pointwise", "Pointwise"],
        "fast_memory_capacity": 35000,
        "slow_memory_bandwidth": 10,
        "native_granularity": [128, 128],
    })


if __name__ == "__main__":
    # Quick sanity check
    import json
    import sys

    prob_data = json.load(open(sys.argv[1]))
    from evaluator import parse_problem

    problem = parse_problem(prob_data)
    sol = optimize(problem)
    total = sum(sg.subgraph_latency for sg in sol.subgraphs)
    print(f"Optimized total latency: {total:.1f}")
    print(f"Subgraphs: {len(sol.subgraphs)}")
    for i, sg in enumerate(sol.subgraphs):
        print(
            f"  [{i}] ops={sg.ops} gran=({sg.granularity.w},{sg.granularity.h},{sg.granularity.k})"
            f" latency={sg.subgraph_latency:.1f} retain={sg.tensors_to_retain}"
        )
