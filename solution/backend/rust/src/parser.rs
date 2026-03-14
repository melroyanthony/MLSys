/// JSON deserialization into Problem struct.

use serde::Deserialize;

use crate::models::{Granularity, Op, Problem, Tensor};

#[derive(Deserialize)]
struct ProblemJson {
    widths: Vec<i64>,
    heights: Vec<i64>,
    inputs: Vec<Vec<usize>>,
    outputs: Vec<Vec<usize>>,
    base_costs: Vec<i64>,
    op_types: Vec<String>,
    fast_memory_capacity: i64,
    slow_memory_bandwidth: i64,
    native_granularity: Vec<i64>,
}

pub fn parse_problem(json_str: &str) -> Result<Problem, String> {
    let raw: ProblemJson =
        serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {e}"))?;

    if raw.widths.len() != raw.heights.len() {
        return Err("widths and heights length mismatch".to_string());
    }
    if raw.inputs.len() != raw.outputs.len()
        || raw.inputs.len() != raw.base_costs.len()
        || raw.inputs.len() != raw.op_types.len()
    {
        return Err("inputs/outputs/base_costs/op_types length mismatch".to_string());
    }
    if raw.native_granularity.len() != 2 {
        return Err("native_granularity must have exactly 2 elements".to_string());
    }

    let tensors: Vec<Tensor> = raw
        .widths
        .iter()
        .zip(raw.heights.iter())
        .map(|(&w, &h)| Tensor { width: w, height: h })
        .collect();

    let ops: Vec<Op> = raw
        .inputs
        .iter()
        .zip(raw.outputs.iter())
        .zip(raw.base_costs.iter())
        .zip(raw.op_types.iter())
        .map(|(((inp, out), &cost), op_type)| Op {
            op_type: op_type.clone(),
            inputs: inp.clone(),
            outputs: out.clone(),
            base_cost: cost,
        })
        .collect();

    Ok(Problem {
        tensors,
        ops,
        fast_memory_capacity: raw.fast_memory_capacity,
        slow_memory_bandwidth: raw.slow_memory_bandwidth,
        native_granularity: (raw.native_granularity[0], raw.native_granularity[1]),
    })
}

/// Determine K_full for a MatMul op: LHS.width = RHS.height.
pub fn k_full_for_matmul(op: &Op, tensors: &[Tensor]) -> i64 {
    // For MatMul: inputs[0] = LHS, inputs[1] = RHS
    // K_full = LHS.width = RHS.height
    let lhs_idx = op.inputs[0];
    tensors[lhs_idx].width
}

/// Granularity at native (w, h) for a subgraph.
/// k is set to K_full for the first MatMul op in the subgraph, or 1 for pointwise-only.
pub fn native_granularity_for_subgraph(
    ops: &[usize],
    problem: &Problem,
) -> Granularity {
    let (native_w, native_h) = problem.native_granularity;
    // Find K_full from first MatMul in subgraph
    let k = ops
        .iter()
        .find_map(|&op_idx| {
            let op = &problem.ops[op_idx];
            if op.is_matmul() {
                Some(k_full_for_matmul(op, &problem.tensors))
            } else {
                None
            }
        })
        .unwrap_or(1);
    Granularity { w: native_w, h: native_h, k }
}
