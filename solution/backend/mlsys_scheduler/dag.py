"""DAG utilities: topological sort (Kahn's algorithm), adjacency lists, graph I/O identification."""

from __future__ import annotations

from collections import deque

from mlsys_scheduler.models import DAGInfo, Problem


def build_dag(problem: Problem) -> DAGInfo:
    """Analyse the problem DAG and return pre-computed graph info.

    Uses Kahn's algorithm for topological sort (O(V + E)).

    Args:
        problem: The parsed Problem struct.

    Returns:
        DAGInfo with adjacency, topo order, graph inputs/outputs.

    Raises:
        ValueError: If the DAG contains a cycle.
    """
    num_ops = len(problem.ops)
    num_tensors = len(problem.tensors)

    # tensor_producer[tensor_id] = op index that produces it, or -1 for graph inputs
    tensor_producer: dict[int, int] = {}
    # tensor_consumers[tensor_id] = list of op indices that consume it
    tensor_consumers: dict[int, list[int]] = {t: [] for t in range(num_tensors)}

    for op_idx, op in enumerate(problem.ops):
        for tensor_id in op.outputs:
            tensor_producer[tensor_id] = op_idx
        for tensor_id in op.inputs:
            tensor_consumers[tensor_id].append(op_idx)

    # Identify graph inputs (tensors with no producer) and graph outputs (tensors with no consumer)
    graph_inputs: set[int] = set()
    graph_outputs: set[int] = set()
    for tensor_id in range(num_tensors):
        if tensor_id not in tensor_producer:
            tensor_producer[tensor_id] = -1
            graph_inputs.add(tensor_id)
        if not tensor_consumers[tensor_id]:
            graph_outputs.add(tensor_id)

    # Build op-level adjacency: op A -> op B if A produces a tensor consumed by B
    adjacency: dict[int, list[int]] = {i: [] for i in range(num_ops)}
    reverse_adj: dict[int, list[int]] = {i: [] for i in range(num_ops)}

    for op_idx, op in enumerate(problem.ops):
        seen_successors: set[int] = set()
        for tensor_id in op.outputs:
            for consumer_op in tensor_consumers[tensor_id]:
                if consumer_op not in seen_successors:
                    seen_successors.add(consumer_op)
                    adjacency[op_idx].append(consumer_op)
                    reverse_adj[consumer_op].append(op_idx)

    # Kahn's algorithm for topological sort
    in_degree = [len(reverse_adj[i]) for i in range(num_ops)]
    queue: deque[int] = deque(op_idx for op_idx in range(num_ops) if in_degree[op_idx] == 0)
    topo_order: list[int] = []

    while queue:
        op_idx = queue.popleft()
        topo_order.append(op_idx)
        for successor in adjacency[op_idx]:
            in_degree[successor] -= 1
            if in_degree[successor] == 0:
                queue.append(successor)

    if len(topo_order) != num_ops:
        raise ValueError("DAG contains a cycle — topological sort failed")

    return DAGInfo(
        num_ops=num_ops,
        num_tensors=num_tensors,
        adjacency=adjacency,
        reverse_adj=reverse_adj,
        tensor_producer=tensor_producer,
        tensor_consumers=tensor_consumers,
        graph_inputs=graph_inputs,
        graph_outputs=graph_outputs,
        topo_order=topo_order,
    )


def get_subgraph_boundary_tensors(
    op_indices: list[int],
    problem: Problem,
    dag: DAGInfo,
) -> tuple[list[int], list[int], set[int]]:
    """Identify boundary inputs, boundary outputs, and ephemeral tensors for a subgraph.

    Boundary inputs: tensors consumed by the subgraph but NOT produced within it.
    Boundary outputs: tensors produced by the subgraph that are NOT solely consumed within it,
                      OR are graph outputs.
    Ephemeral tensors: produced AND consumed only within the same subgraph (zero cost).

    Args:
        op_indices: Op indices in the subgraph (in topological order).
        problem: The problem spec.
        dag: Pre-computed DAG info.

    Returns:
        (boundary_inputs, boundary_outputs, ephemeral_tensor_ids)
    """
    op_set = set(op_indices)

    produced_within: set[int] = set()
    consumed_within: set[int] = set()

    for op_idx in op_indices:
        op = problem.ops[op_idx]
        for t in op.outputs:
            produced_within.add(t)
        for t in op.inputs:
            consumed_within.add(t)

    # Boundary inputs: consumed within but not produced within
    boundary_inputs = [
        t for t in consumed_within if t not in produced_within
    ]

    # A tensor produced within is ephemeral if ALL its consumers are within the subgraph
    # and it is not a graph output
    ephemeral: set[int] = set()
    boundary_outputs_set: set[int] = set()

    for t in produced_within:
        consumers = dag.tensor_consumers[t]
        all_internal = all(c in op_set for c in consumers)
        is_graph_output = t in dag.graph_outputs

        if all_internal and not is_graph_output and consumers:
            ephemeral.add(t)
        else:
            boundary_outputs_set.add(t)

    boundary_outputs = list(boundary_outputs_set)

    return boundary_inputs, boundary_outputs, ephemeral
