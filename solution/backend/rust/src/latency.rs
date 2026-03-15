/// Latency model: compute_time, memory_time, subgraph_latency.
/// Implements the roofline model matching C++ Evaluate() exactly.
///
/// Key formulas derived from PROBLEM.md worked examples:
/// - step_latency = max(compute_time, memory_time)
/// - subgraph_latency = sum of step_latencies over all steps
/// - compute_time for MatMul: base_cost * (k / K_full)
/// - compute_time for Pointwise: base_cost
/// - memory_time = elements_transferred / slow_memory_bandwidth

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::models::{Granularity, Problem};
use crate::parser::k_full_for_matmul;

/// Compute cost for MatMul ops only for one step of a subgraph.
///
/// Each MatMul op is scaled by its own K_full:
///   base_cost * (k / K_full_for_this_op)
pub fn matmul_compute_per_step(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    problem: &Problem,
) -> f64 {
    let k = granularity.k as f64;
    let mut total: f64 = 0.0;
    for &op_idx in subgraph_ops {
        let op = &problem.ops[op_idx];
        if op.is_matmul() {
            let op_k_full = k_full_for_matmul(op, &problem.tensors) as f64;
            total += op.base_cost as f64 * (k / op_k_full);
        }
    }
    total
}

/// Compute cost for Pointwise ops only (independent of k).
///
/// Pointwise ops execute once per spatial tile (on the last k-step when in split-K mode).
pub fn pointwise_compute(subgraph_ops: &[usize], problem: &Problem) -> f64 {
    let mut total: f64 = 0.0;
    for &op_idx in subgraph_ops {
        let op = &problem.ops[op_idx];
        if !op.is_matmul() {
            total += op.base_cost as f64;
        }
    }
    total
}

/// Compute cost for one step of a subgraph (legacy: all ops, every step).
///
/// Each MatMul op is scaled by its own K_full:
///   base_cost * (k / K_full_for_this_op)
/// Pointwise ops always pay full base_cost per step.
#[allow(dead_code)]
pub fn compute_time_per_step(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    problem: &Problem,
    _dag: &DagInfo,
) -> f64 {
    matmul_compute_per_step(subgraph_ops, granularity, problem)
        + pointwise_compute(subgraph_ops, problem)
}

/// Classify inputs/outputs needed per step for a subgraph.
///
/// Returns:
/// - full_load: tensors loaded ONCE at the start of each spatial tile (reused across k-steps)
/// - k_strip: tensors loaded fresh at each k-step
/// - out_evict_size: size of output slice evicted on the last k-step
pub struct StepMemoryPlan {
    /// (tensor_id, elements) for tensors that benefit from row-reuse.
    /// In split-K mode (num_k_steps > 1): loaded on first k-step of each spatial tile.
    /// In spatial-only mode (num_k_steps == 1): loaded only on first column of each row
    /// (reused across columns in the same tile-row, e.g., MatMul LHS row strips).
    pub full_load: Vec<(usize, i64)>,
    /// (tensor_id, elements) for tensors loaded at each k-step
    pub k_strip: Vec<(usize, i64)>,
    /// (tensor_id, elements) for Pointwise inputs: loaded once per spatial tile (first k-step
    /// in split-K mode, every tile in spatial-only mode), no row-reuse benefit (each tile
    /// needs its own slice).
    pub pw_load: Vec<(usize, i64)>,
    /// elements evicted to slow memory on the last k-step of each spatial tile
    pub out_evict_size: i64,
    /// retained tensors from prior subgraphs (pre-loaded, cost=0 except size is counted in WS)
    pub pre_retained: Vec<usize>,
}

/// Build the per-step memory plan for a subgraph.
pub fn build_memory_plan_pub(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    tensors_to_retain: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> StepMemoryPlan {
    build_memory_plan(subgraph_ops, granularity, tensors_to_retain, previously_retained, problem, dag)
}

fn build_memory_plan(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    tensors_to_retain: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> StepMemoryPlan {
    let op_set: HashSet<usize> = subgraph_ops.iter().copied().collect();
    let retain_after: HashSet<usize> = tensors_to_retain.iter().copied().collect();
    let w = granularity.w;
    let h = granularity.h;
    let k = granularity.k;

    let mut full_load: Vec<(usize, i64)> = Vec::new();
    let mut k_strip: Vec<(usize, i64)> = Vec::new();
    let mut pw_load: Vec<(usize, i64)> = Vec::new();
    let mut pre_retained: Vec<usize> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();

    for &op_idx in subgraph_ops {
        let op = &problem.ops[op_idx];

        if !op.is_matmul() {
            // Pointwise: inputs loaded once per spatial tile (PW executes on last k-step only).
            // Place in pw_load so they are charged on the first k-step of each spatial tile.
            for &in_t in &op.inputs {
                let produced_inside = dag.tensor_producer[in_t]
                    .map(|p| op_set.contains(&p))
                    .unwrap_or(false);
                if produced_inside || seen.contains(&in_t) {
                    continue;
                }
                seen.insert(in_t);
                if previously_retained.contains(&in_t) {
                    pre_retained.push(in_t);
                } else {
                    // Pointwise input slice = w * h, loaded once per spatial tile (no row-reuse)
                    pw_load.push((in_t, w * h));
                }
            }
            continue;
        }

        // MatMul op
        let lhs_idx = op.inputs[0];
        let rhs_idx = op.inputs[1];
        let out_t = op.outputs[0];

        // Is this op's output ephemeral within the subgraph?
        let output_ephemeral = !dag.graph_outputs.contains(&out_t)
            && !dag.tensor_consumers[out_t].is_empty()
            && dag.tensor_consumers[out_t].iter().all(|c| op_set.contains(c));

        // LHS input
        let lhs_boundary = !dag.tensor_producer[lhs_idx]
            .map(|p| op_set.contains(&p))
            .unwrap_or(false);
        if lhs_boundary && !seen.contains(&lhs_idx) {
            seen.insert(lhs_idx);
            if previously_retained.contains(&lhs_idx) {
                pre_retained.push(lhs_idx);
            } else if output_ephemeral {
                // Upstream LHS: we need a ROW STRIP = h rows * full K_full_Op0 width
                // h = output height of the FINAL output (the subgraph output)
                // K_full_Op0 = lhs.width (the full reduction dimension of this upstream op)
                let lhs_width = problem.tensors[lhs_idx].width;
                let row_strip_size = h * lhs_width;
                full_load.push((lhs_idx, row_strip_size));
            } else {
                // Standard LHS slice = h * k
                k_strip.push((lhs_idx, h * k));
            }
        }

        // RHS input
        let rhs_boundary = !dag.tensor_producer[rhs_idx]
            .map(|p| op_set.contains(&p))
            .unwrap_or(false);
        if rhs_boundary && !seen.contains(&rhs_idx) {
            seen.insert(rhs_idx);
            if previously_retained.contains(&rhs_idx) {
                pre_retained.push(rhs_idx);
            } else if output_ephemeral {
                // Upstream RHS: col strip of the intermediate = K_full_Op0 * k
                // = rhs.height * k (since rhs.height = K_full_Op0 for this upstream op)
                let rhs_height = problem.tensors[rhs_idx].height;
                k_strip.push((rhs_idx, rhs_height * k));
            } else {
                // Standard RHS slice = k * w
                k_strip.push((rhs_idx, k * w));
            }
        }
    }

    // Eviction: boundary outputs not in retain_after set
    let boundary_out = dag.boundary_outputs(problem, subgraph_ops);
    let out_evict_size: i64 = boundary_out
        .iter()
        .filter(|t| !retain_after.contains(t))
        .map(|_| w * h) // output slice = w * h regardless of op type
        .sum();

    StepMemoryPlan {
        full_load,
        k_strip,
        pw_load,
        out_evict_size,
        pre_retained,
    }
}

/// Compute num_k_steps for a subgraph: ceil(min_K_full / k) across all MatMuls.
/// Returns 1 if there are no MatMul ops.
pub fn compute_num_k_steps(
    subgraph_ops: &[usize],
    k: i64,
    problem: &Problem,
) -> i64 {
    let min_k_full: Option<i64> = subgraph_ops
        .iter()
        .filter_map(|&op_idx| {
            let op = &problem.ops[op_idx];
            if op.is_matmul() {
                Some(k_full_for_matmul(op, &problem.tensors))
            } else {
                None
            }
        })
        .min();
    if k <= 0 {
        return 1; // Guard against division by zero from malformed input
    }
    match min_k_full {
        Some(kf) => (kf + k - 1) / k,
        None => 1,
    }
}

/// Compute total subgraph latency using the roofline model.
pub fn subgraph_latency(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    tensors_to_retain: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> f64 {
    let (w_out, h_out) = dag.output_dimensions(problem, subgraph_ops);
    let w = granularity.w;
    let h = granularity.h;
    let k = granularity.k;

    // Spatial tiles
    let num_tiles_w = (w_out + w - 1) / w;
    let num_tiles_h = (h_out + h - 1) / h;
    let num_spatial_tiles = num_tiles_w * num_tiles_h;

    let num_k_steps = compute_num_k_steps(subgraph_ops, k, problem);

    // Split compute: MatMul cost is paid every k-step; Pointwise cost only on the last k-step.
    let matmul_compute = matmul_compute_per_step(subgraph_ops, granularity, problem);
    let pw_compute = pointwise_compute(subgraph_ops, problem);
    let bw = problem.slow_memory_bandwidth as f64;

    let plan = build_memory_plan(
        subgraph_ops,
        granularity,
        tensors_to_retain,
        previously_retained,
        problem,
        dag,
    );

    let full_load_total: i64 = plan.full_load.iter().map(|(_, sz)| sz).sum();
    let k_strip_total: i64 = plan.k_strip.iter().map(|(_, sz)| sz).sum();
    let pw_load_total: i64 = plan.pw_load.iter().map(|(_, sz)| sz).sum();

    // Identify which k-strip inputs are MatMul LHS (reused across columns in spatial tiling)
    // vs RHS (always reloaded). This matters for spatial tiling (num_k_steps = 1).
    //
    // For spatial tiling (k = K_full → num_k_steps = 1, multiple spatial tiles):
    //   LHS strip = h * k (identified by tile row)
    //   RHS strip = k * w (identified by tile col)
    //   In raster order: same row → reuse LHS strip, reload RHS strip
    //
    // For split-K (k < K_full → num_k_steps > 1):
    //   full_load inputs are reused across k-steps within a spatial tile
    //   k_strip inputs are loaded each k-step

    // Find LHS strips (for spatial reuse)
    let op_set: HashSet<usize> = subgraph_ops.iter().copied().collect();
    let mut lhs_strip_total: i64 = 0;
    let mut rhs_strip_total: i64 = 0;

    // The "final" MatMul's LHS boundary input contributes to lhs_strip_total.
    let final_matmul_lhs: HashSet<usize> = subgraph_ops
        .iter()
        .filter_map(|&op_idx| {
            let op = &problem.ops[op_idx];
            if !op.is_matmul() {
                return None;
            }
            let out_t = op.outputs[0];
            let is_boundary = dag.graph_outputs.contains(&out_t)
                || dag.tensor_consumers[out_t].iter().any(|c| !op_set.contains(c));
            if !is_boundary {
                return None;
            }
            let lhs_idx = op.inputs[0];
            let lhs_is_boundary = !dag.tensor_producer[lhs_idx]
                .map(|p| op_set.contains(&p))
                .unwrap_or(false);
            if lhs_is_boundary && !previously_retained.contains(&lhs_idx) {
                Some(lhs_idx)
            } else {
                None
            }
        })
        .collect();

    for (t, sz) in &plan.k_strip {
        if final_matmul_lhs.contains(t) {
            lhs_strip_total += sz;
        } else {
            rhs_strip_total += sz;
        }
    }

    // Closed-form raster-order latency. Custom traversal orders (snake) use
    // the tile-by-tile simulation in optimizer/traversal.rs::latency_with_traversal.
    if num_k_steps > 1 {
        // Split-K mode: all spatial tiles are identical.
        //
        // First k-step of each spatial tile:
        //   load = full_load_total + pw_load_total + k_strip_total
        //   compute = matmul_compute
        let first_k_mem = (full_load_total + pw_load_total + k_strip_total) as f64 / bw;
        let first_k_lat = f64::max(matmul_compute, first_k_mem);

        // Interior k-steps (steps 2 .. num_k_steps-1):
        //   load = k_strip_total only
        //   compute = matmul_compute
        let interior_k_lat = if num_k_steps > 2 {
            let interior_mem = k_strip_total as f64 / bw;
            f64::max(matmul_compute, interior_mem)
        } else {
            0.0
        };

        // Last k-step of each spatial tile:
        //   load = k_strip_total
        //   evict = out_evict_size
        //   compute = matmul_compute + pw_compute
        let last_k_mem = (k_strip_total + plan.out_evict_size) as f64 / bw;
        let last_k_lat = f64::max(matmul_compute + pw_compute, last_k_mem);

        // num_k_steps >= 2 here (outer branch guarantees it)
        let per_tile_lat = first_k_lat + (num_k_steps - 2).max(0) as f64 * interior_k_lat + last_k_lat;

        num_spatial_tiles as f64 * per_tile_lat
    } else {
        // Spatial-only mode (num_k_steps == 1): row-reuse pattern.
        // For each row of tiles (num_tiles_h rows, num_tiles_w columns):
        //   First column: load full_load_total + lhs_strip_total + rhs_strip_total + pw_load_total
        //   Other columns: load rhs_strip_total + pw_load_total
        // All tiles evict out_evict_size (last k-step == only step).
        //
        // compute = matmul_compute + pw_compute (both run on the single step)
        let compute = matmul_compute + pw_compute;

        let first_col_mem = (full_load_total + lhs_strip_total + rhs_strip_total + pw_load_total + plan.out_evict_size) as f64 / bw;
        let first_col_lat = f64::max(compute, first_col_mem);

        let other_col_mem = (rhs_strip_total + pw_load_total + plan.out_evict_size) as f64 / bw;
        let other_col_lat = f64::max(compute, other_col_mem);

        num_tiles_h as f64 * (first_col_lat + (num_tiles_w - 1).max(0) as f64 * other_col_lat)
    }
}

/// Compute boundary input slice sizes for a subgraph at a given granularity.
pub fn boundary_input_slice_sizes(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    problem: &Problem,
    dag: &DagInfo,
) -> Vec<(usize, i64)> {
    let op_set: HashSet<usize> = subgraph_ops.iter().copied().collect();
    let w = granularity.w;
    let h = granularity.h;
    let k = granularity.k;

    let mut result: Vec<(usize, i64)> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();

    for &op_idx in subgraph_ops {
        let op = &problem.ops[op_idx];
        if op.is_matmul() {
            let lhs = op.inputs[0];
            let rhs = op.inputs[1];
            if !dag.tensor_producer[lhs].map(|p| op_set.contains(&p)).unwrap_or(false)
                && !seen.contains(&lhs)
            {
                seen.insert(lhs);
                result.push((lhs, h * k));
            }
            if !dag.tensor_producer[rhs].map(|p| op_set.contains(&p)).unwrap_or(false)
                && !seen.contains(&rhs)
            {
                seen.insert(rhs);
                result.push((rhs, k * w));
            }
        } else {
            for &in_t in &op.inputs {
                if !dag.tensor_producer[in_t].map(|p| op_set.contains(&p)).unwrap_or(false)
                    && !seen.contains(&in_t)
                {
                    seen.insert(in_t);
                    result.push((in_t, w * h));
                }
            }
        }
    }
    result
}

pub fn boundary_output_slice_sizes(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    problem: &Problem,
    dag: &DagInfo,
) -> Vec<(usize, i64)> {
    let boundary_out = dag.boundary_outputs(problem, subgraph_ops);
    let w = granularity.w;
    let h = granularity.h;
    boundary_out.into_iter().map(|t| (t, w * h)).collect()
}
