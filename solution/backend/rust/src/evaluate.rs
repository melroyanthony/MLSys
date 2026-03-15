/// Full evaluator matching C++ Evaluate() semantics.
///
/// Validates a solution and returns the total latency, or an error if the
/// solution is invalid (OOM, missing ops, latency mismatch, etc.).

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::latency::subgraph_latency;
use crate::memory::check_oom;
use crate::models::{Problem, Solution};

pub struct EvaluateResult {
    pub total_latency: f64,
    pub subgraph_latencies: Vec<f64>,
}

const LATENCY_TOLERANCE: f64 = 0.5;

pub fn evaluate(problem: &Problem, solution: &Solution) -> Result<EvaluateResult, String> {
    let dag = DagInfo::build(problem)?;

    // Validate: every op must appear in at least one subgraph
    let num_ops = problem.ops.len();
    let mut op_covered = vec![false; num_ops];
    for sg in &solution.subgraphs {
        for &op_idx in &sg.ops {
            if op_idx >= num_ops {
                return Err(format!("Op index {op_idx} out of range"));
            }
            op_covered[op_idx] = true;
        }
    }
    for (i, covered) in op_covered.iter().enumerate() {
        if !covered {
            return Err(format!("Op {i} not covered by any subgraph"));
        }
    }

    let mut total_latency = 0.0;
    let mut subgraph_latencies = Vec::with_capacity(solution.subgraphs.len());
    let mut previously_retained: HashSet<usize> = HashSet::new();

    for (sg_idx, sg) in solution.subgraphs.iter().enumerate() {
        // Validate traversal_order if present
        if let Some(ref order) = sg.traversal_order {
            let out_dims = dag.output_dimensions(problem, &sg.ops);
            let num_tiles_w = (out_dims.0 + sg.granularity.w - 1) / sg.granularity.w;
            let num_tiles_h = (out_dims.1 + sg.granularity.h - 1) / sg.granularity.h;
            let num_tiles = (num_tiles_w * num_tiles_h) as usize;

            if order.len() != num_tiles {
                return Err(format!(
                    "Subgraph {sg_idx}: traversal_order has {} elements but expected {num_tiles} tiles",
                    order.len()
                ));
            }
            let mut seen = vec![false; num_tiles];
            for &tile_idx in order {
                if tile_idx < 0 || (tile_idx as usize) >= num_tiles {
                    return Err(format!(
                        "Subgraph {sg_idx}: traversal_order contains invalid tile index {tile_idx}"
                    ));
                }
                if seen[tile_idx as usize] {
                    return Err(format!(
                        "Subgraph {sg_idx}: traversal_order contains duplicate tile index {tile_idx}"
                    ));
                }
                seen[tile_idx as usize] = true;
            }
        }

        // Validate MatMul K_full consistency and k <= K_full
        let matmul_k_fulls: Vec<i64> = sg.ops.iter()
            .filter_map(|&op_idx| {
                let op = &problem.ops[op_idx];
                if op.is_matmul() {
                    Some(crate::parser::k_full_for_matmul(op, &problem.tensors))
                } else {
                    None
                }
            })
            .collect();
        if !matmul_k_fulls.is_empty() {
            // All MatMuls in a subgraph must share the same K_full
            if !matmul_k_fulls.iter().all(|&kf| kf == matmul_k_fulls[0]) {
                return Err(format!(
                    "Subgraph {sg_idx}: MatMul ops have inconsistent K_full values: {:?}",
                    matmul_k_fulls
                ));
            }
            // k must not exceed K_full
            if sg.granularity.k > matmul_k_fulls[0] {
                return Err(format!(
                    "Subgraph {sg_idx}: granularity k={} exceeds K_full={}",
                    sg.granularity.k, matmul_k_fulls[0]
                ));
            }
        }

        // OOM check
        if !check_oom(
            &sg.ops,
            &sg.granularity,
            &sg.tensors_to_retain,
            &previously_retained,
            problem,
            &dag,
        ) {
            return Err(format!("Subgraph {sg_idx} OOM"));
        }

        // Compute latency
        let lat = subgraph_latency(
            &sg.ops,
            &sg.granularity,
            &sg.tensors_to_retain,
            &previously_retained,
            problem,
            &dag,
        );

        // Validate reported latency matches computed latency
        if (sg.subgraph_latency - lat).abs() > LATENCY_TOLERANCE {
            return Err(format!(
                "Subgraph {sg_idx}: reported latency {:.2} does not match computed latency {:.2} (tolerance {LATENCY_TOLERANCE})",
                sg.subgraph_latency, lat
            ));
        }

        subgraph_latencies.push(lat);
        total_latency += lat;

        // Update retained set
        previously_retained.clear();
        for &t in &sg.tensors_to_retain {
            previously_retained.insert(t);
        }
    }

    Ok(EvaluateResult {
        total_latency,
        subgraph_latencies,
    })
}
