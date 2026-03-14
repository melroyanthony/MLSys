/// Traversal-order optimizer: generates snake/zig-zag tile sequences for MatMul subgraphs.
///
/// Snake traversal alternates row direction each row:
///   Row 0: left → right  (tiles 0, 1, 2, ...)
///   Row 1: right → left  (tiles ..., 2, 1, 0 for that row)
///   Row 2: left → right  etc.
///
/// This reduces bandwidth by reusing both the LHS row strip across each row AND
/// the RHS column strip at row transitions (the last column visited on row N is
/// the first column visited on row N+1 in snake order).

use std::collections::HashSet;

use crate::dag::DagInfo;
use crate::latency::{compute_num_k_steps, compute_time_per_step, build_memory_plan_pub};
use crate::models::{Granularity, Problem, SubgraphDef};

/// Generate snake (zig-zag) traversal order for a grid of (num_tiles_w × num_tiles_h) tiles.
///
/// Tile index in raster order: row * num_tiles_w + col
/// Snake: even rows go left→right, odd rows go right→left.
pub fn snake_order(num_tiles_w: i64, num_tiles_h: i64) -> Vec<i64> {
    let mut order = Vec::with_capacity((num_tiles_w * num_tiles_h) as usize);
    for row in 0..num_tiles_h {
        if row % 2 == 0 {
            // Left to right
            for col in 0..num_tiles_w {
                order.push(row * num_tiles_w + col);
            }
        } else {
            // Right to left
            for col in (0..num_tiles_w).rev() {
                order.push(row * num_tiles_w + col);
            }
        }
    }
    order
}

/// Compute subgraph latency with a given traversal order (spatial tiling only, no split-K).
///
/// For spatial tiling (num_k_steps == 1), the traversal order determines which LHS row strip
/// and RHS column strip are resident at each step:
/// - LHS row strip (height-strip) is resident while we stay in the same grid row.
/// - RHS column strip (width-strip) is resident while we stay in the same grid column.
///
/// With snake order, at each row boundary the last column's RHS strip from the previous row
/// is still resident, so the first tile of the new row reuses it if it's the same column.
pub fn latency_with_traversal(
    subgraph_ops: &[usize],
    granularity: &Granularity,
    tensors_to_retain: &[usize],
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
    traversal: &[i64],
) -> f64 {
    let (w_out, _h_out) = dag.output_dimensions(problem, subgraph_ops);
    let w = granularity.w;

    let num_tiles_w = (w_out + w - 1) / w;

    // This function only applies to spatial-only tiling (k == k_full → 1 k-step).
    // Split-K subgraphs don't benefit from traversal reordering.
    let num_k_steps = compute_num_k_steps(subgraph_ops, granularity.k, problem);
    if num_k_steps != 1 {
        // Fall back to standard raster latency for split-K subgraphs.
        return crate::latency::subgraph_latency(
            subgraph_ops,
            granularity,
            tensors_to_retain,
            previously_retained,
            problem,
            dag,
        );
    }

    let compute_step = compute_time_per_step(subgraph_ops, granularity, problem, dag);
    let bw = problem.slow_memory_bandwidth as f64;

    let plan = build_memory_plan_pub(
        subgraph_ops,
        granularity,
        tensors_to_retain,
        previously_retained,
        problem,
        dag,
    );

    // Identify LHS strips vs RHS strips (for spatial reuse accounting).
    let op_set: HashSet<usize> = subgraph_ops.iter().copied().collect();
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

    let full_load_total: i64 = plan.full_load.iter().map(|(_, sz)| sz).sum();
    let mut lhs_strip_total: i64 = 0;
    let mut rhs_strip_total: i64 = 0;
    for (t, sz) in &plan.k_strip {
        if final_matmul_lhs.contains(t) {
            lhs_strip_total += sz;
        } else {
            rhs_strip_total += sz;
        }
    }

    let out_evict_size = plan.out_evict_size;

    // Simulate traversal order, tracking which row/col is currently resident.
    let mut prev_row: Option<i64> = None;
    let mut prev_col: Option<i64> = None;
    let mut total_latency: f64 = 0.0;

    for &tile_idx in traversal {
        let tile_row = tile_idx / num_tiles_w;
        let tile_col = tile_idx % num_tiles_w;

        let same_row = prev_row == Some(tile_row);
        let same_col = prev_col == Some(tile_col);

        let mut load: i64 = 0;

        // full_load inputs (upstream row strips in chained matmul): reuse if same row.
        if !same_row {
            load += full_load_total;
        }

        // LHS row strip: reuse if same row.
        if !same_row {
            load += lhs_strip_total;
        }

        // RHS column strip: reuse if same column.
        if !same_col {
            load += rhs_strip_total;
        }

        // Evict output slice.
        let evict = out_evict_size;

        let mem_time = (load + evict) as f64 / bw;
        total_latency += f64::max(compute_step, mem_time);

        prev_row = Some(tile_row);
        prev_col = Some(tile_col);
    }

    total_latency
}

/// Decide whether to apply snake traversal to a subgraph.
///
/// Conditions for applying snake traversal:
/// 1. Subgraph has at least one MatMul op.
/// 2. Spatial tiling only (num_k_steps == 1, meaning k == k_full).
/// 3. Grid has more than one row AND more than one column (otherwise snake == raster).
///
/// Returns the traversal order if snake is better than raster, or None to keep raster.
pub fn optimize_traversal(
    sg: &SubgraphDef,
    previously_retained: &HashSet<usize>,
    problem: &Problem,
    dag: &DagInfo,
) -> Option<Vec<i64>> {
    let subgraph_ops = &sg.ops;
    let granularity = &sg.granularity;

    // Only applies to subgraphs with MatMul ops.
    let has_matmul = subgraph_ops.iter().any(|&i| problem.ops[i].is_matmul());
    if !has_matmul {
        return None;
    }

    let num_k_steps = compute_num_k_steps(subgraph_ops, granularity.k, problem);

    // Only useful for spatial-only tiling (no split-K).
    if num_k_steps != 1 {
        return None;
    }

    let (w_out, h_out) = dag.output_dimensions(problem, subgraph_ops);
    let num_tiles_w = (w_out + granularity.w - 1) / granularity.w;
    let num_tiles_h = (h_out + granularity.h - 1) / granularity.h;

    // Snake is identical to raster when there's only one row or one column.
    if num_tiles_w <= 1 || num_tiles_h <= 1 {
        return None;
    }

    let snake = snake_order(num_tiles_w, num_tiles_h);

    let snake_lat = latency_with_traversal(
        subgraph_ops,
        granularity,
        &sg.tensors_to_retain,
        previously_retained,
        problem,
        dag,
        &snake,
    );

    let raster_lat = crate::latency::subgraph_latency(
        subgraph_ops,
        granularity,
        &sg.tensors_to_retain,
        previously_retained,
        problem,
        dag,
    );

    if snake_lat < raster_lat {
        Some(snake)
    } else {
        None
    }
}

/// Apply traversal optimization to all subgraphs in sequence.
pub fn optimize_traversals(
    subgraphs: &mut Vec<SubgraphDef>,
    problem: &Problem,
    dag: &DagInfo,
) {
    let mut previously_retained: HashSet<usize> = HashSet::new();

    for sg in subgraphs.iter_mut() {
        if let Some(order) = optimize_traversal(sg, &previously_retained, problem, dag) {
            // Recompute latency with the snake order.
            sg.subgraph_latency = latency_with_traversal(
                &sg.ops,
                &sg.granularity,
                &sg.tensors_to_retain,
                &previously_retained,
                problem,
                dag,
                &order,
            );
            sg.traversal_order = Some(order);
        }

        previously_retained = sg.tensors_to_retain.iter().copied().collect();
    }
}
