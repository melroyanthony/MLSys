/// Core data structures matching the C++ mlsys.h definitions.

#[derive(Debug, Clone, PartialEq)]
pub struct Tensor {
    pub width: i64,
    pub height: i64,
}

impl Tensor {
    pub fn size(&self) -> i64 {
        self.width * self.height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Op {
    pub op_type: String, // "MatMul" or "Pointwise"
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
    pub base_cost: i64,
}

impl Op {
    pub fn is_matmul(&self) -> bool {
        self.op_type == "MatMul"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Granularity {
    pub w: i64,
    pub h: i64,
    pub k: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub tensors: Vec<Tensor>,
    pub ops: Vec<Op>,
    pub fast_memory_capacity: i64,
    pub slow_memory_bandwidth: i64,
    pub native_granularity: (i64, i64), // (native_w, native_h)
}

/// A subgraph groups one or more ops executed together with a shared granularity.
#[derive(Debug, Clone)]
pub struct SubgraphDef {
    /// Op indices (in the problem's ops array) that belong to this subgraph.
    pub ops: Vec<usize>,
    pub granularity: Granularity,
    /// Tensor indices to keep resident in fast memory after this subgraph completes.
    pub tensors_to_retain: Vec<usize>,
    /// Permutation of tile indices (None = raster order).
    pub traversal_order: Option<Vec<i64>>,
    pub subgraph_latency: f64,
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub subgraphs: Vec<SubgraphDef>,
}
