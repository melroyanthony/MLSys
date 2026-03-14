# Data Model Reference

This project has no database. This document specifies the input JSON schema, output JSON schema, and internal Python data structures, with mappings to the C++ reference implementation in `mlsys.h`.

---

## Input JSON Schema (Problem)

```json
{
  "widths": [int, ...],              // width of tensor[i] (columns)
  "heights": [int, ...],             // height of tensor[i] (rows)
  "inputs": [[int, ...], ...],       // inputs[k] = list of tensor indices consumed by op[k]
                                     // For MatMul: [LHS_index, RHS_index] (order matters)
  "outputs": [[int, ...], ...],      // outputs[k] = list of tensor indices produced by op[k]
  "base_costs": [int, ...],          // base_costs[k] = compute cost of op[k] per native tile
  "op_types": [str, ...],            // "MatMul" or "Pointwise"
  "fast_memory_capacity": int,       // max elements in fast memory
  "slow_memory_bandwidth": int,      // elements per unit time for slow memory transfers
  "native_granularity": [int, int]   // [native_w, native_h] hardware execution granularity
}
```

### Constraints
- `len(widths) == len(heights)` (number of tensors)
- `len(inputs) == len(outputs) == len(base_costs) == len(op_types)` (number of ops)
- All tensor indices in `inputs` and `outputs` are in `[0, num_tensors)`
- `op_types[k]` is either `"MatMul"` or `"Pointwise"`
- For MatMul: `len(inputs[k]) == 2`, for Pointwise: `len(inputs[k]) >= 1`
- `len(outputs[k]) == 1` for all ops observed in benchmarks

### Derived Properties
- **Graph input tensors**: tensor indices that do NOT appear in any `outputs[k]`
- **Graph output tensors**: tensor indices that do NOT appear in any `inputs[k]`
- **Reduction dimension (K_full)** for MatMul op k: `tensors[inputs[k][0]].width` (LHS width) = `tensors[inputs[k][1]].height` (RHS height)

---

## Output JSON Schema (Solution)

```json
{
  "subgraphs": [[int, ...], ...],           // subgraphs[i] = list of op indices in subgraph i
  "granularities": [[int, int, int], ...],   // [w, h, k] per subgraph
  "tensors_to_retain": [[int, ...], ...],    // tensor indices to keep in fast memory after each subgraph
  "traversal_orders": [null | [int, ...], ...], // permutation of tile indices, or null for raster
  "subgraph_latencies": [float, ...]         // calculated latency per subgraph
}
```

### Constraints
- All five lists have the same length (number of subgraphs)
- Every op index in `[0, num_ops)` appears in at least one subgraph
- `granularities[i]` is `[w, h, k]` where all are positive integers
- `tensors_to_retain[i]` contains only tensor indices produced by or loaded into subgraph i
- `traversal_orders[i]` is either `null` or a valid permutation of `[0, num_tiles)`
- `subgraph_latencies[i]` matches the evaluator's computed latency

---

## C++ to Python Mapping

### mlsys.h Structs -> Python Dataclasses

| C++ Struct | C++ Fields | Python Class | Python Fields |
|------------|-----------|--------------|---------------|
| `Tensor` | `Width width; Height height;` | `Tensor` | `width: int, height: int` |
| `Op` | `OpType op_type; Inputs inputs; Outputs outputs; BaseCost base_cost;` | `Op` | `op_type: str, inputs: list[int], outputs: list[int], base_cost: int` |
| `Granularity` | `Width width; Height height; Depth depth;` | `Granularity` | `w: int, h: int, k: int` |
| `Problem` | `vector<Tensor> tensors; vector<Op> ops; FastMemoryCapacity; SlowMemoryBandwidth; Granularity native_granularity;` | `Problem` | `tensors: list[Tensor], ops: list[Op], fast_memory_capacity: int, slow_memory_bandwidth: int, native_granularity: tuple[int, int]` |
| `Subgraph` | `vector<size_t> ops; vector<size_t> tensors_to_retain; Granularity granularity; optional<TraversalOrder> traversal_order; SubgraphLatency subgraph_latency;` | `SubgraphDef` | `ops: list[int], tensors_to_retain: list[int], granularity: Granularity, traversal_order: list[int] | None, subgraph_latency: float` |
| `Solution` | `vector<Subgraph> subgraphs;` | `Solution` | `subgraphs: list[SubgraphDef]` |

### C++ Type Aliases -> Python Types

| C++ Type | Python Type | Notes |
|----------|-------------|-------|
| `BaseCost` (`int64_t`) | `int` | Always non-negative |
| `Depth` (`int64_t`) | `int` | k dimension |
| `FastMemoryCapacity` (`int64_t`) | `int` | Elements, not bytes |
| `Height` (`int64_t`) | `int` | Rows |
| `Width` (`int64_t`) | `int` | Columns |
| `SubgraphLatency` (`double`) | `float` | Per-subgraph latency |
| `TotalLatency` (`double`) | `float` | Sum of all subgraph latencies |
| `TraversalOrder` (`vector<int64_t>`) | `list[int]` | Permutation of tile indices |
| `SlowMemoryBandwidth` (`int64_t`) | `int` | Elements per time unit |

---

## Internal Data Structures

### DAG Representation

```python
@dataclass
class DAGInfo:
    num_ops: int
    num_tensors: int
    adjacency: dict[int, list[int]]       # op -> successor ops
    reverse_adj: dict[int, list[int]]     # op -> predecessor ops
    tensor_producer: dict[int, int]       # tensor_id -> op that produces it (or -1 for graph inputs)
    tensor_consumers: dict[int, list[int]] # tensor_id -> ops that consume it
    graph_inputs: set[int]                # tensor indices with no producer
    graph_outputs: set[int]               # tensor indices with no consumer
    topo_order: list[int]                 # topologically sorted op indices
```

### Schedule State (Mutable, passed through optimizer stages)

```python
@dataclass
class ScheduleState:
    problem: Problem
    dag: DAGInfo
    subgraphs: list[SubgraphDef]
    retained_tensors: dict[int, set[int]]  # subgraph_index -> set of tensor ids retained after it
```

---

## Benchmark Input Profiles

| Benchmark | Tensors | Ops | Max Tensor Size | Fast Memory | Bandwidth |
|-----------|---------|-----|----------------|-------------|-----------|
| 1 | 9 | 5 | 512x512 (262,144) | 60,000 | 20 |
| 5 | 29 | 19 | 1024x1024 (1,048,576) | 30,000 | 15 |
| 9 | 49 | 32 | 4096x4096 (16,777,216) | 250,000 | 25 |
| 13 | 96 | 63 | 4096x4096 (16,777,216) | 600,000 | 50 |
| 17 | 160 | 96 | 2048x2048 (4,194,304) | 500,000 | 100 |

Note: Tensor sizes can be much larger than fast memory capacity, requiring tiling (spatial granularity < tensor dimensions).
