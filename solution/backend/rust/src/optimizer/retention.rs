/// Tensor retention optimization.
///
/// After each subgraph, determine which output tensors to keep resident in fast memory
/// for the immediately following subgraph, if doing so saves bandwidth.

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::memory::check_oom;
use crate::models::{Problem, SubgraphDef};

/// For each subgraph boundary, decide which output tensors to retain.
/// A tensor should be retained if:
/// 1. It is consumed by the NEXT subgraph.
/// 2. Retaining it (including its full size in fast memory) doesn't cause the next subgraph to OOM.
/// 3. Retaining it reduces total latency (avoids a re-load cost in the next subgraph).
pub fn optimize_retention(
    subgraphs: &mut Vec<SubgraphDef>,
    problem: &Problem,
    dag: &DagInfo,
) {
    let n = subgraphs.len();
    if n == 0 {
        return;
    }

    let mut previously_retained: HashSet<usize> = HashSet::new();

    for i in 0..n {
        let ops = subgraphs[i].ops.clone();
        let gran = subgraphs[i].granularity.clone();

        // Find all tensors produced by subgraph i that are consumed by subgraph i+1
        let mut candidates: Vec<usize> = Vec::new();
        if i + 1 < n {
            let next_ops = &subgraphs[i + 1].ops;
            let next_input_tensors: HashSet<usize> = next_ops
                .iter()
                .flat_map(|&op_idx| problem.ops[op_idx].inputs.iter().copied())
                .collect();

            // Produced by current subgraph (boundary outputs)
            let produced = dag.boundary_outputs(problem, &ops);
            for t in produced {
                if next_input_tensors.contains(&t) {
                    candidates.push(t);
                }
            }
        }

        // Try retaining each candidate and check if it fits in the next subgraph
        let mut to_retain: Vec<usize> = Vec::new();
        if i + 1 < n {
            let next_ops = subgraphs[i + 1].ops.clone();
            let next_gran = subgraphs[i + 1].granularity.clone();

            for &cand in &candidates {
                let mut trial_retain = to_retain.clone();
                trial_retain.push(cand);

                let trial_retained_set: HashSet<usize> = trial_retain.iter().copied().collect();

                // Check if the NEXT subgraph fits with these tensors retained
                if check_oom(
                    &next_ops,
                    &next_gran,
                    &[],
                    &trial_retained_set,
                    problem,
                    dag,
                ) {
                    to_retain.push(cand);
                }
            }
        }

        subgraphs[i].tensors_to_retain = to_retain;

        // Update previously_retained for the next subgraph
        previously_retained = subgraphs[i]
            .tensors_to_retain
            .iter()
            .copied()
            .collect();
    }
}
