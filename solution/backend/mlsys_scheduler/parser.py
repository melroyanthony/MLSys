"""Parse a problem JSON file into a Problem struct."""

from __future__ import annotations

import json
from pathlib import Path

from mlsys_scheduler.models import Granularity, Op, Problem, Tensor


def parse_problem(path: str | Path) -> Problem:
    """Read a problem JSON file and return a Problem instance.

    Args:
        path: Path to the JSON file.

    Returns:
        Populated Problem struct.

    Raises:
        FileNotFoundError: If the file does not exist.
        ValueError: If the JSON is missing required fields.
    """
    data = json.loads(Path(path).read_text())
    return parse_problem_dict(data)


def parse_problem_dict(data: dict) -> Problem:
    """Parse a problem from a dictionary (already parsed JSON).

    Args:
        data: Dictionary matching the input JSON schema.

    Returns:
        Populated Problem struct.
    """
    widths: list[int] = data["widths"]
    heights: list[int] = data["heights"]
    if len(widths) != len(heights):
        raise ValueError("widths and heights must have the same length")

    tensors = [Tensor(width=w, height=h) for w, h in zip(widths, heights)]

    inputs: list[list[int]] = data["inputs"]
    outputs: list[list[int]] = data["outputs"]
    base_costs: list[int] = data["base_costs"]
    op_types: list[str] = data["op_types"]

    num_ops = len(op_types)
    if not (len(inputs) == len(outputs) == len(base_costs) == num_ops):
        raise ValueError("inputs, outputs, base_costs, op_types must have the same length")

    ops = [
        Op(
            op_type=op_types[i],
            inputs=list(inputs[i]),
            outputs=list(outputs[i]),
            base_cost=int(base_costs[i]),
        )
        for i in range(num_ops)
    ]

    native_g = data["native_granularity"]
    native_granularity = (int(native_g[0]), int(native_g[1]))

    return Problem(
        tensors=tensors,
        ops=ops,
        fast_memory_capacity=int(data["fast_memory_capacity"]),
        slow_memory_bandwidth=int(data["slow_memory_bandwidth"]),
        native_granularity=native_granularity,
    )
