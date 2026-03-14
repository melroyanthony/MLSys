"""Serialize a Solution to the output JSON format."""

from __future__ import annotations

import json
from pathlib import Path

from mlsys_scheduler.models import Solution


def solution_to_dict(solution: Solution) -> dict:
    """Convert a Solution to a JSON-serializable dictionary.

    Args:
        solution: The Solution to serialize.

    Returns:
        Dictionary matching the output JSON schema.
    """
    subgraphs_out: list[list[int]] = []
    granularities_out: list[list[int]] = []
    tensors_to_retain_out: list[list[int]] = []
    traversal_orders_out: list[list[int] | None] = []
    subgraph_latencies_out: list[float] = []

    for sg in solution.subgraphs:
        subgraphs_out.append(list(sg.ops))
        granularities_out.append([sg.granularity.w, sg.granularity.h, sg.granularity.k])
        tensors_to_retain_out.append(list(sg.tensors_to_retain))
        traversal_orders_out.append(sg.traversal_order)
        subgraph_latencies_out.append(sg.subgraph_latency)

    return {
        "subgraphs": subgraphs_out,
        "granularities": granularities_out,
        "tensors_to_retain": tensors_to_retain_out,
        "traversal_orders": traversal_orders_out,
        "subgraph_latencies": subgraph_latencies_out,
    }


def write_solution(solution: Solution, path: str | Path) -> None:
    """Write a Solution to a JSON file.

    Args:
        solution: The Solution to write.
        path: Destination file path.
    """
    data = solution_to_dict(solution)
    Path(path).write_text(json.dumps(data, indent=2))
