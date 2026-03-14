# Data Model Reference

This project has no database. This document specifies the input JSON schema, output JSON schema, and internal Rust data structures, with mappings to the C++ reference implementation in `mlsys.h`.

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

## C++ to Rust Mapping

### mlsys.h Structs -> Rust Structs

| C++ Struct | C++ Fields | Rust Struct | Rust Fields |
|------------|-----------|-------------|-------------|
| `Tensor` | `Width width; Height height;` | `Tensor` | `width: i64, height: i64` |
| `Op` | `OpType op_type; Inputs inputs; Outputs outputs; BaseCost base_cost;` | `Op` | `op_type: String, inputs: Vec<usize>, outputs: Vec<usize>, base_cost: i64` |
| `Granularity` | `Width width; Height height; Depth depth;` | `Granularity` | `w: i64, h: i64, k: i64` |
| `Problem` | `vector<Tensor> tensors; vector<Op> ops; FastMemoryCapacity; SlowMemoryBandwidth; Granularity native_granularity;` | `Problem` | `tensors: Vec<Tensor>, ops: Vec<Op>, fast_memory_capacity: i64, slow_memory_bandwidth: i64, native_granularity: (i64, i64)` |
| `Subgraph` | `vector<size_t> ops; vector<size_t> tensors_to_retain; Granularity granularity; optional<TraversalOrder> traversal_order; SubgraphLatency subgraph_latency;` | `SubgraphDef` | `ops: Vec<usize>, tensors_to_retain: Vec<usize>, granularity: Granularity, traversal_order: Option<Vec<i64>>, subgraph_latency: f64` |
| `Solution` | `vector<Subgraph> subgraphs;` | `Solution` | `subgraphs: Vec<SubgraphDef>` |

### C++ Type Aliases -> Rust Types

| C++ Type | Rust Type | Notes |
|----------|-----------|-------|
| `BaseCost` (`int64_t`) | `i64` | Always non-negative |
| `Depth` (`int64_t`) | `i64` | k dimension |
| `FastMemoryCapacity` (`int64_t`) | `i64` | Elements, not bytes |
| `Height` (`int64_t`) | `i64` | Rows |
| `Width` (`int64_t`) | `i64` | Columns |
| `SubgraphLatency` (`double`) | `f64` | Per-subgraph latency |
| `TotalLatency` (`double`) | `f64` | Sum of all subgraph latencies |
| `TraversalOrder` (`vector<int64_t>`) | `Vec<i64>` | Permutation of tile indices |
| `SlowMemoryBandwidth` (`int64_t`) | `i64` | Elements per time unit |

---

## Internal Data Structures

### DAG Representation

```rust
pub struct DagInfo {
    pub num_ops: usize,
    pub num_tensors: usize,
    pub successors: Vec<Vec<usize>>,          // op -> list of successor ops
    pub predecessors: Vec<Vec<usize>>,        // op -> list of predecessor ops
    pub tensor_producer: Vec<Option<usize>>,  // tensor_id -> producing op (None for graph inputs)
    pub tensor_consumers: Vec<Vec<usize>>,    // tensor_id -> ops that consume it
    pub graph_inputs: HashSet<usize>,         // tensor indices with no producer
    pub graph_outputs: HashSet<usize>,        // tensor indices with no consumer
    pub topo_order: Vec<usize>,               // topologically sorted op indices
}
```

### Schedule State (Mutable, passed through optimizer stages)

The Rust pipeline passes `&mut Vec<SubgraphDef>` directly through each stage alongside shared references to `Problem` and `DagInfo`. There is no separate `ScheduleState` wrapper struct — each optimizer function signature is:

```rust
fn optimize_*(subgraphs: &mut Vec<SubgraphDef>, problem: &Problem, dag: &DagInfo)
```

---

## Benchmark Input Profiles

| Benchmark | Tensors | Ops | Max Tensor Size | Fast Memory | Bandwidth |
|-----------|---------|-----|----------------|-------------|-----------|
| 1 | 9 | 5 | 512x512 (262,144) | 60,000 | 20 |
| 5 | 29 | 19 | 1024x1024 (1,048,576) | 30,000 | 15 |
| 9 | 49 | 32 | 4096x4096 (16,777,216) | 250,000 | 25 |
| 13 | 96 | 63 | 4096x4096 (16,777,216) | 600,000 | 50 |
| 17 | 160 | 103 | 2048x2048 (4,194,304) | 500,000 | 100 |

Note: Tensor sizes can be much larger than fast memory capacity, requiring tiling (spatial granularity < tensor dimensions).
