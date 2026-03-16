# Data Flow Diagrams

## Scheduler Pipeline (End-to-End)

```mermaid
sequenceDiagram
    participant U as User (CLI)
    participant P as Parser
    participant D as DAG Module
    participant B as Baseline
    participant F as Fusion
    participant R as Retention
    participant S as Split-K
    participant G as Granularity
    participant L as Latency Model
    participant M as Memory Model
    participant W as Serializer

    U->>P: problem.json path
    P->>P: Parse JSON into Problem struct
    P-->>D: Problem

    D->>D: Build adjacency lists
    D->>D: Identify graph inputs/outputs
    D->>D: Topological sort (Kahn's algorithm)
    D-->>B: DAGInfo

    B->>B: Create 1 subgraph per op (topo order)
    B->>L: Calculate baseline latency per subgraph
    L->>M: Verify working set fits
    M-->>L: OOM check pass
    L-->>B: subgraph_latencies
    B-->>F: ScheduleState (baseline)

    F->>F: Walk topo order, try merging adjacent subgraphs
    F->>M: Check merged working set
    alt Fits in memory
        M-->>F: OK
        F->>L: Compare merged vs separate latency
        L-->>F: merged is better
        F->>F: Accept merge
    else OOM
        M-->>F: OOM - skip merge
    end
    F-->>R: ScheduleState (fused)

    R->>R: For each subgraph boundary
    R->>R: Check which outputs are consumed by next subgraph
    R->>M: Check if retention fits in next subgraph's working set
    M-->>R: capacity check
    R->>L: Recalculate latency with retention (no reload cost)
    L-->>R: improved latency
    R-->>S: ScheduleState (with retention)

    S->>S: For each MatMul subgraph
    S->>M: Check if full-k OOMs
    alt Full-k OOMs
        S->>S: Binary search for largest k that fits
        S->>M: Validate candidate k
        S->>L: Calculate split-K latency
        L-->>S: step latency with accumulation
    end
    S-->>G: ScheduleState (with split-K)

    G->>G: For each subgraph, generate (w, h, k) candidates
    Note over G: k candidates: K_max down to 1 by repeated halving<br/>K_max = max(K_full across MatMuls in subgraph)
    G->>M: Check OOM for each candidate
    G->>L: Calculate total latency for each valid (w, h, k)
    L-->>G: candidate latencies (sum of per-step roofline)
    G->>G: Select (w, h, k) minimizing total subgraph latency
    G-->>W: ScheduleState (optimized)

    W->>W: Serialize Solution to JSON
    W-->>U: solution.json
```

## Per-Subgraph Latency Calculation (Detailed)

```mermaid
flowchart TD
    Start[Subgraph + Granularity w,h,k] --> ComputeTiles

    ComputeTiles[Compute num_tiles_w, num_tiles_h<br>num_spatial_tiles = ceil W/w x ceil H/h]
    ComputeTiles --> ComputeKSteps

    ComputeKSteps[Compute num_k_steps<br>MatMul: ceil K_full/k<br>Pointwise: 1]
    ComputeKSteps --> IterateSteps

    IterateSteps[For each step s in spatial_tiles x k_steps]
    IterateSteps --> CalcCompute

    CalcCompute[compute_time = sum base_cost per op<br>MatMul: scaled by k/K_full<br>Pointwise: unscaled]
    CalcCompute --> CalcMemory

    CalcMemory[memory_time = bytes_in + bytes_out / bandwidth<br>bytes_in: non-resident input slices<br>bytes_out: evicted output slices]
    CalcMemory --> Roofline

    Roofline[step_latency = max compute_time, memory_time]
    Roofline --> Accumulate

    Accumulate[subgraph_latency += step_latency]
    Accumulate --> MoreSteps{More steps?}
    MoreSteps -->|Yes| IterateSteps
    MoreSteps -->|No| Done[Return subgraph_latency]
```

## Working-Set Check Flow

```mermaid
flowchart TD
    Input[Subgraph ops + Granularity w,h,k + Retained tensors] --> ClassifyTensors

    ClassifyTensors[Classify each tensor:<br>- Boundary input: consumed, not produced internally<br>- Boundary output: produced, not consumed internally<br>- Ephemeral: produced AND consumed internally<br>- Retained from prior subgraph]

    ClassifyTensors --> CalcSlices

    CalcSlices[Calculate slice size per boundary tensor:<br>PW input/output: w x h<br>MatMul LHS: h x k<br>MatMul RHS: k x w<br>MatMul output: w x h]

    CalcSlices --> SumWS

    SumWS[working_set = sum all boundary slice sizes<br>+ full size of retained tensors from prior subgraph]

    SumWS --> Check{working_set <= capacity?}
    Check -->|Yes| Valid[Valid granularity]
    Check -->|No| OOM[OOM - reject this granularity]
```

## Optimizer Stage Composition

Each optimizer stage mutates `Vec<SubgraphDef>` in place. They execute in a fixed sequence of 9 stages.

```mermaid
flowchart LR
    S1[1. Baseline<br>1 op/subgraph<br>native granularity]
    S2[2. Greedy Fusion<br>Merge adjacent ops]
    S3[3. Retention pass 1<br>Keep tensors resident]
    S4[4. Split-K<br>Reduce k for OOM relief]
    S5[5. Granularity Search<br>Optimize w,h,k per subgraph<br>k: K_max...1 by halving]
    S6[6. Retention pass 2<br>Re-evaluate after granularity changes]
    S7[7. Emergency OOM Fix<br>Reduce granularity for any remaining OOM]
    S8[8. Final Latency<br>Recalculate all subgraph latencies]
    S9[9. Traversal Optimization<br>Snake order for MatMul data reuse]

    S1 --> S2 --> S3 --> S4 --> S5 --> S6 --> S7 --> S8 --> S9
```

Each stage only improves or maintains the schedule -- never degrades it. If a stage finds no improvement, it passes the schedule through unchanged.
