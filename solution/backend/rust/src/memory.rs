/// Working-set calculator and OOM checker.
///
/// The working set is the maximum number of fast-memory elements needed simultaneously
/// during any single execution step of a subgraph.

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::models::{Granularity, Problem};
use crate::parser::k_full_for_matmul;

/// Calculate the peak working-set size (in elements) for a subgraph.
///
/// Working set includes:
/// 1. Previously retained tensors (full size, already in fast memory)
/// 2. Boundary input slices needed during one execution step
/// 3. Output slice (accumulator for split-K, or eviction target)
///
/// Ephemeral tensors (produced and consumed within the same subgraph) consume 0 capacity.
pub fn working_set_size(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    _tensors_to_retain: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> i64 {
    let op_set: HashSet<usize> = subgraph_ops.iter().copied().collect();
    let w = granularity.w;
    let h = granularity.h;
    let k = granularity.k;

    // Previously retained tensors occupy their full size in fast memory.
    let retained_size: i64 = previously_retained
        .iter()
        .map(|&t| problem.tensors[t].size())
        .sum();

    let mut ws: i64 = retained_size;
    let mut seen: HashSet<usize> = HashSet::new();

    for &op_idx in subgraph_ops {
        let op = &problem.ops[op_idx];

        if !op.is_matmul() {
            // Pointwise: input slice = w * h, output slice = w * h
            for &in_t in &op.inputs {
                let produced_inside = dag.tensor_producer[in_t]
                    .map(|p| op_set.contains(&p))
                    .unwrap_or(false);
                if !produced_inside && !seen.contains(&in_t) && !previously_retained.contains(&in_t) {
                    seen.insert(in_t);
                    ws += w * h;
                }
            }
            for &out_t in &op.outputs {
                let output_ephemeral = !dag.graph_outputs.contains(&out_t)
                    && !dag.tensor_consumers[out_t].is_empty()
                    && dag.tensor_consumers[out_t].iter().all(|c| op_set.contains(c));
                if !output_ephemeral && !seen.contains(&out_t) {
                    seen.insert(out_t);
                    ws += w * h;
                }
            }
            continue;
        }

        // MatMul op
        let lhs_idx = op.inputs[0];
        let rhs_idx = op.inputs[1];
        let out_t = op.outputs[0];

        let output_ephemeral = !dag.graph_outputs.contains(&out_t)
            && !dag.tensor_consumers[out_t].is_empty()
            && dag.tensor_consumers[out_t].iter().all(|c| op_set.contains(c));

        // LHS input
        let lhs_boundary = !dag.tensor_producer[lhs_idx]
            .map(|p| op_set.contains(&p))
            .unwrap_or(false);
        if lhs_boundary && !seen.contains(&lhs_idx) && !previously_retained.contains(&lhs_idx) {
            seen.insert(lhs_idx);
            let sz = if output_ephemeral {
                // Upstream LHS: row strip = h * lhs.width (where lhs.width = K_full of this op)
                h * problem.tensors[lhs_idx].width
            } else {
                // Standard LHS slice = h * k
                h * k
            };
            ws += sz;
        }

        // RHS input
        let rhs_boundary = !dag.tensor_producer[rhs_idx]
            .map(|p| op_set.contains(&p))
            .unwrap_or(false);
        if rhs_boundary && !seen.contains(&rhs_idx) && !previously_retained.contains(&rhs_idx) {
            seen.insert(rhs_idx);
            let sz = if output_ephemeral {
                // Upstream RHS: col strip = rhs.height * k (rhs.height = K_full of this op)
                problem.tensors[rhs_idx].height * k
            } else {
                // Standard RHS slice = k * w
                k * w
            };
            ws += sz;
        }

        // Output
        if !output_ephemeral && !seen.contains(&out_t) {
            seen.insert(out_t);
            ws += w * h; // output/accumulator slice
        }
    }

    ws
}

/// Check if a subgraph fits in fast memory.
pub fn check_oom(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    tensors_to_retain: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> bool {
    working_set_size(
        subgraph_ops,
        granularity,
        tensors_to_retain,
        previously_retained,
        problem,
        dag,
    ) <= problem.fast_memory_capacity
}

/// Find the largest k (power-of-2 downward from max K_full) that fits in memory.
pub fn find_split_k(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    tensors_to_retain: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> Option<i64> {
    // Find the MAXIMUM K_full across all MatMul ops (for split-K search range)
    let max_k_full = subgraph_ops
        .iter()
        .filter_map(|&op_idx| {
            let op = &problem.ops[op_idx];
            if op.is_matmul() {
                Some(k_full_for_matmul(op, &problem.tensors))
            } else {
                None
            }
        })
        .max();

    let k_full = match max_k_full {
        Some(kf) => kf,
        None => {
            // No MatMul ops -- try with k=1 for pointwise-only
            let trial = Granularity { k: 1, ..granularity.clone() };
            return if check_oom(subgraph_ops, &trial, tensors_to_retain, previously_retained, problem, dag) {
                Some(1)
            } else {
                None
            };
        }
    };

    // Try k values from k_full downward by halving
    let mut k = k_full;
    loop {
        let trial = Granularity { k, ..granularity.clone() };
        if check_oom(subgraph_ops, &trial, tensors_to_retain, previously_retained, problem, dag) {
            return Some(k);
        }
        if k <= 1 {
            break;
        }
        k = (k / 2).max(1);
    }

    None
}
