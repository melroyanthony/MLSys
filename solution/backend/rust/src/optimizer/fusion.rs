/// Greedy bottom-up chain fusion.
///
/// Merges adjacent ops (in topological order) into subgraphs where the
/// merged working set fits in fast memory (with any valid granularity).

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::memory::find_split_k;
use crate::models::{Granularity, Problem, SubgraphDef};
use crate::parser::native_granularity_for_subgraph;

/// Attempt to fuse the subgraphs in topological order.
/// Returns a new list of subgraphs where adjacent groups have been merged
/// where memory allows (with any valid granularity, including split-K).
pub fn greedy_fusion(
    problem: &Problem,
    dag: &DagInfo,
    subgraphs: &[SubgraphDef],
    _previously_retained_per_sg: &[HashSet<usize>],
) -> Vec<SubgraphDef> {
    if subgraphs.is_empty() {
        return vec![];
    }

    let mut groups: Vec<Vec<usize>> = subgraphs.iter().map(|sg| sg.ops.clone()).collect();

    let mut changed = true;
    while changed {
        changed = false;
        let mut new_groups: Vec<Vec<usize>> = Vec::new();
        let mut i = 0;

        while i < groups.len() {
            if i + 1 < groups.len() {
                let merged: Vec<usize> = groups[i]
                    .iter()
                    .chain(groups[i + 1].iter())
                    .copied()
                    .collect();

                let retained_before: HashSet<usize> = HashSet::new();

                if find_feasible_granularity(&merged, &retained_before, problem, dag).is_some() {
                    new_groups.push(merged);
                    i += 2;
                    changed = true;
                    continue;
                }
            }
            new_groups.push(groups[i].clone());
            i += 1;
        }

        groups = new_groups;
    }

    // Convert groups back to SubgraphDef with the best feasible granularity
    let mut result: Vec<SubgraphDef> = Vec::new();
    let prev_retained: HashSet<usize> = HashSet::new();

    for ops in &groups {
        let gran = find_feasible_granularity(ops, &prev_retained, problem, dag)
            .unwrap_or_else(|| native_granularity_for_subgraph(ops, problem));

        result.push(SubgraphDef {
            ops: ops.clone(),
            granularity: gran,
            tensors_to_retain: vec![],
            traversal_order: None,
            subgraph_latency: 0.0,
        });
    }

    result
}

/// Find the smallest feasible granularity for a subgraph (any k that fits in memory).
/// Tries native spatial granularity with decreasing k values.
/// Returns None if no granularity fits.
pub fn find_feasible_granularity(
    ops: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> Option<Granularity> {
    let (native_w, native_h) = problem.native_granularity;
    let base_gran = native_granularity_for_subgraph(ops, problem);

    // First try with native spatial granularity
    if let Some(k) = find_split_k(ops, &base_gran, &[], previously_retained, problem, dag) {
        return Some(Granularity {
            w: native_w,
            h: native_h,
            k,
        });
    }

    // If that doesn't work, try halving spatial dimensions
    let (w_out, h_out) = dag.output_dimensions(problem, ops);
    let mut w = native_w;
    while w >= 1 {
        let mut h = native_h;
        while h >= 1 {
            let trial_gran = Granularity { w, h, k: base_gran.k };
            if let Some(k) =
                find_split_k(ops, &trial_gran, &[], previously_retained, problem, dag)
            {
                return Some(Granularity { w, h, k });
            }
            if h == 1 {
                break;
            }
            h /= 2;
        }
        if w == 1 {
            break;
        }
        w /= 2;
    }

    None
}
