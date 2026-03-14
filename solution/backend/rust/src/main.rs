mod baseline;
mod dag;
mod evaluate;
mod latency;
mod memory;
mod models;
mod optimizer;
mod parser;
mod serializer;

use std::env;
use std::fs;

use dag::DagInfo;
use evaluate::evaluate;
use optimizer::pipeline::run_pipeline;
use parser::{parse_problem, parse_solution};
use serializer::serialize_solution;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("evaluate") => run_evaluate(&args),
        _ => run_solve(&args),
    }
}

/// Solve mode: ./mlsys <input.json> <output.json>
fn run_solve(args: &[String]) {
    if args.len() != 3 {
        eprintln!("Usage: {} <input.json> <output.json>", args[0]);
        eprintln!("       {} evaluate --problem <input.json> --solution <solution.json>", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    let input_str = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Error reading input file '{}': {}", input_path, e);
        std::process::exit(1);
    });

    let problem = parse_problem(&input_str).unwrap_or_else(|e| {
        eprintln!("Error parsing problem: {}", e);
        std::process::exit(1);
    });

    let dag = DagInfo::build(&problem).unwrap_or_else(|e| {
        eprintln!("Error building DAG: {}", e);
        std::process::exit(1);
    });

    let solution = run_pipeline(&problem, &dag);

    let total: f64 = solution.subgraphs.iter().map(|sg| sg.subgraph_latency).sum();
    eprintln!(
        "Solution: {} subgraphs, total latency = {:.2}",
        solution.subgraphs.len(),
        total
    );

    let output_str = serialize_solution(&solution).unwrap_or_else(|e| {
        eprintln!("Error serializing solution: {}", e);
        std::process::exit(1);
    });

    fs::write(output_path, &output_str).unwrap_or_else(|e| {
        eprintln!("Error writing output file '{}': {}", output_path, e);
        std::process::exit(1);
    });

    eprintln!("Solution written to {}", output_path);
}

/// Evaluate mode: ./mlsys evaluate --problem <input.json> --solution <solution.json>
fn run_evaluate(args: &[String]) {
    let mut problem_path: Option<&str> = None;
    let mut solution_path: Option<&str> = None;

    let mut i = 2; // skip binary name and "evaluate"
    while i < args.len() {
        match args[i].as_str() {
            "--problem" if i + 1 < args.len() => {
                problem_path = Some(&args[i + 1]);
                i += 2;
            }
            "--solution" if i + 1 < args.len() => {
                solution_path = Some(&args[i + 1]);
                i += 2;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
    }

    let problem_path = problem_path.unwrap_or_else(|| {
        eprintln!("Usage: {} evaluate --problem <input.json> --solution <solution.json>", args[0]);
        std::process::exit(1);
    });
    let solution_path = solution_path.unwrap_or_else(|| {
        eprintln!("Usage: {} evaluate --problem <input.json> --solution <solution.json>", args[0]);
        std::process::exit(1);
    });

    let problem_str = fs::read_to_string(problem_path).unwrap_or_else(|e| {
        eprintln!("Error reading problem file '{}': {}", problem_path, e);
        std::process::exit(1);
    });
    let solution_str = fs::read_to_string(solution_path).unwrap_or_else(|e| {
        eprintln!("Error reading solution file '{}': {}", solution_path, e);
        std::process::exit(1);
    });

    let problem = parse_problem(&problem_str).unwrap_or_else(|e| {
        eprintln!("Error parsing problem: {}", e);
        std::process::exit(1);
    });
    let solution = parse_solution(&solution_str).unwrap_or_else(|e| {
        eprintln!("Error parsing solution: {}", e);
        std::process::exit(1);
    });

    match evaluate(&problem, &solution) {
        Ok(result) => {
            println!("PASS");
            println!("Total latency: {:.2}", result.total_latency);
            for (i, lat) in result.subgraph_latencies.iter().enumerate() {
                println!("  Subgraph {}: {:.2}", i, lat);
            }
        }
        Err(e) => {
            println!("FAIL: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::latency::subgraph_latency;
    use crate::models::Granularity;
    use std::collections::HashSet;

    fn load_problem(json: &str) -> (models::Problem, DagInfo) {
        let problem = parse_problem(json).unwrap();
        let dag = DagInfo::build(&problem).unwrap();
        (problem, dag)
    }

    // ===== Example 1: Baseline =====

    const EX1_JSON: &str = r#"{
        "widths": [128,128,128],
        "heights": [128,128,128],
        "inputs": [[0], [1]],
        "outputs": [[1], [2]],
        "base_costs": [1000, 100],
        "op_types": ["Pointwise","Pointwise"],
        "fast_memory_capacity": 35000,
        "slow_memory_bandwidth": 10,
        "native_granularity": [128, 128]
    }"#;

    #[test]
    fn test_ex1_strategy_a_two_subgraphs() {
        // Strategy A: [[0],[1]], both at native 128x128
        // Expected: 3276.8 each
        let (problem, dag) = load_problem(EX1_JSON);
        let retained = HashSet::new();
        let gran = Granularity { w: 128, h: 128, k: 1 };

        let lat0 = subgraph_latency(&[0], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat0 - 3276.8).abs() < 0.5,
            "Subgraph 0 latency: got {lat0}, expected 3276.8"
        );

        let lat1 = subgraph_latency(&[1], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat1 - 3276.8).abs() < 0.5,
            "Subgraph 1 latency: got {lat1}, expected 3276.8"
        );
    }

    #[test]
    fn test_ex1_strategy_b_fused_128x128() {
        // Strategy B: [[0,1]] at 128x128, Expected: 3276.8
        let (problem, dag) = load_problem(EX1_JSON);
        let retained = HashSet::new();
        let gran = Granularity { w: 128, h: 128, k: 1 };

        let lat = subgraph_latency(&[0, 1], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat - 3276.8).abs() < 0.5,
            "Fused latency: got {lat}, expected 3276.8"
        );
    }

    #[test]
    fn test_ex1_strategy_c_fused_64x64() {
        // Strategy C: [[0,1]] at 64x64, Expected: 4400.0
        let (problem, dag) = load_problem(EX1_JSON);
        let retained = HashSet::new();
        let gran = Granularity { w: 64, h: 64, k: 1 };

        let lat = subgraph_latency(&[0, 1], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat - 4400.0).abs() < 0.5,
            "64x64 fused latency: got {lat}, expected 4400.0"
        );
    }

    // ===== Example 2: Larger Tensors =====

    const EX2_JSON: &str = r#"{
        "widths": [256,256,256],
        "heights": [256,256,256],
        "inputs": [[0], [1]],
        "outputs": [[1], [2]],
        "base_costs": [1000, 100],
        "op_types": ["Pointwise","Pointwise"],
        "fast_memory_capacity": 35000,
        "slow_memory_bandwidth": 10,
        "native_granularity": [128, 128]
    }"#;

    #[test]
    fn test_ex2_strategy_a() {
        // Strategy A: [[0]], native 128x128, 4 tiles, Expected: 13107.2
        let (problem, dag) = load_problem(EX2_JSON);
        let retained = HashSet::new();
        let gran = Granularity { w: 128, h: 128, k: 1 };

        let lat0 = subgraph_latency(&[0], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat0 - 13107.2).abs() < 0.5,
            "Subgraph 0 latency: got {lat0}, expected 13107.2"
        );
    }

    #[test]
    fn test_ex2_strategy_b_fused_128x128() {
        // Strategy B: [[0,1]] at 128x128, Expected: 13107.2
        let (problem, dag) = load_problem(EX2_JSON);
        let retained = HashSet::new();
        let gran = Granularity { w: 128, h: 128, k: 1 };

        let lat = subgraph_latency(&[0, 1], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat - 13107.2).abs() < 0.5,
            "Fused 128x128 latency: got {lat}, expected 13107.2"
        );
    }

    // ===== Example 3: Diamond Graph =====

    const EX3_JSON: &str = r#"{
        "widths": [128,128,128,128],
        "heights": [128,128,128,128],
        "inputs": [[0],[1],[1,2]],
        "outputs": [[1],[2],[3]],
        "base_costs": [1500,1500,1500],
        "op_types": ["Pointwise","Pointwise","Pointwise"],
        "fast_memory_capacity": 50000,
        "slow_memory_bandwidth": 10,
        "native_granularity": [128, 128]
    }"#;

    #[test]
    fn test_ex3_strategy_a_spilling() {
        // Strategy A: [[0],[1],[2]], native 128x128
        // Expected: 3276.8 + 3276.8 + 4915.2
        let (problem, dag) = load_problem(EX3_JSON);
        let gran = Granularity { w: 128, h: 128, k: 1 };
        let retained = HashSet::new();

        let lat0 = subgraph_latency(&[0], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat0 - 3276.8).abs() < 0.5,
            "Subgraph 0: got {lat0}, expected 3276.8"
        );

        let lat1 = subgraph_latency(&[1], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat1 - 3276.8).abs() < 0.5,
            "Subgraph 1: got {lat1}, expected 3276.8"
        );

        let lat2 = subgraph_latency(&[2], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat2 - 4915.2).abs() < 0.5,
            "Subgraph 2: got {lat2}, expected 4915.2"
        );
    }

    #[test]
    fn test_ex3_strategy_c_selective_residency() {
        // Strategy C: [[0],[1,2]], tensor 1 retained after subgraph 0
        // Subgraph 0: 1638.4, Subgraph 1: 3000.0
        let (problem, dag) = load_problem(EX3_JSON);
        let gran = Granularity { w: 128, h: 128, k: 1 };
        let retained = HashSet::new();

        let lat0 = subgraph_latency(&[0], &gran, &[1], &retained, &problem, &dag);
        assert!(
            (lat0 - 1638.4).abs() < 0.5,
            "Subgraph 0 (retain T1): got {lat0}, expected 1638.4"
        );

        let retained1: HashSet<usize> = vec![1].into_iter().collect();
        let lat1 = subgraph_latency(&[1, 2], &gran, &[], &retained1, &problem, &dag);
        assert!(
            (lat1 - 3000.0).abs() < 0.5,
            "Subgraph 1 (T1 resident): got {lat1}, expected 3000.0"
        );
    }

    // ===== Example 4: MatMul with Spatial Tiling =====

    const EX4_JSON: &str = r#"{
        "widths": [128,128,128],
        "heights": [128,128,128],
        "inputs": [[0,1]],
        "outputs": [[2]],
        "base_costs": [1500],
        "op_types": ["MatMul"],
        "fast_memory_capacity": 25000,
        "slow_memory_bandwidth": 10,
        "native_granularity": [128, 128]
    }"#;

    #[test]
    fn test_ex4_strategy_a_naive_tiling() {
        // Strategy A: [[0]] at 64x64x128, raster order, Expected: 7096
        let (problem, dag) = load_problem(EX4_JSON);
        let gran = Granularity { w: 64, h: 64, k: 128 };
        let retained = HashSet::new();

        let lat = subgraph_latency(&[0], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat - 7096.0).abs() < 1.0,
            "MatMul naive tiling: got {lat}, expected 7096"
        );
    }

    // ===== Example 5: Chained MatMul (Split-K) =====

    const EX5_JSON: &str = r#"{
        "widths": [128,128,128,128,128],
        "heights": [128,128,128,128,128],
        "inputs": [[0,1], [3,2]],
        "outputs": [[3], [4]],
        "base_costs": [2000, 2000],
        "op_types": ["MatMul", "MatMul"],
        "fast_memory_capacity": 45000,
        "slow_memory_bandwidth": 10,
        "native_granularity": [128, 128]
    }"#;

    #[test]
    fn test_ex5_strategy_b_splitk() {
        // Strategy B: [[0,1]] at 128x128x32, Expected: 6915.2
        let (problem, dag) = load_problem(EX5_JSON);
        let gran = Granularity { w: 128, h: 128, k: 32 };
        let retained = HashSet::new();

        let lat = subgraph_latency(&[0, 1], &gran, &[], &retained, &problem, &dag);
        assert!(
            (lat - 6915.2).abs() < 1.0,
            "Split-K latency: got {lat}, expected 6915.2"
        );
    }

    // ===== Edge Cases =====

    // Single op, single tile (1x1 tensor) — minimal compute and memory
    const SINGLE_OP_TINY_JSON: &str = r#"{
        "widths": [1,1],
        "heights": [1,1],
        "inputs": [[0]],
        "outputs": [[1]],
        "base_costs": [500],
        "op_types": ["Pointwise"],
        "fast_memory_capacity": 10000,
        "slow_memory_bandwidth": 10,
        "native_granularity": [1, 1]
    }"#;

    #[test]
    fn test_edge_single_op_tiny_tensor() {
        // Single pointwise op on a 1x1 tensor: latency = max(500, 2/10) = 500
        let (problem, dag) = load_problem(SINGLE_OP_TINY_JSON);
        let gran = Granularity { w: 1, h: 1, k: 1 };
        let retained = HashSet::new();
        let lat = subgraph_latency(&[0], &gran, &[], &retained, &problem, &dag);
        // compute=500, mem=2/10=0.2 => max=500
        assert!(
            (lat - 500.0).abs() < 0.5,
            "Tiny single op latency: got {lat}, expected 500.0"
        );
    }

    // OOM detection: working set > capacity should be detected
    const OOM_JSON: &str = r#"{
        "widths": [128,128,128,128,128],
        "heights": [128,128,128,128,128],
        "inputs": [[0,1]],
        "outputs": [[2]],
        "base_costs": [1000],
        "op_types": ["MatMul"],
        "fast_memory_capacity": 100,
        "slow_memory_bandwidth": 10,
        "native_granularity": [128, 128]
    }"#;

    #[test]
    fn test_edge_oom_detection() {
        use crate::memory::check_oom;
        let (problem, dag) = load_problem(OOM_JSON);
        // At native 128x128 with 100 elements capacity, this should OOM
        let gran = Granularity { w: 128, h: 128, k: 128 };
        let retained = HashSet::new();
        let fits = check_oom(&[0], &gran, &[], &retained, &problem, &dag);
        assert!(!fits, "Expected OOM for capacity=100, but check_oom returned true");
    }

    // Serialization round-trip: parse → serialize → parse produces same structure
    #[test]
    fn test_edge_serialization_roundtrip() {
        use crate::serializer::serialize_solution;
        use crate::models::{Solution, SubgraphDef};

        let (problem, dag) = load_problem(EX1_JSON);
        let gran = Granularity { w: 128, h: 128, k: 1 };
        let retained = HashSet::new();
        let lat = subgraph_latency(&[0], &gran, &[], &retained, &problem, &dag);

        let solution = Solution {
            subgraphs: vec![
                SubgraphDef {
                    ops: vec![0],
                    granularity: gran.clone(),
                    tensors_to_retain: vec![],
                    traversal_order: None,
                    subgraph_latency: lat,
                },
                SubgraphDef {
                    ops: vec![1],
                    granularity: gran.clone(),
                    tensors_to_retain: vec![],
                    traversal_order: None,
                    subgraph_latency: lat,
                },
            ],
        };

        // Serialize and verify it's valid JSON with expected fields
        let json_str = serialize_solution(&solution).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let subgraphs = parsed["subgraphs"].as_array().unwrap();
        assert_eq!(subgraphs.len(), 2, "Expected 2 subgraphs after serialization");
        assert_eq!(subgraphs[0][0].as_u64().unwrap(), 0, "First subgraph should contain op 0");
        assert_eq!(subgraphs[1][0].as_u64().unwrap(), 1, "Second subgraph should contain op 1");

        let lats = parsed["subgraph_latencies"].as_array().unwrap();
        assert_eq!(lats.len(), 2, "Expected 2 latency entries");
        assert!((lats[0].as_f64().unwrap() - lat).abs() < 0.01, "Latency round-trip mismatch");

        let grans = parsed["granularities"].as_array().unwrap();
        assert_eq!(grans[0][0].as_i64().unwrap(), 128, "Granularity w mismatch after roundtrip");
        assert_eq!(grans[0][1].as_i64().unwrap(), 128, "Granularity h mismatch after roundtrip");
        assert_eq!(grans[0][2].as_i64().unwrap(), 1, "Granularity k mismatch after roundtrip");
    }

    // Fusion correctness: fused subgraph's ephemeral tensors should not appear in boundary outputs
    #[test]
    fn test_edge_fusion_ephemeral_correctness() {
        // Ex5: [[0,1]] fused — tensor 3 is ephemeral (Op0 output, Op1 input)
        let (problem, dag) = load_problem(EX5_JSON);
        // Boundary outputs of the fused subgraph [0,1]
        let boundary_outs = dag.boundary_outputs(&problem, &[0, 1]);
        // Tensor 3 is ephemeral (produced by Op0, consumed by Op1), must NOT appear
        assert!(
            !boundary_outs.contains(&3),
            "Ephemeral tensor 3 should not be a boundary output of fused [0,1]"
        );
        // Tensor 4 is the final output — must appear
        assert!(
            boundary_outs.contains(&4),
            "Final output tensor 4 must be a boundary output of fused [0,1]"
        );
    }

    // DAG parse error: cyclic graph should return an error
    const CYCLIC_JSON: &str = r#"{
        "widths": [128,128],
        "heights": [128,128],
        "inputs": [[1],[0]],
        "outputs": [[0],[1]],
        "base_costs": [100, 100],
        "op_types": ["Pointwise","Pointwise"],
        "fast_memory_capacity": 10000,
        "slow_memory_bandwidth": 10,
        "native_granularity": [128, 128]
    }"#;

    #[test]
    fn test_edge_cyclic_dag_rejected() {
        let problem = parse_problem(CYCLIC_JSON).unwrap();
        let dag_result = DagInfo::build(&problem);
        assert!(dag_result.is_err(), "Cyclic DAG should be rejected by DagInfo::build");
        assert!(
            dag_result.unwrap_err().contains("cycle"),
            "Error message should mention 'cycle'"
        );
    }

    // Fused MatMul+Pointwise with split-K: Pointwise runs once per spatial tile (last k-step only)
    #[test]
    fn test_fused_matmul_pointwise_splitk() {
        // MatMul(Op0): Tensor0(128x128) @ Tensor1(128x128) -> Tensor2(128x128), cost=2000
        // Pointwise(Op1): Tensor2 -> Tensor3(128x128), cost=500
        // Fused subgraph [0,1] with k=32 (K_full=128, so 4 k-steps)
        let json = r#"{
            "widths": [128,128,128,128],
            "heights": [128,128,128,128],
            "inputs": [[0,1],[2]],
            "outputs": [[2],[3]],
            "base_costs": [2000, 500],
            "op_types": ["MatMul","Pointwise"],
            "fast_memory_capacity": 50000,
            "slow_memory_bandwidth": 10,
            "native_granularity": [128, 128]
        }"#;
        let problem = parse_problem(json).unwrap();
        let dag = DagInfo::build(&problem).unwrap();

        let gran = Granularity { w: 128, h: 128, k: 32 };
        let lat = subgraph_latency(&[0, 1], &gran, &[], &HashSet::new(), &problem, &dag);

        // 4 k-steps (128/32), 1 spatial tile
        // Steps 1-3 (non-last): compute = MatMul only = 2000*(32/128) = 500
        // Step 4 (last): compute = 500 + Pointwise 500 = 1000
        // Memory: step 1 loads Tensor0 (128*128=16384) + Tensor1 strip (128*32=4096) = 20480/10 = 2048
        //   Pointwise input (Tensor2) is ephemeral within subgraph — no load cost
        // Step 2-3: Tensor1 strip reload (4096/10 = 409.6), Tensor0 reused
        // Step 4: Tensor1 strip (409.6) + evict Tensor3 (128*128/10 = 1638.4) = 2048
        //
        // Step 1: max(500, 2048) = 2048
        // Step 2: max(500, 409.6) = 500
        // Step 3: max(500, 409.6) = 500
        // Step 4: max(1000, 2048) = 2048
        // Total = 2048 + 500 + 500 + 2048 = 5096

        // Verify Pointwise cost appears only once (not 4 times)
        // If PW ran every step: total compute would be 500+500=1000 per step * 4 = different
        assert!(
            (lat - 5096.0).abs() < 1.0,
            "Fused MatMul+PW split-K latency should be 5096, got {:.1}",
            lat
        );

        // Also verify: with k=128 (no split-K, 1 step), latency should be different
        let gran_full = Granularity { w: 128, h: 128, k: 128 };
        let lat_full = subgraph_latency(&[0, 1], &gran_full, &[], &HashSet::new(), &problem, &dag);

        // 1 k-step: compute = 2000 + 500 = 2500 (both ops run)
        // Memory: Tensor0 (16384/10=1638.4) + Tensor1 (128*128/10=1638.4) + evict Tensor3 (1638.4)
        // Total mem = 4915.2
        // max(2500, 4915.2) = 4915.2
        assert!(
            (lat_full - 4915.2).abs() < 1.0,
            "Full-k fused MatMul+PW latency should be 4915.2, got {:.1}",
            lat_full
        );
    }

    // Fused MatMul+Pointwise with split-K where Pointwise consumes a boundary bias tensor.
    // This exercises the pw_load path: the bias tensor is a graph input (boundary), not ephemeral.
    #[test]
    fn test_fused_matmul_pointwise_splitk_boundary_pw_input() {
        // MatMul(Op0): Tensor0(128x128) @ Tensor1(128x128) -> Tensor2(128x128), cost=2000
        // Pointwise(Op1): [Tensor2, Tensor3_bias] -> Tensor4(128x128), cost=500
        //   Tensor3 is a graph input (bias), so it is a boundary input to the subgraph.
        // Fused subgraph [0,1] with k=32 (K_full=128, so 4 k-steps)
        let json = r#"{
            "widths": [128,128,128,128,128],
            "heights": [128,128,128,128,128],
            "inputs": [[0,1],[2,3]],
            "outputs": [[2],[4]],
            "base_costs": [2000, 500],
            "op_types": ["MatMul","Pointwise"],
            "fast_memory_capacity": 50000,
            "slow_memory_bandwidth": 10,
            "native_granularity": [128, 128]
        }"#;
        let problem = parse_problem(json).unwrap();
        let dag = DagInfo::build(&problem).unwrap();

        let gran = Granularity { w: 128, h: 128, k: 32 };
        let lat = subgraph_latency(&[0, 1], &gran, &[], &HashSet::new(), &problem, &dag);

        // 4 k-steps (128/32), 1 spatial tile
        // pw_load path: Tensor3 (bias, 128*128=16384 elements) is a boundary Pointwise input.
        // It is loaded once per spatial tile on the first k-step (pw_load).
        //
        // Step 1 (first k): load Tensor0 full (16384) + Tensor1 strip (4096) + Tensor3 bias (16384)
        //   = (16384 + 4096 + 16384) / 10 = 36864/10 = 3686.4
        //   compute = 2000*(32/128) = 500
        //   step latency = max(500, 3686.4) = 3686.4
        // Step 2: load Tensor1 strip (4096/10=409.6), compute=500
        //   step latency = max(500, 409.6) = 500
        // Step 3: same as step 2 => 500
        // Step 4 (last k): load Tensor1 strip (409.6) + evict Tensor4 (16384/10=1638.4)
        //   mem = (4096 + 16384)/10 = 2048
        //   compute = 500 + 500 (Pointwise last k) = 1000
        //   step latency = max(1000, 2048) = 2048
        // Total = 3686.4 + 500 + 500 + 2048 = 6734.4

        assert!(
            (lat - 6734.4).abs() < 1.0,
            "Fused MatMul+PW split-K with boundary bias: expected 6734.4, got {:.1}",
            lat
        );
    }

    // Benchmark validation: all 5 benchmark solutions should cover all ops and have non-negative latencies
    #[test]
    fn test_benchmark_solutions_validity() {
        use crate::optimizer::pipeline::run_pipeline;
        use crate::serializer::serialize_solution;

        let bench_jsons = [
            include_str!("../../../../problem/benchmarks/mlsys-2026-1.json"),
            include_str!("../../../../problem/benchmarks/mlsys-2026-5.json"),
            include_str!("../../../../problem/benchmarks/mlsys-2026-9.json"),
            include_str!("../../../../problem/benchmarks/mlsys-2026-13.json"),
            include_str!("../../../../problem/benchmarks/mlsys-2026-17.json"),
        ];

        for (idx, json) in bench_jsons.iter().enumerate() {
            let problem = parse_problem(json).unwrap_or_else(|e| {
                panic!("Benchmark {} parse error: {}", idx, e)
            });
            let dag = DagInfo::build(&problem).unwrap_or_else(|e| {
                panic!("Benchmark {} DAG error: {}", idx, e)
            });

            let solution = run_pipeline(&problem, &dag);

            // Verify all ops covered (no duplicates, full coverage)
            let num_ops = problem.ops.len();
            let mut op_covered = vec![false; num_ops];
            for sg in &solution.subgraphs {
                for &op_idx in &sg.ops {
                    assert!(op_idx < num_ops, "Benchmark {}: op index {} out of range", idx, op_idx);
                    op_covered[op_idx] = true;
                }
            }
            for (op_i, &covered) in op_covered.iter().enumerate() {
                assert!(covered, "Benchmark {}: op {} not covered", idx, op_i);
            }

            // Verify latencies non-negative
            for sg in &solution.subgraphs {
                assert!(
                    sg.subgraph_latency >= 0.0,
                    "Benchmark {}: negative latency {}", idx, sg.subgraph_latency
                );
            }

            // Verify serialization works
            serialize_solution(&solution).unwrap_or_else(|e| {
                panic!("Benchmark {} serialization error: {}", idx, e)
            });
        }
    }
}
