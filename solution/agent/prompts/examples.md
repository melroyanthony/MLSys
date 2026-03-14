# Few-Shot Examples from PROBLEM.md

These are verified worked examples. Use them to calibrate your latency calculations.

---

## Example 1B: Chain Fusion (128x128 Pointwise chain)

Problem: Two pointwise ops, tensors 128x128, fast_memory=35000, bandwidth=10, native=[128,128]

**Key insight**: Fusing both ops into one subgraph eliminates the intermediate tensor transfer.

Result latency: **3276.8** (vs 6553.6 unfused)

```json
{
  "subgraphs": [[0,1]],
  "granularities": [[128,128,1]],
  "tensors_to_retain": [[]],
  "traversal_orders": [null],
  "subgraph_latencies": [3276.8]
}
```

Calculation:
- 1 tile (128x128 = native size), 1 k-step
- memory_in = 128×128/10 = 1638.4 (load Tensor0)
- memory_out = 128×128/10 = 1638.4 (evict Tensor2)
- compute = 1000 + 100 = 1100
- step_latency = max(1100, 1638.4 + 1638.4) = 3276.8

---

## Example 2B: Fusion with 256x256 tensors (4 tiles at native 128x128)

Problem: Two pointwise ops, tensors 256x256, fast_memory=35000, bandwidth=10, native=[128,128]

Result latency: **13107.2** (vs 26214.4 unfused)

```json
{
  "subgraphs": [[0,1]],
  "granularities": [[128,128,1]],
  "tensors_to_retain": [[]],
  "traversal_orders": [null],
  "subgraph_latencies": [13107.2]
}
```

Calculation (4 tiles):
- Per tile: memory_in=1638.4 (load ¼ Tensor0), memory_out=1638.4 (evict ¼ Tensor2), compute=1100
- Per tile latency = max(1100, 3276.8) = 3276.8
- Total = 4 × 3276.8 = 13107.2

---

## Example 3C: Selective Residency (Diamond graph)

Problem: 3 pointwise ops, 128x128, fast_memory=50000, bandwidth=10. Tensor1 feeds both Op1 and Op2.

**Strategy**: Compute Tensor1, RETAIN it, then fuse Op1+Op2 (Tensor2 ephemeral).

Result latency: **4638.4** (best of three strategies)

```json
{
  "subgraphs": [[0], [1,2]],
  "granularities": [[128,128,1],[128,128,1]],
  "tensors_to_retain": [[1],[]],
  "traversal_orders": [null,null],
  "subgraph_latencies": [1638.4, 3000]
}
```

Subgraph 0 calculation:
- memory_in = 1638.4, memory_out = 0 (retained, not evicted)
- compute = 1500
- latency = max(1500, 1638.4) = 1638.4

Subgraph 1 calculation:
- Tensor1 already resident (retained) — no load cost
- memory_out = 1638.4 (evict Tensor3)
- compute = 1500 + 1500 = 3000
- latency = max(3000, 1638.4) = 3000

---

## Example 4B: Snake Traversal Order for MatMul

Problem: 1 MatMul op, 128x128 all tensors, fast_memory=25000, bandwidth=10, native=[128,128]

**Strategy**: Use 64x64 granularity (can't fit all 3 tensors at 128x128) with snake order [0,1,3,2].

Result latency: **6548** (vs 7096 raster order)

```json
{
  "subgraphs": [[0]],
  "granularities": [[64,64,128]],
  "tensors_to_retain": [[]],
  "traversal_orders": [[0,1,3,2]],
  "subgraph_latencies": [6548]
}
```

Tile layout (64x64 tiles of 128x128 output):
- Tile 0 (top-left, row=0, col=0): load LHS row0 + RHS col0 → latency=2048
- Tile 1 (top-right, row=0, col=1): reuse LHS row0, load RHS col1 → latency=1500
- Tile 3 (bot-right, row=1, col=1): load LHS row1, reuse RHS col1 → latency=1500
- Tile 2 (bot-left, row=1, col=0): reuse LHS row1, load RHS col0 → latency=1500
- Total = 2048+1500+1500+1500 = 6548

---

## Example 5B: Split-K for Chained MatMuls

Problem: 2 MatMul ops chained ((T0@T1)@T2), 128x128 all tensors, fast_memory=45000, bandwidth=10

**Strategy**: k=32 (split into 4 k-steps). Tensor0 (128x128=16384) and accumulator Tensor4 (128x128=16384) are resident. Stream Tensor1 strips (128x32=4096) and Tensor2 strips (32x128=4096).

Working set = 16384+16384+4096+4096 = 40960 < 45000. OK.

Result latency: **6915.2**

```json
{
  "subgraphs": [[0,1]],
  "granularities": [[128,128,32]],
  "tensors_to_retain": [[]],
  "traversal_orders": [null],
  "subgraph_latencies": [6915.2]
}
```

Calculation (4 k-steps):
- compute_per_step = 2000×(32/128) + 2000×(32/128) = 500+500 = 1000
- Step 1: load Tensor0 (16384/10=1638.4) + T1 strip (4096/10=409.6) + T2 strip (409.6) = 2457.6. latency=max(1000,2457.6)=2457.6
- Step 2: reuse Tensor0, load T1 strip (409.6) + T2 strip (409.6) = 819.2. latency=max(1000,819.2)=1000
- Step 3: same as step 2 → 1000
- Step 4: load strips (819.2) + evict T4 (1638.4) = 2457.6. latency=max(1000,2457.6)=2457.6
- Total = 2457.6+1000+1000+2457.6 = 6915.2
