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
use crate::parser::{k_full_for_matmul, native_granularity_for_subgraph};

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

                // Structural validity: merged ops must have consistent boundary
                // output dimensions for the shared granularity to be meaningful.
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

                // K_full consistency: all MatMuls in a merged subgraph must share
                // the same K_full, since the subgraph has a single k-step loop.
                let matmul_k_fulls: Vec<i64> = merged.iter()
                    .filter(|&&op_idx| problem.ops[op_idx].is_matmul())
                    .map(|&op_idx| k_full_for_matmul(&problem.ops[op_idx], &problem.tensors))
                    .collect();
                let k_full_consistent = matmul_k_fulls.is_empty()
                    || matmul_k_fulls.iter().all(|&kf| kf == matmul_k_fulls[0]);

                if dims_consistent && k_full_consistent {
                    if let Some(merged_gran) =
                        find_feasible_granularity(&merged, &retained_before, problem, dag)
                    {
                        // Cost-based fusion decision: only merge if fusing reduces total latency.
                        // Compare fused latency vs. (latency_a + latency_b + DRAM boundary cost).
                        let lat_fused = subgraph_latency(
                            &merged,
                            &merged_gran,
                            &[],
                            &retained_before,
                            problem,
                            dag,
                        );

                        // Individual granularities: use the first feasible one for each group.
                        let gran_a = find_feasible_granularity(&groups[i], &retained_before, problem, dag)
                            .unwrap_or_else(|| native_granularity_for_subgraph(&groups[i], problem));
                        let gran_b = find_feasible_granularity(&groups[i + 1], &retained_before, problem, dag)
                            .unwrap_or_else(|| native_granularity_for_subgraph(&groups[i + 1], problem));

                        let lat_a = subgraph_latency(
                            &groups[i],
                            &gran_a,
                            &[],
                            &retained_before,
                            problem,
                            dag,
                        );
                        let lat_b = subgraph_latency(
                            &groups[i + 1],
                            &gran_b,
                            &[],
                            &retained_before,
                            problem,
                            dag,
                        );

                        let boundary_cost =
                            compute_boundary_dram_cost(&groups[i], &groups[i + 1], problem, dag);

                        if lat_fused < lat_a + lat_b + boundary_cost {
                            new_groups.push(merged);
                            i += 2;
                            changed = true;
                            continue;
                        }
                    }
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

/// Compute the DRAM round-trip cost for tensors at the boundary between group A and group B.
///
/// The boundary tensors are those produced by ops in group_a and consumed by ops in group_b.
/// Each must be fully materialized in DRAM (write from A + read into B), so cost = 2 * size / bw.
fn compute_boundary_dram_cost(
    group_a: &[usize],
    group_b: &[usize],
    problem: &Problem,
    dag: &DagInfo,
) -> f64 {
    let op_set_a: HashSet<usize> = group_a.iter().copied().collect();
    let op_set_b: HashSet<usize> = group_b.iter().copied().collect();
    let bw = problem.slow_memory_bandwidth as f64;

    let mut cost = 0.0;
    let mut seen: HashSet<usize> = HashSet::new();

    for &op_idx in group_a {
        for &out_t in &problem.ops[op_idx].outputs {
            if seen.contains(&out_t) {
                continue;
            }
            // Is this tensor consumed by any op in group_b?
            let consumed_by_b = dag.tensor_consumers[out_t]
                .iter()
                .any(|c| op_set_b.contains(c));
            if consumed_by_b {
                seen.insert(out_t);
                let tensor = &problem.tensors[out_t];
                let full_size = (tensor.width * tensor.height) as f64;
                // Write from A + read into B
                cost += 2.0 * full_size / bw;
            }
        }
    }

    cost
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
