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
use optimizer::pipeline::run_pipeline;
use parser::parse_problem;
use serializer::serialize_solution;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.json> <output.json>", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    // Read and parse input
    let input_str = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Error reading input file '{}': {}", input_path, e);
        std::process::exit(1);
    });

    let problem = parse_problem(&input_str).unwrap_or_else(|e| {
        eprintln!("Error parsing problem: {}", e);
        std::process::exit(1);
    });

    // Build DAG
    let dag = DagInfo::build(&problem).unwrap_or_else(|e| {
        eprintln!("Error building DAG: {}", e);
        std::process::exit(1);
    });

    // Run optimization pipeline
    let solution = run_pipeline(&problem, &dag);

    // Calculate and report total latency
    let total: f64 = solution.subgraphs.iter().map(|sg| sg.subgraph_latency).sum();
    eprintln!(
        "Solution: {} subgraphs, total latency = {:.2}",
        solution.subgraphs.len(),
        total
    );

    // Serialize and write output
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
}
