/// Naive baseline scheduler: one op per subgraph, native granularity, no retention.
///
/// This always produces a valid (no-OOM) solution. It may be suboptimal but is
/// the fallback if all other optimizations fail.

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::latency::subgraph_latency;
use crate::models::{Granularity, Problem, Solution, SubgraphDef};
use crate::parser::k_full_for_matmul;

pub fn build_baseline(problem: &Problem, dag: &DagInfo) -> Solution {
    let (native_w, native_h) = problem.native_granularity;
    let mut subgraphs: Vec<SubgraphDef> = Vec::new();
    let mut previously_retained: HashSet<usize> = HashSet::new();

    for &op_idx in &dag.topo_order {
        let op = &problem.ops[op_idx];
        let ops = vec![op_idx];

        // Determine k: K_full for MatMul, 1 for Pointwise
        let k = if op.is_matmul() {
            k_full_for_matmul(op, &problem.tensors)
        } else {
            1
        };

        let granularity = Granularity { w: native_w, h: native_h, k };

        let lat = subgraph_latency(
            &ops,
            &granularity,
            &[],
            &previously_retained,
            problem,
            dag,
        );

        subgraphs.push(SubgraphDef {
            ops,
            granularity,
            tensors_to_retain: vec![],
            traversal_order: None,
            subgraph_latency: lat,
        });

        previously_retained.clear();
    }

    Solution { subgraphs }
}
