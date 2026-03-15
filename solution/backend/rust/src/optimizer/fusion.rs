/// Greedy cost-based chain fusion.
///
/// Merges adjacent ops (in topological order) into subgraphs where the
/// merged working set fits in fast memory AND fusing reduces total latency
/// compared to executing the two subgraphs separately with a DRAM round-trip
/// at their boundary.

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::latency::subgraph_latency;
use crate::memory::find_split_k;
use crate::models::{Granularity, Problem, SubgraphDef};
use crate::optimizer::granularity::search_best_granularity;
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

    // Cache best latency per group to avoid redundant granularity searches.
    // Entry is (best_granularity, best_latency). Invalidated on merge.
    let retained_before: HashSet<usize> = HashSet::new();
    let mut cache: Vec<Option<(Granularity, f64)>> = groups
        .iter()
        .map(|ops| {
            let base = native_granularity_for_subgraph(ops, problem);
            let gran = search_best_granularity(ops, &base, &[], &retained_before, problem, dag);
            let lat = subgraph_latency(ops, &gran, &[], &retained_before, problem, dag);
            Some((gran, lat))
        })
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        let mut new_groups: Vec<Vec<usize>> = Vec::new();
        let mut new_cache: Vec<Option<(Granularity, f64)>> = Vec::new();
        let mut i = 0;

        while i < groups.len() {
            if i + 1 < groups.len() {
                let merged: Vec<usize> = groups[i]
                    .iter()
                    .chain(groups[i + 1].iter())
                    .copied()
                    .collect();

                // Structural validity: consistent boundary output dimensions.
                let boundary_outputs = dag.boundary_outputs(problem, &merged);
                let dims_consistent = if boundary_outputs.is_empty() {
                    true
                } else {
                    let first = boundary_outputs[0];
                    let (w0, h0) = (problem.tensors[first].width, problem.tensors[first].height);
                    boundary_outputs.iter().all(|&t| {
                        problem.tensors[t].width == w0 && problem.tensors[t].height == h0
                    })
                };

                if dims_consistent {
                    if find_feasible_granularity(&merged, &retained_before, problem, dag).is_some()
                    {
                        let base_merged = native_granularity_for_subgraph(&merged, problem);
                        let merged_gran = search_best_granularity(
                            &merged, &base_merged, &[], &retained_before, problem, dag,
                        );
                        let lat_fused = subgraph_latency(
                            &merged, &merged_gran, &[], &retained_before, problem, dag,
                        );

                        // Use cached latencies for individual groups.
                        let lat_a = cache[i].as_ref().map(|(_, l)| *l).unwrap_or_else(|| {
                            let base = native_granularity_for_subgraph(&groups[i], problem);
                            let g = search_best_granularity(&groups[i], &base, &[], &retained_before, problem, dag);
                            subgraph_latency(&groups[i], &g, &[], &retained_before, problem, dag)
                        });
                        let lat_b = cache[i + 1].as_ref().map(|(_, l)| *l).unwrap_or_else(|| {
                            let base = native_granularity_for_subgraph(&groups[i + 1], problem);
                            let g = search_best_granularity(&groups[i + 1], &base, &[], &retained_before, problem, dag);
                            subgraph_latency(&groups[i + 1], &g, &[], &retained_before, problem, dag)
                        });

                        // Only merge when fused is meaningfully better (relative tolerance).
                        let lat_split = lat_a + lat_b;
                        let eps = 1.0_f64.max(lat_split.abs()) * 1e-9;
                        if lat_fused < lat_split - eps {
                            new_groups.push(merged);
                            // Cache the merged group's result.
                            new_cache.push(Some((merged_gran, lat_fused)));
                            i += 2;
                            changed = true;
                            continue;
                        }
                    }
                }
            }
            new_groups.push(groups[i].clone());
            new_cache.push(cache[i].take());
            i += 1;
        }

        groups = new_groups;
        cache = new_cache;
    }

    // Convert groups back to SubgraphDef, reusing cached best granularity.
    let mut result: Vec<SubgraphDef> = Vec::new();

    for (i, ops) in groups.iter().enumerate() {
        let gran = cache[i]
            .as_ref()
            .map(|(g, _)| g.clone())
            .unwrap_or_else(|| {
                find_feasible_granularity(ops, &retained_before, problem, dag)
                    .unwrap_or_else(|| native_granularity_for_subgraph(ops, problem))
            });

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
