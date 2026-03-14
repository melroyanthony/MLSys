/// Split-K search for MatMul subgraphs.
///
/// For MatMul-containing subgraphs where the full-k working set exceeds fast memory,
/// find the largest k (divisor of K_full) that fits within the memory constraint.

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::memory::{check_oom, find_split_k};
use crate::models::{Problem, SubgraphDef};

/// Apply split-K optimization to all subgraphs.
/// For each subgraph with a MatMul, if the current granularity causes OOM,
/// reduce k to the largest valid value.
pub fn apply_splitk(
    subgraphs: &mut Vec<SubgraphDef>,
    problem: &Problem,
    dag: &DagInfo,
    retained_per_sg: &[HashSet<usize>],
) {
    for (i, sg) in subgraphs.iter_mut().enumerate() {
        let has_matmul = sg.ops.iter().any(|&op_idx| problem.ops[op_idx].is_matmul());
        if !has_matmul {
            continue;
        }

        let prev_retained = &retained_per_sg[i];

        // Check if current granularity already fits
        if check_oom(&sg.ops, &sg.granularity, &sg.tensors_to_retain, prev_retained, problem, dag) {
            continue;
        }

        // Find a valid k
        if let Some(k) = find_split_k(
            &sg.ops,
            &sg.granularity,
            &sg.tensors_to_retain,
            prev_retained,
            problem,
            dag,
        ) {
            sg.granularity.k = k;
        } else {
            // Cannot fit even with k=1; this is an error condition.
            // Fall back to k=1 and hope spatial granularity reduction fixes it.
            sg.granularity.k = 1;
        }
    }
}

/// Build the per-subgraph previously-retained tensor sets from the current solution.
pub fn build_retained_sets(subgraphs: &[SubgraphDef]) -> Vec<HashSet<usize>> {
    let mut sets: Vec<HashSet<usize>> = Vec::with_capacity(subgraphs.len());
    let mut prev: HashSet<usize> = HashSet::new();

    for sg in subgraphs {
        sets.push(prev.clone());
        prev = sg.tensors_to_retain.iter().copied().collect();
    }

    sets
}
