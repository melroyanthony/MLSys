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
    produced_inside, consumed_inside, _ = _classify_tensors(subgraph_ops, problem)
    boundary_outputs = produced_inside - consumed_inside

    if not boundary_outputs:
        last_op = problem.ops[subgraph_ops[-1]]
        return problem.tensors[last_op.outputs[0]]

    t_idx = next(iter(boundary_outputs))
    return problem.tensors[t_idx]


# ---------------------------------------------------------------------------
# Input categorization for split-K
# ---------------------------------------------------------------------------


def _graph_outputs(problem: Problem) -> set[int]:
    """Tensor indices that are not consumed by any op (graph outputs)."""
    consumed: set[int] = set()
    for op in problem.ops:
        consumed.update(op.inputs)
    all_tensors = set(range(len(problem.tensors)))
    return all_tensors - consumed


def _categorize_inputs(
    subgraph_ops: list[int],
    problem: Problem,
    boundary_inputs: set[int],
    is_split_k: bool,
) -> tuple[set[int], set[int], set[int], set[int]]:
    """
    Categorize boundary inputs into (matching Rust build_memory_plan):
    - full_load_lhs: MatMul LHS with ephemeral output — loaded as row-strips
      (h × K_full) once per spatial tile, reused across k-steps and columns.
    - k_strip_lhs: MatMul LHS with non-ephemeral output — loaded as k-strips
      (h × k) every k-step.
    - rhs_streamed: MatMul RHS (always k-strips, k × w per k-step).
    - pw_inputs: Pointwise inputs (w × h per spatial tile).

    The key distinction: ephemeral-output MatMul LHS goes to full_load (row-reusable),
    non-ephemeral-output MatMul LHS goes to k_strip (reloaded each k-step).
    """
    op_set = set(subgraph_ops)
    full_load_lhs: set[int] = set()
    k_strip_lhs: set[int] = set()
    rhs_streamed: set[int] = set()
    pw_inputs: set[int] = set()
    seen: set[int] = set()

    for op_idx in subgraph_ops:
        op = problem.ops[op_idx]
        if op.op_type == "MatMul":
            lhs_idx = op.inputs[0]
            rhs_idx = op.inputs[1]
            out_t = op.outputs[0]

            # Is output ephemeral? (consumed only within subgraph)
            consumers = [i for i, o in enumerate(problem.ops)
                         if out_t in o.inputs]
            output_ephemeral = (
                out_t not in _graph_outputs(problem)
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
                rhs_streamed.add(rhs_idx)
        else:  # Pointwise
            for t in op.inputs:
                if t in boundary_inputs and t not in seen:
                    seen.add(t)
                    pw_inputs.add(t)

    return full_load_lhs, k_strip_lhs, rhs_streamed, pw_inputs


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
      - LHS of MatMul (boundary input): FULL tensor size (loaded once, resident)
      - RHS of MatMul (boundary input): k × w per k-step (streamed)
      - Output accumulator: w × h (resident across k-steps)
      - Pointwise inputs: w × h per spatial tile
    - In non-split-K mode (num_k_steps == 1):
      - MatMul LHS: h × k (= h × K_full) per tile
      - MatMul RHS: k × w (= K_full × w) per tile
      - Output: w × h
    """
    w, h, k = gran.w, gran.h, gran.k

    produced_inside, consumed_inside, ephemeral = _classify_tensors(subgraph_ops, problem)
    boundary_inputs = consumed_inside - produced_inside
    boundary_outputs = produced_inside - consumed_inside

    # Determine whether this is a split-K scenario.
    # Use min(K_full) across all MatMuls, consistent with compute_subgraph_latency().
    matmul_ops = [op_idx for op_idx in subgraph_ops
                  if problem.ops[op_idx].op_type == "MatMul"]
    if matmul_ops:
        k_full = min(_k_full_for_op(problem.ops[op_idx], problem) for op_idx in matmul_ops)
        num_k_steps = math.ceil(k_full / k)
    else:
        k_full = 1
        num_k_steps = 1

    is_split_k = num_k_steps > 1

    ws = 0

    # Retained tensors from previous subgraphs: full size
    for t_idx in retained_tensors:
        ws += problem.tensors[t_idx].size

    # Categorize boundary inputs (4-tuple matching build_memory_plan)
    full_load_lhs, k_strip_lhs, rhs_streamed, pw_inputs = _categorize_inputs(
        subgraph_ops, problem, boundary_inputs, is_split_k
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

    # RHS of MatMul: streamed as k-strips (k × w elements per k-step)
    for t_idx in rhs_streamed:
        if t_idx in retained_tensors:
            continue
        ws += k * w

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
    - In split-K mode:
      - LHS tensors: loaded FULLY in first k-step, held resident
      - RHS tensors: streamed as k-strips per k-step
      - Output accumulator: held resident, evicted on last k-step of each spatial tile
    - In non-split-K mode (or pointwise):
      - LHS treated as row-strips for intra-subgraph reuse tracking
      - RHS treated as col-strips for intra-subgraph reuse tracking
    """
    if tensors_to_retain_after is None:
        tensors_to_retain_after = set()
    produced_inside, consumed_inside, ephemeral = _classify_tensors(subgraph_ops, problem)
    boundary_inputs = consumed_inside - produced_inside
    boundary_outputs = produced_inside - consumed_inside

    # Spatial tiling
    out_tensor = _output_tensor_for_subgraph(subgraph_ops, problem)
    W_out = out_tensor.width
    H_out = out_tensor.height

    w, h, k = gran.w, gran.h, gran.k

    num_tiles_w = math.ceil(W_out / w)
    num_tiles_h = math.ceil(H_out / h)
    num_spatial_tiles = num_tiles_w * num_tiles_h

    # K-steps: derive from the minimum K_full across ALL MatMuls in the subgraph.
    # Internal MatMuls (whose output is ephemeral) still need k-steps.
    # If there is no MatMul at all, k is irrelevant: 1 k-step.
    matmul_ops = [op_idx for op_idx in subgraph_ops
                  if problem.ops[op_idx].op_type == "MatMul"]
    if matmul_ops:
        min_k_full = min(
            _k_full_for_op(problem.ops[op_idx], problem) for op_idx in matmul_ops
        )
        num_k_steps = math.ceil(min_k_full / k)
    else:
        num_k_steps = 1

    is_split_k = num_k_steps > 1

    # Split compute: MatMul cost paid every k-step; Pointwise cost only on last k-step.
    matmul_compute_per_step = 0.0
    pointwise_compute = 0.0
    for op_idx in subgraph_ops:
        op = problem.ops[op_idx]
        if op.op_type == "MatMul":
            k_full_op = _k_full_for_op(op, problem)
            matmul_compute_per_step += op.base_cost * (k / k_full_op)
        else:
            pointwise_compute += op.base_cost

    # Categorize boundary inputs (matches Rust build_memory_plan)
    full_load_lhs, k_strip_lhs, rhs_streamed_inputs, pw_inputs = _categorize_inputs(
        subgraph_ops, problem, boundary_inputs, is_split_k
    )

    bw = problem.slow_memory_bandwidth

    # Pre-compute memory totals needed for both the fast path and the simulation path.

    # full_load LHS (ephemeral-output MatMul): h * K_full, row-reusable.
    # In split-K: loaded once per spatial tile (first k-step).
    # In spatial-only: loaded once per tile-row (reused across columns).
    full_load_per_row = sum(
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

    # RHS: k * w per k-step (split-K) OR k * w per new column (spatial-only).
    rhs_load_per_step = sum(
        (k * w) / bw
        for t_idx in rhs_streamed_inputs
        if t_idx not in retained_tensors
    )

    # Total k_strip load per step (k_strip LHS + RHS)
    k_strip_total_per_step = k_strip_lhs_per_step + rhs_load_per_step

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
            # full_load LHS loaded once per tile. k_strip LHS + RHS loaded every k-step.
            # First k-step: full_load + pw_load + k_strip_total
            first_k_mem = full_load_per_row + pw_load_per_tile + k_strip_total_per_step
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
            return num_spatial_tiles * per_tile_lat

        else:
            # Spatial-only mode (num_k_steps == 1): row-reuse pattern.
            # full_load LHS reused across columns. k_strip LHS + RHS loaded per tile.
            compute = matmul_compute_per_step + pointwise_compute

            first_col_mem = full_load_per_row + k_strip_total_per_step + pw_load_per_tile + out_evict_per_tile
            first_col_lat = max(compute, first_col_mem)

            other_col_mem = k_strip_total_per_step + pw_load_per_tile + out_evict_per_tile
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
    resident_rhs: dict[int, int] = {t: -1 for t in rhs_streamed_inputs}

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

            # ------- k_strip LHS (non-ephemeral-output MatMul, per k-step) -------
            for t_idx in k_strip_lhs:
                if t_idx in retained_tensors:
                    continue
                mem_in += (h * k) / bw

            # ------- RHS tensors (k_strip, per k-step in split-K, per column in spatial) -------
            if is_split_k:
                for t_idx in rhs_streamed_inputs:
                    if t_idx in retained_tensors:
                        continue
                    mem_in += (k * w) / bw
            else:
                if is_first_k:
                    for t_idx in rhs_streamed_inputs:
                        if t_idx in retained_tensors:
                            continue
                        if resident_rhs[t_idx] != tile_col:
                            mem_in += (k * w) / bw
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

            compute_this_step = matmul_compute_per_step + (pointwise_compute if is_last_k else 0.0)
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

        # Validate MatMul K_full consistency and k <= K_full
        matmul_k_fulls = [
            _k_full_for_op(problem.ops[op_idx], problem)
            for op_idx in ops_in_sg
            if problem.ops[op_idx].op_type == "MatMul"
        ]
        if matmul_k_fulls:
            if len(set(matmul_k_fulls)) > 1:
                raise ValidationError(
                    f"Subgraph {sg_idx}: MatMul ops have inconsistent K_full values: {matmul_k_fulls}"
                )
            if gran.k > matmul_k_fulls[0]:
                raise ValidationError(
                    f"Subgraph {sg_idx}: granularity k={gran.k} exceeds K_full={matmul_k_fulls[0]}"
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
