"""
Local Python evaluator that mirrors the C++ Evaluate() function from mlsys.h.

This module lets us validate and score solutions without burning Gemini API calls.
All formulas are derived from PROBLEM.md and the five worked examples.

Key insight from Example 5B (Split-K chained MatMul):
- In output-stationary (split-K) mode, the LHS of each MatMul is loaded
  at FULL size and held resident across all k-steps.
- Only the RHS is streamed as k-width strips.
- The final output accumulator (w × h) is held resident across k-steps.
- For a chained MatMul [Op0, Op1] where Op1's LHS (Tensor3) is ephemeral
  (produced by Op0), Op0's LHS (Tensor0) is the one held resident at full size.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Optional


# ---------------------------------------------------------------------------
# Data model (mirrors mlsys.h structs)
# ---------------------------------------------------------------------------


@dataclass
class Tensor:
    width: int
    height: int

    @property
    def size(self) -> int:
        return self.width * self.height


@dataclass
class Op:
    op_type: str          # "MatMul" or "Pointwise"
    inputs: list[int]     # tensor indices consumed (MatMul: [LHS, RHS])
    outputs: list[int]    # tensor indices produced
    base_cost: int        # compute cost at native granularity per tile


@dataclass
class Problem:
    tensors: list[Tensor]
    ops: list[Op]
    fast_memory_capacity: int
    slow_memory_bandwidth: int
    native_granularity: tuple[int, int]  # (native_w, native_h)


@dataclass
class Granularity:
    w: int
    h: int
    k: int


@dataclass
class SubgraphDef:
    ops: list[int]
    granularity: Granularity
    tensors_to_retain: list[int]
    traversal_order: Optional[list[int]]
    subgraph_latency: float = 0.0


@dataclass
class Solution:
    subgraphs: list[SubgraphDef]


# ---------------------------------------------------------------------------
# JSON parsing helpers
# ---------------------------------------------------------------------------


def parse_problem(data: dict) -> Problem:
    """Parse a problem JSON dict into a Problem struct."""
    widths = data["widths"]
    heights = data["heights"]
    if len(widths) != len(heights):
        raise ValueError(
            f"widths ({len(widths)}) and heights ({len(heights)}) arrays must have the same length"
        )
    tensors = [Tensor(w, h) for w, h in zip(widths, heights)]

    ops = []
    for i in range(len(data["inputs"])):
        ops.append(Op(
            op_type=data["op_types"][i],
            inputs=list(data["inputs"][i]),
            outputs=list(data["outputs"][i]),
            base_cost=int(data["base_costs"][i]),
        ))

    native_g = data["native_granularity"]
    return Problem(
        tensors=tensors,
        ops=ops,
        fast_memory_capacity=int(data["fast_memory_capacity"]),
        slow_memory_bandwidth=int(data["slow_memory_bandwidth"]),
        native_granularity=(int(native_g[0]), int(native_g[1])),
    )


def parse_solution(data: dict, num_subgraphs: int) -> Solution:
    """Parse a solution JSON dict into a Solution struct."""
    subgraphs = []
    sg_ops = data["subgraphs"]
    granularities = data["granularities"]
    retain_lists = data["tensors_to_retain"]
    trav_orders = data.get("traversal_orders", [None] * len(sg_ops))
    latencies = data.get("subgraph_latencies", [0.0] * len(sg_ops))

    for i in range(len(sg_ops)):
        g = granularities[i]
        w, h, k = int(g[0]), int(g[1]), int(g[2])
        if w <= 0 or h <= 0 or k <= 0:
            raise ValidationError(
                f"granularities[{i}] values must be positive, got [{w}, {h}, {k}]"
            )
        subgraphs.append(SubgraphDef(
            ops=list(sg_ops[i]),
            granularity=Granularity(w, h, k),
            tensors_to_retain=list(retain_lists[i]),
            traversal_order=trav_orders[i],
            subgraph_latency=float(latencies[i]),
        ))
    return Solution(subgraphs=subgraphs)


# ---------------------------------------------------------------------------
# DAG utilities
# ---------------------------------------------------------------------------


def topological_sort(problem: Problem) -> list[int]:
    """Kahn's algorithm. Returns op indices in valid execution order."""
    n = len(problem.ops)
    # Build: which op produces which tensor
    tensor_producer: dict[int, int] = {}
    for op_idx, op in enumerate(problem.ops):
        for t in op.outputs:
            tensor_producer[t] = op_idx

    # in-degree based on DAG dependencies
    in_degree = [0] * n
    adj: dict[int, list[int]] = {i: [] for i in range(n)}
    for op_idx, op in enumerate(problem.ops):
        for t in op.inputs:
            if t in tensor_producer:
                parent = tensor_producer[t]
                adj[parent].append(op_idx)
                in_degree[op_idx] += 1

    queue = [i for i in range(n) if in_degree[i] == 0]
    order: list[int] = []
    while queue:
        node = queue.pop(0)
        order.append(node)
        for child in adj[node]:
            in_degree[child] -= 1
            if in_degree[child] == 0:
                queue.append(child)

    if len(order) != n:
        raise ValueError("DAG has a cycle — invalid problem input")
    return order


def get_graph_inputs(problem: Problem) -> set[int]:
    """Tensors that are not produced by any op (true graph inputs)."""
    produced = set()
    for op in problem.ops:
        produced.update(op.outputs)
    all_tensors = set(range(len(problem.tensors)))
    return all_tensors - produced


def get_graph_outputs(problem: Problem) -> set[int]:
    """Tensors that are not consumed by any op (true graph outputs)."""
    consumed = set()
    for op in problem.ops:
        consumed.update(op.inputs)
    all_tensors = set(range(len(problem.tensors)))
    return all_tensors - consumed


# ---------------------------------------------------------------------------
# Tensor classification helpers
# ---------------------------------------------------------------------------


def _classify_tensors(
    subgraph_ops: list[int], problem: Problem
) -> tuple[set[int], set[int], set[int]]:
    """
    Returns (produced_inside, consumed_inside, ephemeral).
    ephemeral = produced AND consumed inside (zero capacity, zero transfer cost).
    """
    produced_inside: set[int] = set()
    consumed_inside: set[int] = set()
    for op_idx in subgraph_ops:
        op = problem.ops[op_idx]
        produced_inside.update(op.outputs)
        consumed_inside.update(op.inputs)
    ephemeral = produced_inside & consumed_inside
    return produced_inside, consumed_inside, ephemeral


def _boundary_outputs_for_subgraph(
    subgraph_ops: list[int], problem: Problem
) -> set[int]:
    """
    Boundary outputs: tensors produced inside the subgraph that are either
    graph outputs OR consumed by ops outside the subgraph. Includes fan-out
    (tensors consumed both inside and outside).
    """
    op_set = set(subgraph_ops)
    tensor_consumers, graph_outs = _get_problem_info(problem)
    result: set[int] = set()
    for op_idx in subgraph_ops:
        for t in problem.ops[op_idx].outputs:
            if t in graph_outs:
                result.add(t)
            elif any(c not in op_set for c in tensor_consumers.get(t, [])):
                result.add(t)
    return result


def _k_full_for_op(op: Op, problem: Problem) -> int:
    """The full reduction dimension (K) for a MatMul op."""
    lhs_idx = op.inputs[0]
    lhs = problem.tensors[lhs_idx]
    # LHS has shape (H_out x K_full), so K_full = LHS.width
    return lhs.width


def _output_tensor_for_subgraph(
    subgraph_ops: list[int], problem: Problem
) -> Tensor:
    """
    The 'canonical' output tensor of a subgraph for spatial tiling.
    This is the final boundary output tensor.
    """
    boundary_outputs = _boundary_outputs_for_subgraph(subgraph_ops, problem)

    if not boundary_outputs:
        last_op = problem.ops[subgraph_ops[-1]]
        return problem.tensors[last_op.outputs[0]]

    t_idx = next(iter(boundary_outputs))
    return problem.tensors[t_idx]


# ---------------------------------------------------------------------------
# Input categorization for split-K
# ---------------------------------------------------------------------------


def _precompute_problem_info(problem: Problem) -> tuple[dict[int, list[int]], set[int]]:
    """Precompute tensor_consumers and graph_outputs for a Problem. Cache-friendly."""
    tensor_consumers: dict[int, list[int]] = {}
    for i, op in enumerate(problem.ops):
        for t in op.inputs:
            tensor_consumers.setdefault(t, []).append(i)
    graph_outs = get_graph_outputs(problem)
    return tensor_consumers, graph_outs


# Single-entry cache: stores (problem_ref, info) for the last Problem seen.
# Safe against id-reuse because we compare identity, not id().
_cached_problem: list = [None, None]  # [problem_object, (tensor_consumers, graph_outputs)]


def _get_problem_info(problem: Problem) -> tuple[dict[int, list[int]], set[int]]:
    if _cached_problem[0] is not problem:
        _cached_problem[0] = problem
        _cached_problem[1] = _precompute_problem_info(problem)
    return _cached_problem[1]


def _categorize_inputs(
    subgraph_ops: list[int],
    problem: Problem,
    boundary_inputs: set[int],
) -> tuple[set[int], set[int], set[int], set[int], set[int]]:
    """
    Categorize boundary inputs into (matching Rust build_memory_plan):
    - full_load_lhs: MatMul LHS with ephemeral output — loaded as row-strips
      (h × K_full). In split-K: loaded once per tile (reused across k-steps within
      tile, NOT across tiles). In spatial-only: loaded once per tile-row (reused
      across columns in same row).
    - k_strip_lhs: MatMul LHS with non-ephemeral output — loaded as k-strips
      (h × k) every k-step.
    - rhs_standard: MatMul RHS with non-ephemeral output (k × w per k-step).
    - rhs_ephemeral: MatMul RHS with ephemeral output (rhs.height × k per k-step,
      where rhs.height = upstream K_full).
    - pw_inputs: Pointwise inputs (w × h per spatial tile).

    The key distinction: ephemeral-output MatMul LHS goes to full_load (row-reusable),
    non-ephemeral-output MatMul LHS goes to k_strip (reloaded each k-step).
    """
    op_set = set(subgraph_ops)
    full_load_lhs: set[int] = set()
    k_strip_lhs: set[int] = set()
    rhs_standard: set[int] = set()
    rhs_ephemeral: set[int] = set()
    pw_inputs: set[int] = set()
    seen: set[int] = set()

    tensor_consumers, graph_outs = _get_problem_info(problem)

    for op_idx in subgraph_ops:
        op = problem.ops[op_idx]
        if op.op_type == "MatMul":
            lhs_idx = op.inputs[0]
            rhs_idx = op.inputs[1]
            out_t = op.outputs[0]

            # Is output ephemeral? (has consumers, all within subgraph, not a graph output)
            consumers = tensor_consumers.get(out_t, [])
            output_ephemeral = (
                len(consumers) > 0
                and out_t not in graph_outs
                and all(c in op_set for c in consumers)
            )

            if lhs_idx in boundary_inputs and lhs_idx not in seen:
                seen.add(lhs_idx)
                if output_ephemeral:
                    full_load_lhs.add(lhs_idx)  # h * K_full, row-reusable
                else:
                    k_strip_lhs.add(lhs_idx)     # h * k, per k-step
            if rhs_idx in boundary_inputs and rhs_idx not in seen:
                seen.add(rhs_idx)
                if output_ephemeral:
                    rhs_ephemeral.add(rhs_idx)  # rhs.height * k
                else:
                    rhs_standard.add(rhs_idx)   # k * w
        else:  # Pointwise
            for t in op.inputs:
                if t in boundary_inputs and t not in seen:
                    seen.add(t)
                    pw_inputs.add(t)

    return full_load_lhs, k_strip_lhs, rhs_standard, rhs_ephemeral, pw_inputs


# ---------------------------------------------------------------------------
# Working-set calculator
# ---------------------------------------------------------------------------


def compute_working_set(
    subgraph_ops: list[int],
    gran: Granularity,
    problem: Problem,
    retained_tensors: set[int],   # full tensors already in fast memory
) -> int:
    """
    Calculate the maximum fast-memory footprint for one execution step.

    Rules (from PROBLEM.md + Example 5B):
    - Ephemeral tensors: 0 capacity
    - Retained tensors from previous subgraphs: full size
    - In split-K mode (num_k_steps > 1):
      - full_load LHS (ephemeral-output MatMul): h × K_full row-strip per tile
      - k_strip LHS (non-ephemeral-output MatMul): h × k per k-step
      - Standard RHS: k × w per k-step
      - Ephemeral-output RHS: rhs.height × k per k-step
      - Output accumulator: w × h (resident across k-steps)
      - Pointwise inputs: w × h per spatial tile
    - In non-split-K mode (num_k_steps == 1):
      - full_load LHS: h × K_full row-strip, row-reusable
      - k_strip LHS: h × k, row-reusable
      - Standard RHS: k × w per column
      - Ephemeral-output RHS: rhs.height × k per column
      - Output: w × h
    """
    w, h, k = gran.w, gran.h, gran.k

    produced_inside, consumed_inside, ephemeral = _classify_tensors(subgraph_ops, problem)
    boundary_inputs = consumed_inside - produced_inside
    boundary_outputs = _boundary_outputs_for_subgraph(subgraph_ops, problem)

    # Determine whether this is a split-K scenario.
    # Use max(K_full) across all MatMuls, consistent with compute_subgraph_latency().
    matmul_ops = [op_idx for op_idx in subgraph_ops
                  if problem.ops[op_idx].op_type == "MatMul"]
    if matmul_ops:
        k_full = max(_k_full_for_op(problem.ops[op_idx], problem) for op_idx in matmul_ops)
        num_k_steps = math.ceil(k_full / k)
    else:
        k_full = 1
        num_k_steps = 1

    is_split_k = num_k_steps > 1

    ws = 0

    # Retained tensors from previous subgraphs: full size
    for t_idx in retained_tensors:
        ws += problem.tensors[t_idx].size

    # Categorize boundary inputs (5-tuple matching build_memory_plan)
    full_load_lhs, k_strip_lhs, rhs_standard, rhs_ephemeral, pw_inputs = _categorize_inputs(
        subgraph_ops, problem, boundary_inputs
    )

    # full_load LHS (ephemeral-output MatMul): h × K_full, resident across k-steps
    for t_idx in full_load_lhs:
        if t_idx in retained_tensors:
            continue
        k_full_for_lhs = problem.tensors[t_idx].width
        ws += h * k_full_for_lhs

    # k_strip LHS (non-ephemeral-output MatMul): h × k per k-step
    for t_idx in k_strip_lhs:
        if t_idx in retained_tensors:
            continue
        ws += h * k

    # Standard RHS (non-ephemeral output): k × w per k-step
    for t_idx in rhs_standard:
        if t_idx in retained_tensors:
            continue
        ws += k * w

    # Ephemeral-output RHS: rhs.height × k per k-step
    for t_idx in rhs_ephemeral:
        if t_idx in retained_tensors:
            continue
        ws += problem.tensors[t_idx].height * k

    # Pointwise inputs: w × h per spatial tile
    for t_idx in pw_inputs:
        if t_idx in retained_tensors:
            continue
        ws += w * h

    # Boundary outputs: w × h (output slice / accumulator)
    for t_idx in boundary_outputs:
        ws += w * h

    return ws


def check_oom(
    subgraph_ops: list[int],
    gran: Granularity,
    problem: Problem,
    retained_tensors: set[int] = frozenset(),
) -> bool:
    """Return True if the subgraph fits in fast memory (no OOM)."""
    ws = compute_working_set(subgraph_ops, gran, problem, retained_tensors)
    return ws <= problem.fast_memory_capacity


# ---------------------------------------------------------------------------
# Latency model
# ---------------------------------------------------------------------------


def compute_subgraph_latency(
    subgraph_ops: list[int],
    gran: Granularity,
    problem: Problem,
    retained_tensors: set[int] = frozenset(),
    traversal_order: Optional[list[int]] = None,
    tensors_to_retain_after: Optional[set[int]] = None,
) -> float:
    """
    Compute the total latency for one subgraph, matching C++ Evaluate() semantics.

    Memory model:
    - retained_tensors: already in fast memory at full size; no load cost
    - tensors_to_retain_after: outputs that are RETAINED (not evicted) after this
      subgraph. These tensors do NOT incur mem_out cost.
    - In split-K mode (num_k_steps > 1):
      - full_load LHS (ephemeral-output MatMul): h × K_full row-strip, loaded once per tile
      - k_strip LHS (non-ephemeral-output MatMul): h × k, loaded every k-step
      - Standard RHS: k × w per k-step; Ephemeral-output RHS: rhs.height × k per k-step
      - Output accumulator: w × h, evicted on last k-step of each spatial tile
    - In spatial-only mode (num_k_steps == 1):
      - full_load + k_strip LHS: row-reusable (loaded on first column of each row)
      - RHS: loaded per column (col-reuse in snake traversal)
    """
    if tensors_to_retain_after is None:
        tensors_to_retain_after = set()
    produced_inside, consumed_inside, ephemeral = _classify_tensors(subgraph_ops, problem)
    boundary_inputs = consumed_inside - produced_inside
    boundary_outputs = _boundary_outputs_for_subgraph(subgraph_ops, problem)

    # Spatial tiling
    out_tensor = _output_tensor_for_subgraph(subgraph_ops, problem)
    W_out = out_tensor.width
    H_out = out_tensor.height

    w, h, k = gran.w, gran.h, gran.k

    num_tiles_w = math.ceil(W_out / w)
    num_tiles_h = math.ceil(H_out / h)
    num_spatial_tiles = num_tiles_w * num_tiles_h

    # K-steps: derive from the maximum K_full across ALL MatMuls in the subgraph.
    # The subgraph runs until the longest reduction finishes (mixed-K support).
    # Internal MatMuls (whose output is ephemeral) still need k-steps.
    # If there is no MatMul at all, k is irrelevant: 1 k-step.
    matmul_ops = [op_idx for op_idx in subgraph_ops
                  if problem.ops[op_idx].op_type == "MatMul"]
    if matmul_ops:
        max_k_full = max(
            _k_full_for_op(problem.ops[op_idx], problem) for op_idx in matmul_ops
        )
        num_k_steps = math.ceil(max_k_full / k)
    else:
        num_k_steps = 1

    is_split_k = num_k_steps > 1

    # Split compute: MatMul cost paid every k-step it is active; Pointwise only on last k-step.
    # For mixed-K: each MatMul is active for ceil(its_K_full / k) steps.
    # matmul_compute_per_step is the total compute when ALL MatMuls are active (step 0 onward).
    matmul_compute_per_step = 0.0
    pointwise_compute = 0.0
    for op_idx in subgraph_ops:
        op = problem.ops[op_idx]
        if op.op_type == "MatMul":
            k_full_op = _k_full_for_op(op, problem)
            cost_per_step = op.base_cost * (min(k, k_full_op) / k_full_op)
            matmul_compute_per_step += cost_per_step
        else:
            pointwise_compute += op.base_cost

    # Categorize boundary inputs (5-tuple matching Rust build_memory_plan)
    full_load_lhs, k_strip_lhs, rhs_standard, rhs_ephemeral, pw_inputs = _categorize_inputs(
        subgraph_ops, problem, boundary_inputs
    )

    bw = problem.slow_memory_bandwidth

    # Pre-compute memory totals needed for both the fast path and the simulation path.

    # full_load LHS (ephemeral-output MatMul): h * K_full, row-reusable.
    full_load_lhs_time = sum(
        (h * problem.tensors[t_idx].width) / bw
        for t_idx in full_load_lhs
        if t_idx not in retained_tensors
    )

    # k_strip LHS (non-ephemeral-output MatMul): h * k, per k-step.
    k_strip_lhs_per_step = sum(
        (h * k) / bw
        for t_idx in k_strip_lhs
        if t_idx not in retained_tensors
    )

    # Standard RHS: k * w per k-step.
    rhs_std_per_step = sum(
        (k * w) / bw
        for t_idx in rhs_standard
        if t_idx not in retained_tensors
    )

    # Ephemeral-output RHS: rhs.height * k per k-step (rhs.height = upstream K_full).
    rhs_eph_per_step = sum(
        (problem.tensors[t_idx].height * k) / bw
        for t_idx in rhs_ephemeral
        if t_idx not in retained_tensors
    )

    # Total k_strip load per step (k_strip LHS + both RHS types)
    k_strip_total_per_step = k_strip_lhs_per_step + rhs_std_per_step + rhs_eph_per_step

    # Per-MatMul k_strip contribution and per-tensor active-step-count for mixed-K.
    # Each MatMul op contributes k_strip from its boundary inputs that are not retained:
    #   - non-ephemeral LHS (in k_strip_lhs): h * k_eff / bw
    #   - non-ephemeral RHS (in rhs_standard): k_eff * w / bw
    #   - ephemeral RHS (in rhs_ephemeral): rhs.height * k_eff / bw
    #   where k_eff = min(k, K_full_op) for each MatMul
    # Deduplication mirrors _categorize_inputs (a tensor counted once for its first op).
    #
    # matmul_phase_info: list of (k_full, base_cost, k_strip_contribution_per_step)
    # k_strip_tensor_active_steps: tensor_id -> step count of its owning MatMul.
    #   Used in the simulation path to load k_strip inputs only while their op is active.
    matmul_phase_info: list[tuple[int, float, float]] = []
    k_strip_tensor_active_steps: dict[int, int] = {}
    k_strip_tensor_k_eff: dict[int, int] = {}  # tensor_id -> min(k, K_full_op)
    _seen_k_strip: set[int] = set()
    for op_idx in subgraph_ops:
        op = problem.ops[op_idx]
        if op.op_type != "MatMul":
            continue
        k_full_op = _k_full_for_op(op, problem)
        op_steps = math.ceil(k_full_op / k)
        k_eff = min(k, k_full_op)  # clamp k to this op's K_full
        lhs_idx = op.inputs[0]
        rhs_idx = op.inputs[1]
        op_k_strip = 0.0
        if lhs_idx in k_strip_lhs and lhs_idx not in _seen_k_strip:
            _seen_k_strip.add(lhs_idx)
            if lhs_idx not in retained_tensors:
                op_k_strip += (h * k_eff) / bw
                k_strip_tensor_active_steps[lhs_idx] = op_steps
                k_strip_tensor_k_eff[lhs_idx] = k_eff
        if rhs_idx in rhs_standard and rhs_idx not in _seen_k_strip:
            _seen_k_strip.add(rhs_idx)
            if rhs_idx not in retained_tensors:
                op_k_strip += (k_eff * w) / bw
                k_strip_tensor_active_steps[rhs_idx] = op_steps
                k_strip_tensor_k_eff[rhs_idx] = k_eff
        if rhs_idx in rhs_ephemeral and rhs_idx not in _seen_k_strip:
            _seen_k_strip.add(rhs_idx)
            if rhs_idx not in retained_tensors:
                op_k_strip += (problem.tensors[rhs_idx].height * k_eff) / bw
                k_strip_tensor_active_steps[rhs_idx] = op_steps
                k_strip_tensor_k_eff[rhs_idx] = k_eff
        matmul_phase_info.append((k_full_op, op.base_cost, op_k_strip))

    # Pointwise inputs: w * h per spatial tile (first k-step).
    pw_load_per_tile = sum(
        (w * h) / bw
        for t_idx in pw_inputs
        if t_idx not in retained_tensors
    )

    # Output eviction: w * h per spatial tile (last k-step), for non-retained outputs.
    out_evict_per_tile = sum(
        (w * h) / bw
        for t_idx in boundary_outputs
        if t_idx not in tensors_to_retain_after
    )

    # ------------------------------------------------------------------
    # Fast path: closed-form for raster order (traversal_order is None).
    # ------------------------------------------------------------------
    if traversal_order is None:
        if is_split_k:
            # Split-K mode: all spatial tiles are identical (no row-reuse).
            #
            # Check if all MatMuls have the same K_full (fast path) or mixed-K (phase path).
            unique_k_fulls = set(kf for kf, _, _ in matmul_phase_info)
            all_same_k_full = len(unique_k_fulls) <= 1

            if all_same_k_full:
                # Fast path: uniform K_full — original formula.
                # full_load LHS loaded once per tile. k_strip LHS + RHS loaded every k-step.
                # First k-step: full_load + pw_load + k_strip_total
                first_k_mem = full_load_lhs_time + pw_load_per_tile + k_strip_total_per_step
                first_k_lat = max(matmul_compute_per_step, first_k_mem)

                # Interior k-steps: k_strip_total only
                if num_k_steps > 2:
                    interior_k_lat = max(matmul_compute_per_step, k_strip_total_per_step)
                else:
                    interior_k_lat = 0.0

                # Last k-step: k_strip_total + eviction, compute includes PW
                last_k_mem = k_strip_total_per_step + out_evict_per_tile
                last_k_lat = max(matmul_compute_per_step + pointwise_compute, last_k_mem)

                per_tile_lat = first_k_lat + max(0, num_k_steps - 2) * interior_k_lat + last_k_lat
            else:
                # Mixed-K path: compute phase-by-phase.
                # Each MatMul is active for ceil(its_K_full / k) steps.
                # Phases are defined by sorted unique step-end boundaries.
                step_ends = sorted(set(math.ceil(kf / k) for kf, _, _ in matmul_phase_info))
                # step_ends[-1] == num_k_steps

                per_tile_lat = 0.0
                prev_end = 0

                for phase_idx, phase_end in enumerate(step_ends):
                    # Active MatMuls: those whose step count >= phase_end.
                    active_compute = sum(
                        bc * (min(k, kf) / kf)
                        for kf, bc, _ in matmul_phase_info
                        if math.ceil(kf / k) >= phase_end
                    )

                    # Active k_strip: sum per-op contributions for active MatMuls only.
                    # This is exact (no proxy ratio) because each op's contribution is
                    # precomputed from its actual tensor dimensions.
                    active_k_strip = sum(
                        ks
                        for kf, _, ks in matmul_phase_info
                        if math.ceil(kf / k) >= phase_end
                    )

                    phase_steps = phase_end - prev_end
                    is_last_phase = (phase_idx == len(step_ends) - 1)

                    # O(1) per phase: classify steps as first, interior, or last.
                    # Special steps: global step 0 (loads full_load + pw_load) and
                    # global last step (evicts output, adds PW compute).
                    has_first = (prev_end == 0)
                    has_last = is_last_phase  # last phase always contains the last step

                    # Interior steps: all steps in this phase that are neither first nor last.
                    interior_count = phase_steps - (1 if has_first else 0) - (1 if has_last else 0)
                    # interior_count can be negative when a single step is both first and last.
                    interior_count = max(0, interior_count)

                    if has_first:
                        mem = full_load_lhs_time + pw_load_per_tile + active_k_strip
                        # The first step is also the last only when num_k_steps == 1,
                        # but that case is handled by the all_same_k_full branch above.
                        per_tile_lat += max(active_compute, mem)

                    if interior_count > 0:
                        interior_lat = max(active_compute, active_k_strip)
                        per_tile_lat += interior_count * interior_lat

                    if has_last:
                        # If phase has only one step and it's also the first step,
                        # we already added the first step above; skip duplicate.
                        is_also_first = has_first and (phase_steps == 1)
                        if not is_also_first:
                            mem_last = active_k_strip + out_evict_per_tile
                            compute_last = active_compute + pointwise_compute
                            per_tile_lat += max(compute_last, mem_last)
                        else:
                            # Single-step phase that is both first and last: adjust the
                            # first-step cost to include eviction and PW compute.
                            mem_last = full_load_lhs_time + pw_load_per_tile + active_k_strip + out_evict_per_tile
                            compute_last = active_compute + pointwise_compute
                            # Undo the first-step contribution already added and replace it.
                            first_mem = full_load_lhs_time + pw_load_per_tile + active_k_strip
                            per_tile_lat -= max(active_compute, first_mem)
                            per_tile_lat += max(compute_last, mem_last)

                    prev_end = phase_end

            return num_spatial_tiles * per_tile_lat

        else:
            # Spatial-only mode (num_k_steps == 1): row-reuse pattern.
            # full_load LHS + k_strip LHS are row-reusable (first column only).
            # RHS (standard + ephemeral) loaded per column.
            compute = matmul_compute_per_step + pointwise_compute

            # Per-column RHS load (not row-reusable)
            rhs_per_col = rhs_std_per_step + rhs_eph_per_step

            first_col_mem = (full_load_lhs_time + k_strip_lhs_per_step
                             + rhs_per_col + pw_load_per_tile + out_evict_per_tile)
            first_col_lat = max(compute, first_col_mem)

            other_col_mem = rhs_per_col + pw_load_per_tile + out_evict_per_tile
            other_col_lat = max(compute, other_col_mem)

            return num_tiles_h * (first_col_lat + max(0, num_tiles_w - 1) * other_col_lat)

    # ------------------------------------------------------------------
    # Simulation path: custom traversal order (e.g. snake).
    # Tracks actual row/col residency for data-reuse accounting.
    # ------------------------------------------------------------------
    tile_sequence = traversal_order

    total_latency = 0.0

    # Track intra-subgraph residency for row/col strips.
    # full_load_lhs: row-reusable (resident across columns in same row)
    # k_strip_lhs: loaded every k-step (no row-reuse)
    resident_full_lhs: dict[int, int] = {t: -1 for t in full_load_lhs}
    resident_k_strip_lhs: dict[int, int] = {t: -1 for t in k_strip_lhs}
    all_rhs = rhs_standard | rhs_ephemeral
    resident_rhs: dict[int, int] = {t: -1 for t in all_rhs}

    for spatial_step, tile_flat_idx in enumerate(tile_sequence):
        tile_row = tile_flat_idx // num_tiles_w
        tile_col = tile_flat_idx % num_tiles_w

        for k_step in range(num_k_steps):
            is_first_k = (k_step == 0)
            is_last_k = (k_step == num_k_steps - 1)

            mem_in = 0.0
            mem_out = 0.0

            # ------- full_load LHS (ephemeral-output MatMul, row-reusable) -------
            if is_first_k:
                for t_idx in full_load_lhs:
                    if t_idx in retained_tensors:
                        continue
                    if is_split_k or resident_full_lhs[t_idx] != tile_row:
                        k_full_lhs = problem.tensors[t_idx].width
                        mem_in += (h * k_full_lhs) / bw
                        resident_full_lhs[t_idx] = tile_row

            # ------- k_strip LHS (non-ephemeral-output MatMul) -------
            # In split-K: loaded every k-step (no reuse), but only while the
            # owning MatMul is still active (k_step < ceil(K_full_op / k)).
            # In spatial-only: row-reusable (same as full_load).
            for t_idx in k_strip_lhs:
                if t_idx in retained_tensors:
                    continue
                ke = k_strip_tensor_k_eff.get(t_idx, k)
                if is_split_k:
                    if k_step < k_strip_tensor_active_steps.get(t_idx, num_k_steps):
                        mem_in += (h * ke) / bw
                elif is_first_k and resident_k_strip_lhs[t_idx] != tile_row:
                    mem_in += (h * ke) / bw
                    resident_k_strip_lhs[t_idx] = tile_row

            # ------- RHS tensors -------
            if is_split_k:
                for t_idx in all_rhs:
                    if t_idx in retained_tensors:
                        continue
                    # Only load if the owning MatMul is still active this step.
                    if k_step >= k_strip_tensor_active_steps.get(t_idx, num_k_steps):
                        continue
                    ke = k_strip_tensor_k_eff.get(t_idx, k)
                    if t_idx in rhs_ephemeral:
                        mem_in += (problem.tensors[t_idx].height * ke) / bw
                    else:
                        mem_in += (ke * w) / bw
            else:
                if is_first_k:
                    for t_idx in all_rhs:
                        if t_idx in retained_tensors:
                            continue
                        if resident_rhs[t_idx] != tile_col:
                            ke = k_strip_tensor_k_eff.get(t_idx, k)
                            if t_idx in rhs_ephemeral:
                                mem_in += (problem.tensors[t_idx].height * ke) / bw
                            else:
                                mem_in += (ke * w) / bw
                            resident_rhs[t_idx] = tile_col

            # ------- Pointwise inputs -------
            if is_first_k:
                for t_idx in pw_inputs:
                    if t_idx in retained_tensors:
                        continue
                    mem_in += (w * h) / bw

            # ------- Output eviction -------
            if is_last_k:
                for t_idx in boundary_outputs:
                    if t_idx not in tensors_to_retain_after:
                        mem_out += (w * h) / bw

            # For mixed-K: only MatMuls that haven't finished yet contribute compute.
            active_matmul_compute = sum(
                bc * (min(k, kf) / kf)
                for kf, bc, _ in matmul_phase_info
                if k_step < math.ceil(kf / k)
            )
            compute_this_step = active_matmul_compute + (pointwise_compute if is_last_k else 0.0)
            memory_time = mem_in + mem_out
            step_latency = max(compute_this_step, memory_time)
            total_latency += step_latency

    return total_latency


# ---------------------------------------------------------------------------
# Full evaluator
# ---------------------------------------------------------------------------


class OOMError(Exception):
    pass


class ValidationError(Exception):
    pass


def evaluate(problem: Problem, solution: Solution) -> float:
    """
    Validate the solution and compute total latency.
    Raises OOMError or ValidationError for invalid solutions.
    """
    all_ops = set(range(len(problem.ops)))
    covered_ops: set[int] = set()
    retained_tensors: set[int] = set()
    total_latency = 0.0

    for sg_idx, sg in enumerate(solution.subgraphs):
        ops_in_sg = sg.ops
        gran = sg.granularity

        # Validate k does not exceed the maximum K_full across all MatMuls.
        # Mixed-K subgraphs (MatMuls with different K_full values) are allowed.
        matmul_k_fulls = [
            _k_full_for_op(problem.ops[op_idx], problem)
            for op_idx in ops_in_sg
            if problem.ops[op_idx].op_type == "MatMul"
        ]
        if matmul_k_fulls:
            max_k_full = max(matmul_k_fulls)
            if gran.k > max_k_full:
                raise ValidationError(
                    f"Subgraph {sg_idx}: granularity k={gran.k} exceeds max K_full={max_k_full}"
                )

        if not check_oom(ops_in_sg, gran, problem, retained_tensors):
            ws = compute_working_set(ops_in_sg, gran, problem, retained_tensors)
            raise OOMError(
                f"Subgraph {sg_idx}: working set {ws} > "
                f"fast memory {problem.fast_memory_capacity}"
            )

        latency = compute_subgraph_latency(
            ops_in_sg, gran, problem, retained_tensors, sg.traversal_order,
            tensors_to_retain_after=set(sg.tensors_to_retain),
        )
        total_latency += latency

        retained_tensors = set(sg.tensors_to_retain)
        covered_ops.update(ops_in_sg)

    if not all_ops.issubset(covered_ops):
        missing = all_ops - covered_ops
        raise ValidationError(f"Ops not covered by any subgraph: {missing}")

    return total_latency


# ---------------------------------------------------------------------------
# Serialization helpers
# ---------------------------------------------------------------------------


def solution_to_dict(solution: Solution) -> dict:
    """Convert Solution to the output JSON format."""
    subgraphs = []
    granularities = []
    tensors_to_retain = []
    traversal_orders = []
    subgraph_latencies = []

    for sg in solution.subgraphs:
        subgraphs.append(sg.ops)
        granularities.append([sg.granularity.w, sg.granularity.h, sg.granularity.k])
        tensors_to_retain.append(sg.tensors_to_retain)
        traversal_orders.append(sg.traversal_order)
        subgraph_latencies.append(sg.subgraph_latency)

    return {
        "subgraphs": subgraphs,
        "granularities": granularities,
        "tensors_to_retain": tensors_to_retain,
        "traversal_orders": traversal_orders,
        "subgraph_latencies": subgraph_latencies,
    }
