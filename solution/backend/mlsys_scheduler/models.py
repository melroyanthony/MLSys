"""Data classes mirroring the C++ structs in mlsys.h."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Tensor:
    """A tensor in the compute graph."""

    width: int   # number of columns
    height: int  # number of rows

    @property
    def size(self) -> int:
        """Total number of elements."""
        return self.width * self.height


@dataclass
class Op:
    """A compute operation node in the DAG."""

    op_type: str          # "MatMul" or "Pointwise"
    inputs: list[int]     # tensor indices consumed (MatMul: [LHS, RHS])
    outputs: list[int]    # tensor indices produced
    base_cost: int        # compute cost at native granularity per tile


@dataclass
class Granularity:
    """Execution granularity tuple [w, h, k]."""

    w: int  # spatial width of output slice
    h: int  # spatial height of output slice
    k: int  # reduction depth (meaningful only for MatMul)


@dataclass
class Problem:
    """The full problem specification parsed from JSON."""

    tensors: list[Tensor]
    ops: list[Op]
    fast_memory_capacity: int
    slow_memory_bandwidth: int
    native_granularity: tuple[int, int]  # (native_w, native_h)


@dataclass
class SubgraphDef:
    """A single subgraph entry in the solution schedule."""

    ops: list[int]                       # op indices in this subgraph
    granularity: Granularity
    tensors_to_retain: list[int]         # tensor indices to keep in fast memory after
    traversal_order: list[int] | None    # permutation of tile indices, or None for raster
    subgraph_latency: float


@dataclass
class Solution:
    """The complete execution schedule."""

    subgraphs: list[SubgraphDef]

    @property
    def total_latency(self) -> float:
        return sum(sg.subgraph_latency for sg in self.subgraphs)


@dataclass
class DAGInfo:
    """Pre-computed DAG analysis result."""

    num_ops: int
    num_tensors: int
    adjacency: dict[int, list[int]]         # op -> successor op indices
    reverse_adj: dict[int, list[int]]       # op -> predecessor op indices
    tensor_producer: dict[int, int]         # tensor_id -> op that produces it (-1 for graph inputs)
    tensor_consumers: dict[int, list[int]]  # tensor_id -> ops that consume it
    graph_inputs: set[int]                  # tensor indices with no producer
    graph_outputs: set[int]                 # tensor indices with no consumer
    topo_order: list[int]                   # topologically sorted op indices


@dataclass
class ScheduleState:
    """Mutable schedule state passed through optimizer stages."""

    problem: Problem
    dag: DAGInfo
    subgraphs: list[SubgraphDef]
    retained_after: dict[int, set[int]] = field(default_factory=dict)
    # retained_after[subgraph_index] = set of tensor ids retained after that subgraph
