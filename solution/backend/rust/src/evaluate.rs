/// Full evaluator matching C++ Evaluate() semantics.
///
/// Validates a solution and returns the total latency, or an error if the
/// solution is invalid (OOM, missing ops, etc.).

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::latency::subgraph_latency;
use crate::memory::check_oom;
use crate::models::{Problem, Solution};

pub struct EvaluateResult {
    pub total_latency: f64,
    pub subgraph_latencies: Vec<f64>,
}

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

        subgraph_latencies.push(lat);
        total_latency += lat;

        // Update retained set: previous retained are evicted (unless the current subgraph
        // also retains them), then new retentions added.
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
