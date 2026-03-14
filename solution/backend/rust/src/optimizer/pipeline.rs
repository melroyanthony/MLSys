/// Pipeline: orchestrates all optimizer stages in sequence.
///
/// Stages:
/// 1. Baseline (one op per subgraph, native granularity)
/// 2. Greedy chain fusion (merge adjacent ops)
/// 3. Retention pass 1 (decide what to keep resident)
/// 4. Split-K (handle OOM from large k)
/// 5. Granularity search (find best w, h, k per subgraph)
/// 6. Retention pass 2 (re-evaluate after granularity changes)
/// 7. Emergency OOM fix (reduce granularity for any remaining OOM)
/// 8. Final latency recalculation
/// 9. Traversal optimization (snake order for MatMul data reuse)

use std::collections::HashSet;

use crate::baseline::build_baseline;
use crate::dag::DagInfo;
use crate::latency::subgraph_latency;
use crate::memory::check_oom;
use crate::models::{Granularity, Problem, Solution, SubgraphDef};
use crate::optimizer::fusion::greedy_fusion;
use crate::optimizer::granularity::optimize_granularities;
use crate::optimizer::retention::optimize_retention;
use crate::optimizer::splitk::{apply_splitk, build_retained_sets};
use crate::optimizer::traversal::optimize_traversals;

pub fn run_pipeline(problem: &Problem, dag: &DagInfo) -> Solution {
    // Stage 1: Baseline
    let baseline = build_baseline(problem, dag);
    let mut subgraphs = baseline.subgraphs;

    // Stage 2: Greedy fusion (multiple passes until no more merges)
    let no_retained: Vec<HashSet<usize>> = vec![HashSet::new(); subgraphs.len()];
    subgraphs = greedy_fusion(problem, dag, &subgraphs, &no_retained);

    // After fusion, ensure all subgraphs have valid granularities
    // (greedy_fusion assigns native granularity for each merged group)

    // Stage 3: Retention (first pass, before split-K and granularity search)
    optimize_retention(&mut subgraphs, problem, dag);

    // Stage 4: Split-K (for OOM subgraphs)
    let retained_sets = build_retained_sets(&subgraphs);
    apply_splitk(&mut subgraphs, problem, dag, &retained_sets);

    // Stage 5: Granularity search (find best w, h, k per subgraph)
    // Reset retention first, then search, then redo retention
    for sg in &mut subgraphs {
        sg.tensors_to_retain = vec![];
    }
    optimize_granularities(&mut subgraphs, problem, dag);

    // Stage 6: Retention (second pass, after granularity is finalized)
    optimize_retention(&mut subgraphs, problem, dag);

    // Stage 7: Emergency OOM fixes - if any subgraph still OOMs, reduce to smallest granularity
    emergency_oom_fix(&mut subgraphs, problem, dag);

    // Stage 8: Final latency recalculation
    recalculate_latencies(&mut subgraphs, problem, dag);

    // Stage 9: Traversal optimization (snake/zig-zag order for spatial-tiled MatMul subgraphs)
    optimize_traversals(&mut subgraphs, problem, dag);

    Solution { subgraphs }
}

/// Fix any remaining OOM issues by reducing granularity to smallest possible.
fn emergency_oom_fix(
    subgraphs: &mut Vec<SubgraphDef>,
    problem: &Problem,
    dag: &DagInfo,
) {
    let mut previously_retained: HashSet<usize> = HashSet::new();

    for sg in subgraphs.iter_mut() {
        if !check_oom(
            &sg.ops,
            &sg.granularity,
            &sg.tensors_to_retain,
            &previously_retained,
            problem,
            dag,
        ) {
            // Try to find smallest feasible granularity
            let (native_w, native_h) = problem.native_granularity;
            let (w_out, h_out) = dag.output_dimensions(problem, &sg.ops);

            let mut found = false;
            // Try powers of 2 downward
            let mut w = native_w;
            while w >= 1 {
                let mut h = native_h;
                while h >= 1 {
                    let trial = Granularity { w, h, k: sg.granularity.k };
                    if check_oom(
                        &sg.ops,
                        &trial,
                        &sg.tensors_to_retain,
                        &previously_retained,
                        problem,
                        dag,
                    ) {
                        sg.granularity = trial;
                        found = true;
                        break;
                    }
                    h /= 2;
                }
                if found {
                    break;
                }
                w /= 2;
            }

            // If still not fixed, try k=1
            if !found {
                let trial = Granularity { w: 1, h: 1, k: 1 };
                if check_oom(
                    &sg.ops,
                    &trial,
                    &sg.tensors_to_retain,
                    &previously_retained,
                    problem,
                    dag,
                ) {
                    sg.granularity = trial;
                }
            }
        }

        previously_retained = sg.tensors_to_retain.iter().copied().collect();
    }
}

/// Recompute subgraph_latency for all subgraphs using the final granularities.
fn recalculate_latencies(
    subgraphs: &mut Vec<SubgraphDef>,
    problem: &Problem,
    dag: &DagInfo,
) {
    let mut previously_retained: HashSet<usize> = HashSet::new();

    for sg in subgraphs.iter_mut() {
        sg.subgraph_latency = subgraph_latency(
            &sg.ops,
            &sg.granularity,
            &sg.tensors_to_retain,
            &previously_retained,
            problem,
            dag,
        );

        previously_retained = sg.tensors_to_retain.iter().copied().collect();
    }
}
