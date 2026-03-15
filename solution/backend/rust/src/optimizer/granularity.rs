/// Granularity search.
///
/// For each subgraph, search candidate (w, h, k) values to minimize subgraph latency
/// while satisfying the OOM constraint.

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::latency::subgraph_latency;
use crate::memory::{check_oom, find_split_k};
use crate::models::{Granularity, Problem, SubgraphDef};
use crate::parser::k_full_for_matmul;

/// Find K_full for a subgraph: the minimum K_full across ALL MatMuls in the subgraph.
/// Internal MatMuls (ephemeral output) still drive k-step counts, so we must consider them.
fn find_k_full(ops: &[usize], problem: &Problem, _dag: &DagInfo) -> Option<i64> {
    ops.iter()
        .filter_map(|&op_idx| {
            let op = &problem.ops[op_idx];
            if op.is_matmul() {
                Some(k_full_for_matmul(op, &problem.tensors))
            } else {
                None
            }
        })
        .min()
}

/// Generate candidate w/h values for a given tensor dimension.
fn candidates_for_dim(tensor_dim: i64, native_dim: i64) -> Vec<i64> {
    let mut candidates: Vec<i64> = Vec::new();

    // Multiples of native from native up to tensor_dim
    let mut c = native_dim;
    while c <= tensor_dim {
        candidates.push(c);
        if c >= tensor_dim {
            break;
        }
        c *= 2;
    }
    // Include tensor_dim
    if !candidates.contains(&tensor_dim) {
        candidates.push(tensor_dim);
    }

    // Sub-native values (halves of native downward)
    let mut sub = native_dim / 2;
    while sub >= 1 {
        candidates.push(sub);
        sub /= 2;
    }

    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

/// Generate candidate k values from K_full downward (powers of 2).
fn candidates_for_k(k_full: i64) -> Vec<i64> {
    let mut candidates: Vec<i64> = Vec::new();
    let mut k = k_full;
    loop {
        candidates.push(k);
        if k <= 1 {
            break;
        }
        k = (k / 2).max(1);
    }
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

/// Search for the best granularity for a single subgraph.
pub fn search_best_granularity(
    subgraph_ops: &[usize],
    current_gran: &Granularity,
    tensors_to_retain: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> Granularity {
    let (native_w, native_h) = problem.native_granularity;
    let (w_out, h_out) = dag.output_dimensions(problem, subgraph_ops);

    let w_candidates = candidates_for_dim(w_out, native_w);
    let h_candidates = candidates_for_dim(h_out, native_h);

    let has_matmul = subgraph_ops
        .iter()
        .any(|&op_idx| problem.ops[op_idx].is_matmul());

    // Find K_full for generating k candidates
    let k_full = if has_matmul {
        find_k_full(subgraph_ops, problem, dag).unwrap_or(current_gran.k)
    } else {
        1
    };

    let k_candidates = if has_matmul {
        candidates_for_k(k_full)
    } else {
        vec![1]
    };

    let mut best_latency = f64::INFINITY;
    let mut best_gran = current_gran.clone();

    // Try all (w, h, k) combinations
    // For each (w, h), we only need to find the LARGEST k that fits (for the best latency),
    // but we should try all k candidates since smaller k can sometimes reduce latency
    // by reducing memory transfers per step.
    for &w in &w_candidates {
        for &h in &h_candidates {
            // For this (w, h), find largest valid k first to ensure we start with feasible ones
            let gran_base = Granularity { w, h, k: k_full };
            let largest_k = if has_matmul {
                find_split_k(
                    subgraph_ops,
                    &gran_base,
                    tensors_to_retain,
                    previously_retained,
                    problem,
                    dag,
                )
                .unwrap_or(0)
            } else {
                if check_oom(
                    subgraph_ops,
                    &Granularity { w, h, k: 1 },
                    tensors_to_retain,
                    previously_retained,
                    problem,
                    dag,
                ) {
                    1
                } else {
                    0
                }
            };

            if largest_k == 0 {
                // No k works for this (w, h)
                continue;
            }

            // Try k candidates up to largest_k
            for &k in k_candidates.iter().filter(|&&k| k <= largest_k) {
                let trial = Granularity { w, h, k };

                let lat = subgraph_latency(
                    subgraph_ops,
                    &trial,
                    tensors_to_retain,
                    previously_retained,
                    problem,
                    dag,
                );

                // Prefer lower latency; tie-break with larger k (fewer k-steps).
                // Use relative tolerance for float equality to handle accumulation error.
                let effectively_equal = (lat - best_latency).abs() <= 1.0_f64.max(best_latency.abs()) * 1e-9;
                if lat < best_latency || (effectively_equal && trial.k > best_gran.k) {
                    best_latency = lat;
                    best_gran = trial;
                }
            }
        }
    }

    best_gran
}

/// Apply granularity search to all subgraphs.
pub fn optimize_granularities(
    subgraphs: &mut Vec<SubgraphDef>,
    problem: &Problem,
    dag: &DagInfo,
) {
    let mut previously_retained: HashSet<usize> = HashSet::new();

    for sg in subgraphs.iter_mut() {
        let best_gran = search_best_granularity(
            &sg.ops,
            &sg.granularity,
            &sg.tensors_to_retain,
            &previously_retained,
            problem,
            dag,
        );

        sg.granularity = best_gran;
        previously_retained = sg.tensors_to_retain.iter().copied().collect();
    }
}
