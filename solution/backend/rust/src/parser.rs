/// JSON deserialization into Problem struct.

use serde::Deserialize;

use serde_json::Value;

use crate::models::{Granularity, Op, Problem, Solution, SubgraphDef, Tensor};

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

    let num_tensors = tensors.len();
    let mut ops: Vec<Op> = Vec::with_capacity(raw.inputs.len());
    for (i, (((inp, out), &cost), op_type)) in raw.inputs.iter()
        .zip(raw.outputs.iter())
        .zip(raw.base_costs.iter())
        .zip(raw.op_types.iter())
        .enumerate()
    {
        if op_type != "MatMul" && op_type != "Pointwise" {
            return Err(format!("Op {i}: unknown op_type '{op_type}'"));
        }
        if op_type == "MatMul" && inp.len() != 2 {
            return Err(format!("Op {i}: MatMul requires exactly 2 inputs, got {}", inp.len()));
        }
        if out.is_empty() {
            return Err(format!("Op {i}: outputs must not be empty"));
        }
        for &t in inp.iter().chain(out.iter()) {
            if t >= num_tensors {
                return Err(format!("Op {i}: tensor index {t} out of range (num_tensors={num_tensors})"));
            }
        }
        ops.push(Op {
            op_type: op_type.clone(),
            inputs: inp.clone(),
            outputs: out.clone(),
            base_cost: cost,
        });
    }

    Ok(Problem {
        tensors,
        ops,
        fast_memory_capacity: raw.fast_memory_capacity,
        slow_memory_bandwidth: raw.slow_memory_bandwidth,
        native_granularity: (raw.native_granularity[0], raw.native_granularity[1]),
    })
}

pub fn parse_solution(json_str: &str) -> Result<Solution, String> {
    let raw: Value =
        serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {e}"))?;

    let subgraphs_arr = raw["subgraphs"].as_array()
        .ok_or("missing 'subgraphs' array")?;
    let grans_arr = raw["granularities"].as_array()
        .ok_or("missing 'granularities' array")?;
    let retain_arr = raw["tensors_to_retain"].as_array()
        .ok_or("missing 'tensors_to_retain' array")?;
    let latencies_arr = raw["subgraph_latencies"].as_array()
        .ok_or("missing 'subgraph_latencies' array")?;
    let traversal_arr = raw.get("traversal_orders")
        .and_then(|v| v.as_array());

    let n = subgraphs_arr.len();
    let mut subgraphs = Vec::with_capacity(n);

    for i in 0..n {
        let ops: Vec<usize> = subgraphs_arr[i].as_array()
            .ok_or(format!("subgraphs[{i}] not an array"))?
            .iter()
            .enumerate()
            .map(|(j, v)| v.as_u64()
                .ok_or(format!("subgraphs[{i}][{j}] is not a valid integer"))
                .map(|n| n as usize))
            .collect::<Result<Vec<_>, _>>()?;

        let g = grans_arr.get(i).and_then(|v| v.as_array())
            .ok_or(format!("granularities[{i}] not an array"))?;
        if g.len() != 3 {
            return Err(format!("granularities[{i}] must have exactly 3 elements, got {}", g.len()));
        }
        let granularity = Granularity {
            w: g[0].as_i64().ok_or(format!("granularities[{i}][0] is not an integer"))?,
            h: g[1].as_i64().ok_or(format!("granularities[{i}][1] is not an integer"))?,
            k: g[2].as_i64().ok_or(format!("granularities[{i}][2] is not an integer"))?,
        };

        let retain_items = retain_arr.get(i)
            .and_then(|v| v.as_array())
            .ok_or(format!("tensors_to_retain[{i}] not an array"))?;
        let tensors_to_retain: Vec<usize> = retain_items.iter()
            .enumerate()
            .map(|(j, v)| v.as_u64()
                .ok_or(format!("tensors_to_retain[{i}][{j}] is not a valid integer"))
                .map(|n| n as usize))
            .collect::<Result<Vec<_>, _>>()?;

        let traversal_order: Option<Vec<i64>> = match traversal_arr.and_then(|arr| arr.get(i)) {
            Some(v) if v.is_null() => None,
            Some(v) => {
                let arr = v.as_array()
                    .ok_or(format!("traversal_orders[{i}] is not an array or null"))?;
                let order: Vec<i64> = arr.iter()
                    .enumerate()
                    .map(|(j, v)| v.as_i64()
                        .ok_or(format!("traversal_orders[{i}][{j}] is not an integer")))
                    .collect::<Result<Vec<_>, _>>()?;
                Some(order)
            }
            None => None,
        };

        let subgraph_latency = latencies_arr.get(i)
            .and_then(|v| v.as_f64())
            .ok_or(format!("subgraph_latencies[{i}] is not a number"))?;

        subgraphs.push(SubgraphDef {
            ops,
            granularity,
            tensors_to_retain,
            traversal_order,
            subgraph_latency,
        });
    }

    Ok(Solution { subgraphs })
}

/// Determine K_full for a MatMul op: LHS.width = RHS.height.
pub fn k_full_for_matmul(op: &Op, tensors: &[Tensor]) -> i64 {
    // For MatMul: inputs[0] = LHS, inputs[1] = RHS
    // K_full = LHS.width = RHS.height
    let lhs_idx = op.inputs[0];
    tensors[lhs_idx].width
}

/// Granularity at native (w, h) for a subgraph.
/// k is set to the minimum K_full across MatMul ops (safe for all ops), or 1 for pointwise-only.
pub fn native_granularity_for_subgraph(
    ops: &[usize],
    problem: &Problem,
) -> Granularity {
    let (native_w, native_h) = problem.native_granularity;
    let k = ops
        .iter()
        .filter_map(|&op_idx| {
            let op = &problem.ops[op_idx];
            if op.is_matmul() {
                Some(k_full_for_matmul(op, &problem.tensors))
            } else {
                None
            }
        })
        .min()
        .unwrap_or(1);
    Granularity { w: native_w, h: native_h, k }
}
