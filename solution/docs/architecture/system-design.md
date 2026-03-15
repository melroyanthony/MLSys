# System Design: MLSys DAG Scheduler

## System Type

This is a **computational optimization tool**, not a web service. It is a single-process Rust CLI (Track A) + Python agent (Track B) that reads a problem JSON, computes an optimized execution schedule, and writes a solution JSON.

## Scale Estimates

- Input size: 2 ops / 3 tensors (trivial) to 96 ops / 160 tensors (benchmark 17)
- Runtime target: < 5 minutes per benchmark on a standard developer machine
- No concurrency, no network, no database
- Memory: All data fits easily in RAM (< 1 MB input, < 10 MB working state)

---

## Module Decomposition

```
src/
    main.rs                 # Entry point: CLI subcommands (solve + evaluate), file I/O
    models.rs               # Rust structs (Problem, Tensor, Op, Granularity, SubgraphDef, Solution)
    parser.rs               # JSON -> Problem (serde_json)
    serializer.rs           # Solution -> JSON (serde_json)
    dag.rs                  # DAG utilities: topological sort (Kahn's), adjacency, reachability
    latency.rs              # Latency model: compute_time, memory_time, subgraph_latency
    memory.rs               # Working-set calculator, OOM checker
    baseline.rs             # Naive scheduler: one op per subgraph, native granularity
    evaluate.rs             # Standalone solution evaluator (evaluate subcommand)
    optimizer/
        mod.rs              # Module declarations
        fusion.rs           # Greedy bottom-up chain fusion
        retention.rs        # Tensor retention decision logic
        splitk.rs           # Split-K search for MatMul subgraphs
        granularity.rs      # Granularity search (w, h, k candidates)
        traversal.rs        # Traversal order optimization (snake/zig-zag)
        pipeline.rs         # Orchestrates all 9 optimizer stages in sequence
```

### Module Dependency Graph

```mermaid
graph TD
    Main[main.rs] --> Parser[parser.rs]
    Main --> Serializer[serializer.rs]
    Main --> Pipeline[optimizer/pipeline.rs]
    Main --> Evaluate[evaluate.rs]

    Parser --> Models[models.rs]
    Serializer --> Models

    Pipeline --> Baseline[baseline.rs]
    Pipeline --> Fusion[optimizer/fusion.rs]
    Pipeline --> Retention[optimizer/retention.rs]
    Pipeline --> SplitK[optimizer/splitk.rs]
    Pipeline --> Granularity[optimizer/granularity.rs]
    Pipeline --> Traversal[optimizer/traversal.rs]

    Baseline --> DAG[dag.rs]
    Baseline --> Latency[latency.rs]
    Baseline --> Memory[memory.rs]

    Fusion --> DAG
    Fusion --> Memory
    Fusion --> Latency

    Retention --> Memory
    SplitK --> Memory
    SplitK --> Latency
    Granularity --> Memory
    Granularity --> Latency
    Traversal --> Latency

    Latency --> Models
    Memory --> Models
    DAG --> Models
```

---

## Data Model

All data structures are Rust structs with derived traits. They mirror the C++ structs in `mlsys.h`.

### Core Types

```rust
pub struct Tensor {
    pub width: i64,    // number of columns
    pub height: i64,   // number of rows
    // size = width * height (elements, not bytes)
}

pub struct Op {
    pub op_type: String,    // "MatMul" or "Pointwise"
    pub inputs: Vec<usize>, // tensor indices consumed (for MatMul: [LHS, RHS])
    pub outputs: Vec<usize>,// tensor indices produced
    pub base_cost: i64,     // compute cost at native granularity per tile
}

pub struct Granularity {
    pub w: i64,   // spatial width of output slice
    pub h: i64,   // spatial height of output slice
    pub k: i64,   // reduction depth (only meaningful for MatMul)
}

pub struct Problem {
    pub tensors: Vec<Tensor>,
    pub ops: Vec<Op>,
    pub fast_memory_capacity: i64,
    pub slow_memory_bandwidth: i64,
    pub native_granularity: (i64, i64),  // (native_w, native_h)
}

pub struct SubgraphDef {
    pub ops: Vec<usize>,              // op indices in this subgraph
    pub granularity: Granularity,
    pub tensors_to_retain: Vec<usize>,    // tensor indices to keep in fast memory after
    pub traversal_order: Option<Vec<i64>>,// permutation of tile indices, or None for raster
    pub subgraph_latency: f64,
}

pub struct Solution {
    pub subgraphs: Vec<SubgraphDef>,
}
```

---

## Algorithm Pipeline

The scheduler executes the following stages in strict sequence:

```
Input JSON
    |
    v
[1. Parse] -----> Problem struct
    |
    v
[2. DAG Analysis] --> topological order, adjacency lists,
    |                  graph inputs/outputs identification
    v
[3. Baseline Schedule] --> one op per subgraph, native granularity,
    |                       no retention, no fusion
    v
[4. Greedy Fusion] --> merge adjacent ops into subgraphs
    |                   where working set fits in fast memory
    v
[5. Tensor Retention] --> for each subgraph boundary, decide which
    |                      output tensors to retain in fast memory
    v
[6. Split-K Search] --> for MatMul subgraphs where full-k OOMs,
    |                    find largest k divisor that fits
    v
[7. Granularity Search] --> for each subgraph, search candidate
    |                        (w, h, k) triples to minimize total
    |                        subgraph latency (see below)
    v
[8. Latency Calculation] --> compute final subgraph_latencies
    |
    v
[9. Serialize] --> Solution JSON
```

Each stage takes the current schedule and refines it. Stages 4-7 are the optimization core. The baseline (stage 3) guarantees a valid output even if all optimizers are disabled.

### Stage 7: Granularity Search -- Full (w, h, k) Search

The granularity search explores a three-dimensional candidate space for each subgraph:

**Search space:**
```
w candidates: powers of 2 from 1 up to output_width
h candidates: powers of 2 from 1 up to output_height
k candidates: K_cap, K_cap/2, K_cap/4, ..., 1  (powers of 2, descending)
```

Where `K_cap = K_full` (the shared reduction dimension across all MatMuls in the subgraph). **Invariant: all MatMul ops within a single subgraph must share the same K_full.** This invariant is enforced during fusion (ops with different K_full are not merged) and validated during evaluation. It ensures the subgraph has a single, well-defined k-step loop. For Pointwise-only subgraphs, k is fixed at 1 and only (w, h) is searched.

**For each (w, h, k) candidate:**
1. Compute the working set (input slices + output slices + retained tensors)
2. If `working_set > fast_memory_capacity`, skip this candidate (OOM)
3. Compute total subgraph latency = sum of per-step roofline costs across all `num_spatial_tiles * num_k_steps` steps
4. Track the candidate with the lowest total latency

**Why k must be searched (not fixed at 1):**

With k=1, each k-step loads minimal input strips (h*1 + 1*w elements for MatMul), but requires K_full such steps per spatial tile. The total memory traffic is K_full * (h + w) per tile. With k=K_full, there is only 1 k-step per tile, loading h*K_full + K_full*w elements, but only once. The optimal k depends on the roofline balance: larger k increases per-step memory but reduces total steps and total memory traffic.

The previous implementation always selected k=1 because it minimized the per-step working set. However, the correct objective is to minimize **total** subgraph latency (summed across all steps), which accounts for the multiplicative effect of k-step count on repeated data reloading.

**Search complexity:** For a subgraph with output dimensions W x H and reduction K_full:
- w candidates: O(log W)
- h candidates: O(log H)
- k candidates: O(log K_full)
- Total: O(log W * log H * log K_full) candidates per subgraph
- Each candidate evaluation is O(1)
- Total search time remains well under 1 second for all benchmarks

---

## Latency Model Specification

The latency model implements the roofline evaluation described in PROBLEM.md and must match the C++ `Evaluate()` function exactly.

### Key Concepts

**Spatial Tiles**: For a subgraph with output tensor of dimensions `(W_out, H_out)` and granularity `(w, h, k)`:
- `num_tiles_w = ceil(W_out / w)` -- number of spatial tiles along width
- `num_tiles_h = ceil(H_out / h)` -- number of spatial tiles along height
- `num_spatial_tiles = num_tiles_w * num_tiles_h`

**K-Steps (Split-K)**: For MatMul with reduction dimension `K_full`:
- `num_k_steps = ceil(K_full / k)`
- For Pointwise: `num_k_steps = 1` (k is ignored)

**Total Iterations**: `num_spatial_tiles * num_k_steps`

However, the roofline is applied **per execution step**, and total latency is the **sum** of per-step latencies.

### Per-Step Latency Formulas

For each execution step (one spatial tile, one k-step):

#### Compute Time

**Hardware padding rule**: If `w < native_w` or `h < native_h`, the compute cost per step is unchanged (the hardware pads to native size, so you pay full cost but produce a smaller output tile). The cost is already accounted for by the increased number of spatial tiles. Specifically:

- The `base_cost` is the cost for **one execution at native granularity**
- When granularity equals native: `base_cost` is the cost per tile, and `num_spatial_tiles` tiles cover the full tensor
- When granularity is smaller: `base_cost` is still the cost per tile (hardware pads), but more tiles are needed

**Reduction scaling**: For MatMul, each k-step costs `base_cost * (k / K_full)` where `K_full` is the op's full reduction dimension. Verified against Example 5B: `k=32`, `K_full=128`, `base_cost=2000` per op, compute per step = `2000*(32/128) + 2000*(32/128) = 1000`.

For Pointwise, k is irrelevant — the op executes **once per spatial tile** (on the last k-step only). In a fused subgraph with k-steps from a MatMul, Pointwise compute is added only on the final k-step of each spatial tile, not every step. Verified against Example 1C (pure Pointwise, no k-steps): `base_cost=1000+100=1100` per tile, 4 tiles.

**Spatial padding**: if `w < native_w` or `h < native_h`, you still pay full `base_cost` per step (hardware pads), but need more spatial tiles to cover the tensor.

**Summary**:
```
For each k-step of a spatial tile:
  matmul_compute = sum(op.base_cost * (k / K_full) for MatMul ops)
  if is_last_k_step:
      compute = matmul_compute + sum(op.base_cost for Pointwise ops)
  else:
      compute = matmul_compute
```

Where `K_full_for_this_op` is the inner/reduction dimension of that specific MatMul (the width of the LHS input = height of the RHS input... actually: for MatMul with inputs [LHS, RHS], `K_full = LHS.width = RHS.height`).

Actually, let me re-examine. From the granularity definition:
- LHS input slice: width `k`, height `h`
- RHS input slice: width `w`, height `k`
- Output slice: width `w`, height `h`

So the full LHS tensor has width = `K_full` and height = `H_out`. The full RHS tensor has width = `W_out` and height = `K_full`. Therefore `K_full` = LHS.width = RHS.height.

#### Memory Time (Per Step)

For each execution step, we must account for data loaded from slow memory and data evicted to slow memory.

**Inputs loaded from slow memory**:
- For each **boundary input** tensor of the subgraph (not ephemeral):
  - Compute the slice size based on the op type and granularity
  - If the tensor is already resident in fast memory (retained from previous subgraph, or already loaded in a previous step of the same subgraph via intra-subgraph reuse) it costs 0
  - Otherwise: `slice_size / slow_memory_bandwidth`

**Slice sizes** (for one spatial tile + one k-step):
- Pointwise input: `w * h` elements
- Pointwise output: `w * h` elements
- MatMul LHS input: `h * k` elements
- MatMul RHS input: `k * w` elements
- MatMul output: `w * h` elements

**Outputs evicted to slow memory**:
- Output slices that are NOT retained and are NOT ephemeral must be evicted
- Eviction happens on the **last k-step** for that spatial tile (for split-K, eviction only on final accumulation step)
- Actually, from Example 5B, eviction of Tensor4 only happens on step 4 (the last k-step). In the non-split-K case, every spatial tile evicts.

**Memory time per step**:
```
memory_time = (bytes_loaded_from_slow + bytes_evicted_to_slow) / slow_memory_bandwidth
```

**Intra-subgraph data reuse (traversal order)**: When processing spatial tiles, input strips may be reused across adjacent tiles. In raster order for MatMul:
- Moving to the next column: LHS row strip is reused, RHS column strip is reloaded
- Moving to the next row: both are reloaded (LHS row strip changes, RHS column strip was evicted)

In snake/zig-zag order: one of the two input strips is always reused between consecutive tiles.

#### Total Subgraph Latency

```
subgraph_latency = sum over all steps of:
    max(compute_time_per_step, memory_time_per_step)
```

#### Total Graph Latency

```
total_latency = sum(sg.subgraph_latency for sg in solution.subgraphs)
```

---

## Working-Set Calculation

The working set is the total fast-memory capacity consumed during one execution iteration.

### For a Subgraph with Granularity (w, h, k)

1. Identify **boundary inputs**: tensors consumed by the subgraph that are NOT produced within the subgraph (not ephemeral)
2. Identify **boundary outputs**: tensors produced by the subgraph that are NOT consumed within the subgraph, OR that are listed in `tensors_to_retain`
3. Identify **ephemeral tensors**: produced and consumed within the same subgraph -- these consume 0 capacity
4. Add **retained tensors** from previous subgraphs that are still in fast memory

For each boundary tensor, compute its slice size:

| Tensor Role | Op Type | Slice Size |
|-------------|---------|------------|
| Pointwise input | Pointwise | `w * h` |
| Pointwise output | Pointwise | `w * h` |
| MatMul LHS input | MatMul | `h * k` |
| MatMul RHS input | MatMul | `k * w` |
| MatMul output | MatMul | `w * h` |

**Special case for output-stationary (split-K)**: The output/accumulator tensor (`w * h`) is held resident across all k-steps. Input strips are streamed.

**Working set**:
```
working_set = sum(slice_size for each boundary input and output tensor that must
                  be simultaneously resident in fast memory during one step)
            + sum(size of retained tensors from previous subgraphs)
```

**OOM check**: `working_set <= fast_memory_capacity`

### Retained Tensors from Previous Subgraphs

When a previous subgraph retains a tensor, that tensor occupies fast memory at its **full size** (not a slice), because it was computed across all spatial tiles and remains fully materialized.

Wait -- actually, retained tensors are computed slice-by-slice but the full tensor accumulates. Let me reconsider.

Actually, from Example 3C: Tensor1 (128x128 = 16384) is retained. The working set of subgraph 1 must include this full tensor. The subgraph 1 has Tensor1 as input (already resident), processes Op1 and Op2 producing Tensor3. Working set = Tensor1 (16384, resident) + Tensor2 (ephemeral, 0) + Tensor3 output (16384) = 32768 <= 50000. This works.

But wait -- if the subgraph uses a granularity smaller than the tensor, only a slice of the retained tensor is needed per step. The retained tensor is at full size in fast memory though (it was fully computed by the prior subgraph at its granularity).

Actually, the problem says retained tensors stay in fast memory at full size. The working set calculation must include:
- The **full size** of all currently retained tensors
- Plus the **slice sizes** of all boundary inputs/outputs needed for the current execution step

Correction: from Example 5B, the accumulator Tensor4 (128x128 = 16384) and Tensor0 (128x128 = 16384) are resident, plus Tensor1 strip (128x32 = 4096) and Tensor2 strip (32x128 = 4096). Working set = 16384 + 16384 + 4096 + 4096 = 40960. That matches.

But Tensor0 is a full input that gets loaded in step 1 and reused. It's NOT a retained tensor from a previous subgraph -- it's loaded in this subgraph. Tensor4 is the accumulator (output). So the working set includes:
- Full-size inputs that are resident (loaded once, reused): full tensor size
- Streamed input strips: slice size
- Output/accumulator: slice size (w * h)

This is more nuanced. The working set depends on which step we're computing and the traversal order. The **maximum** working set across all steps must fit.

For the OOM check, we need the **worst-case step** (typically the first step, where the most data is loaded fresh).

---

## Memory Hierarchy Summary

| Tier | Capacity | Access Cost | Persistence |
|------|----------|-------------|-------------|
| Slow Memory | Infinite | `size / bandwidth` per transfer | Permanent (graph I/O lives here) |
| Fast Memory | `fast_memory_capacity` elements | 0 (instant access) | Explicit: evicted unless retained |
| Ephemeral | 0 (no capacity consumed) | 0 | Intra-subgraph only |

---

## Key Formulas Reference

### Tensor Slice Sizes

For granularity `(w, h, k)`:

| Role | Width | Height | Size |
|------|-------|--------|------|
| Output (any op) | w | h | w * h |
| Pointwise input | w | h | w * h |
| MatMul LHS input | k | h | h * k |
| MatMul RHS input | w | k | k * w |

### Number of Tiles

```
num_tiles_w = ceil(output_tensor.width / w)
num_tiles_h = ceil(output_tensor.height / h)
num_spatial_tiles = num_tiles_w * num_tiles_h
```

### Number of K-Steps

```
For MatMul: num_k_steps = ceil(K_full / k)
For Pointwise-only subgraphs: num_k_steps = 1
```

### Compute Cost Per Step

```
compute_per_step = sum for each op in subgraph:
    if MatMul: base_cost * min(k, K_full_remaining) / native_k
    if Pointwise: base_cost
```

Actually, let me be more precise. From the problem: "choosing k below native simply runs fewer cycles, dividing compute proportionally without waste." So for MatMul:
```
compute_per_matmul_step = base_cost * (k / K_full)
```
where `K_full` is the full reduction dimension of that MatMul.

For the spatial dimensions, if `w < native_w` or `h < native_h`, you still pay `base_cost` (padded), but you need more tiles. The examples confirm this.

### Roofline Per Step

```
step_latency = max(compute_time, memory_time)
where:
    compute_time = sum of per-op compute costs for this step
    memory_time = (bytes_in + bytes_out) / slow_memory_bandwidth
```

### Total Latency

```
subgraph_latency = sum(step_latency for each step)
total_latency = sum(subgraph_latency for each subgraph)
```

---

## Benchmark Summary

| Benchmark | Ops | Tensors | Tensor Sizes | Fast Memory | Bandwidth | Pattern |
|-----------|-----|---------|-------------|-------------|-----------|---------|
| 1 | 5 | 9 | 512x512 | 60,000 | 20 | Linear chain (MatMul + Pointwise) |
| 5 | 19 | 29 | 128-1024 mixed | 30,000 | 15 | 3x attention heads + aggregation |
| 9 | 32 | 49 | 1024-4096 mixed | 250,000 | 25 | 8x repeating MatMul+PW blocks |
| 13 | 63 | 96 | 128-4096 mixed | 600,000 | 50 | 16x parallel MatMul heads + PW aggregation |
| 17 | 96 | 160 | 128-2048 mixed | 500,000 | 100 | 8x attention + 8x MLP blocks + residual |

---

## Performance Considerations

1. **Rust zero-cost abstractions**: The scheduler performs integer arithmetic, comparisons, and vector operations. Rust compiles to native code with no garbage collection pauses.
2. **Granularity search space**: Candidates are powers of 2 for each dimension. For spatial dimensions, up to ~6 candidates per dimension for a 4096-wide tensor. For the k dimension, O(log2 K_full) candidates (e.g., 12 for K_full=4096). The full search space per subgraph is O(log W * log H * log K) candidates, each evaluated in O(1) time. See ADR-004 for details on the k-dimension search.
3. **Fusion feasibility check**: Before merging two subgraphs, check working-set OOM at the most restrictive granularity. This is O(1) per candidate merge.
4. **Topological sort**: Kahn's algorithm, O(V + E), runs once.
5. **Total optimizer complexity**: O(N^2) for fusion (N = number of ops), O(G) for granularity search per subgraph (G = candidate granularities). Well within the contest time budget even for benchmark 17.
6. **Static binary**: `cargo build --release` with `lto = true` and `codegen-units = 1` produces a fully optimized, statically linked binary with no runtime dependencies.
