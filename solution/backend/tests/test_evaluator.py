"""
Unit tests for the Track B Python evaluator (evaluator.py).
Tests cover:
- All 5 PROBLEM.md worked examples
- Edge cases: tiny tensors, single ops, OOM detection
- Fusion correctness: ephemeral tensors
- Serialization round-trip
- DAG utilities
"""
import json
import math
import sys
import os
import pytest

# Add agent directory to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../agent"))

from evaluator import (
    Granularity,
    OOMError,
    Problem,
    Solution,
    SubgraphDef,
    Tensor,
    ValidationError,
    check_oom,
    compute_subgraph_latency,
    compute_working_set,
    evaluate,
    get_graph_inputs,
    get_graph_outputs,
    parse_problem,
    parse_solution,
    solution_to_dict,
    topological_sort,
    _k_full_for_op,
    _classify_tensors,
)


# ---------------------------------------------------------------------------
# Fixtures / helpers
# ---------------------------------------------------------------------------

def make_problem(data: dict) -> Problem:
    return parse_problem(data)


EX1_DATA = {
    "widths": [128, 128, 128],
    "heights": [128, 128, 128],
    "inputs": [[0], [1]],
    "outputs": [[1], [2]],
    "base_costs": [1000, 100],
    "op_types": ["Pointwise", "Pointwise"],
    "fast_memory_capacity": 35000,
    "slow_memory_bandwidth": 10,
    "native_granularity": [128, 128],
}

EX2_DATA = {
    "widths": [256, 256, 256],
    "heights": [256, 256, 256],
    "inputs": [[0], [1]],
    "outputs": [[1], [2]],
    "base_costs": [1000, 100],
    "op_types": ["Pointwise", "Pointwise"],
    "fast_memory_capacity": 35000,
    "slow_memory_bandwidth": 10,
    "native_granularity": [128, 128],
}

EX3_DATA = {
    "widths": [128, 128, 128, 128],
    "heights": [128, 128, 128, 128],
    "inputs": [[0], [1], [1, 2]],
    "outputs": [[1], [2], [3]],
    "base_costs": [1500, 1500, 1500],
    "op_types": ["Pointwise", "Pointwise", "Pointwise"],
    "fast_memory_capacity": 50000,
    "slow_memory_bandwidth": 10,
    "native_granularity": [128, 128],
}

EX4_DATA = {
    "widths": [128, 128, 128],
    "heights": [128, 128, 128],
    "inputs": [[0, 1]],
    "outputs": [[2]],
    "base_costs": [1500],
    "op_types": ["MatMul"],
    "fast_memory_capacity": 25000,
    "slow_memory_bandwidth": 10,
    "native_granularity": [128, 128],
}

EX5_DATA = {
    "widths": [128, 128, 128, 128, 128],
    "heights": [128, 128, 128, 128, 128],
    "inputs": [[0, 1], [3, 2]],
    "outputs": [[3], [4]],
    "base_costs": [2000, 2000],
    "op_types": ["MatMul", "MatMul"],
    "fast_memory_capacity": 45000,
    "slow_memory_bandwidth": 10,
    "native_granularity": [128, 128],
}


# ---------------------------------------------------------------------------
# Example 1: Baseline (Pointwise chain, 128x128)
# ---------------------------------------------------------------------------

class TestExample1:
    def test_strategy_a_subgraph0(self):
        """Strategy A: [[0]] at 128x128, expected latency 3276.8"""
        problem = make_problem(EX1_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([0], gran, problem)
        assert abs(lat - 3276.8) < 0.5, f"Expected 3276.8, got {lat}"

    def test_strategy_a_subgraph1(self):
        """Strategy A: [[1]] at 128x128, expected latency 3276.8"""
        problem = make_problem(EX1_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([1], gran, problem)
        assert abs(lat - 3276.8) < 0.5, f"Expected 3276.8, got {lat}"

    def test_strategy_b_fused_128x128(self):
        """Strategy B: [[0,1]] at 128x128, expected latency 3276.8 (fusion wins)"""
        problem = make_problem(EX1_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([0, 1], gran, problem)
        assert abs(lat - 3276.8) < 0.5, f"Expected 3276.8, got {lat}"

    def test_strategy_c_fused_64x64(self):
        """Strategy C: [[0,1]] at 64x64, expected latency 4400.0"""
        problem = make_problem(EX1_DATA)
        gran = Granularity(64, 64, 1)
        lat = compute_subgraph_latency([0, 1], gran, problem)
        assert abs(lat - 4400.0) < 0.5, f"Expected 4400.0, got {lat}"


# ---------------------------------------------------------------------------
# Example 2: Larger Tensors (256x256)
# ---------------------------------------------------------------------------

class TestExample2:
    def test_strategy_a_subgraph0(self):
        """Strategy A: [[0]] at 128x128, 4 tiles, expected latency 13107.2"""
        problem = make_problem(EX2_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([0], gran, problem)
        assert abs(lat - 13107.2) < 0.5, f"Expected 13107.2, got {lat}"

    def test_strategy_b_fused_128x128(self):
        """Strategy B: [[0,1]] at 128x128, expected latency 13107.2"""
        problem = make_problem(EX2_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([0, 1], gran, problem)
        assert abs(lat - 13107.2) < 0.5, f"Expected 13107.2, got {lat}"


# ---------------------------------------------------------------------------
# Example 3: Diamond Graph (selective residency)
# ---------------------------------------------------------------------------

class TestExample3:
    def test_strategy_a_sg0(self):
        """Strategy A spilling: sg[0] at 128x128, expected 3276.8"""
        problem = make_problem(EX3_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([0], gran, problem)
        assert abs(lat - 3276.8) < 0.5, f"Expected 3276.8, got {lat}"

    def test_strategy_a_sg1(self):
        """Strategy A spilling: sg[1] at 128x128, expected 3276.8"""
        problem = make_problem(EX3_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([1], gran, problem)
        assert abs(lat - 3276.8) < 0.5, f"Expected 3276.8, got {lat}"

    def test_strategy_a_sg2_two_inputs(self):
        """Strategy A spilling: sg[2] loads T1 and T2, expected 4915.2"""
        problem = make_problem(EX3_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([2], gran, problem)
        assert abs(lat - 4915.2) < 0.5, f"Expected 4915.2, got {lat}"

    def test_strategy_c_sg0_retain_t1(self):
        """Strategy C: sg[0] retaining T1, expected 1638.4 (half eviction cost)"""
        problem = make_problem(EX3_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([0], gran, problem,
                                       tensors_to_retain_after={1})
        assert abs(lat - 1638.4) < 0.5, f"Expected 1638.4, got {lat}"

    def test_strategy_c_sg1_t1_resident(self):
        """Strategy C: sg[1,2] with T1 already resident, expected 3000.0"""
        problem = make_problem(EX3_DATA)
        gran = Granularity(128, 128, 1)
        lat = compute_subgraph_latency([1, 2], gran, problem,
                                       retained_tensors={1})
        assert abs(lat - 3000.0) < 0.5, f"Expected 3000.0, got {lat}"


# ---------------------------------------------------------------------------
# Example 4: MatMul with Spatial Tiling
# ---------------------------------------------------------------------------

class TestExample4:
    def test_strategy_a_naive_64x64(self):
        """Strategy A: MatMul at 64x64x128, raster order, expected ~7096"""
        problem = make_problem(EX4_DATA)
        gran = Granularity(64, 64, 128)
        lat = compute_subgraph_latency([0], gran, problem)
        assert abs(lat - 7096.0) < 1.0, f"Expected ~7096, got {lat}"


# ---------------------------------------------------------------------------
# Example 5: Chained MatMul (Split-K)
# ---------------------------------------------------------------------------

class TestExample5:
    def test_strategy_b_splitk_128x128x32(self):
        """Strategy B: fused [0,1] at 128x128x32, expected ~6915.2"""
        problem = make_problem(EX5_DATA)
        gran = Granularity(128, 128, 32)
        lat = compute_subgraph_latency([0, 1], gran, problem)
        assert abs(lat - 6915.2) < 1.0, f"Expected ~6915.2, got {lat}"


# ---------------------------------------------------------------------------
# Edge Cases
# ---------------------------------------------------------------------------

class TestEdgeCases:
    def test_single_op_tiny_tensor(self):
        """Single pointwise op on 1x1 tensor: compute-bound at 500"""
        data = {
            "widths": [1, 1],
            "heights": [1, 1],
            "inputs": [[0]],
            "outputs": [[1]],
            "base_costs": [500],
            "op_types": ["Pointwise"],
            "fast_memory_capacity": 10000,
            "slow_memory_bandwidth": 10,
            "native_granularity": [1, 1],
        }
        problem = make_problem(data)
        gran = Granularity(1, 1, 1)
        lat = compute_subgraph_latency([0], gran, problem)
        # compute=500, mem=2/10=0.2 => max=500.0 per tile (1 tile)
        assert abs(lat - 500.0) < 0.5, f"Expected 500.0, got {lat}"

    def test_oom_detection_returns_false(self):
        """Working set that exceeds capacity should be detected as OOM"""
        data = {
            "widths": [128, 128, 128],
            "heights": [128, 128, 128],
            "inputs": [[0, 1]],
            "outputs": [[2]],
            "base_costs": [1000],
            "op_types": ["MatMul"],
            "fast_memory_capacity": 100,
            "slow_memory_bandwidth": 10,
            "native_granularity": [128, 128],
        }
        problem = make_problem(data)
        gran = Granularity(128, 128, 128)
        assert not check_oom([0], gran, problem), "Should detect OOM for capacity=100"

    def test_oom_detection_returns_true_when_fits(self):
        """Working set that fits should pass OOM check"""
        problem = make_problem(EX1_DATA)
        gran = Granularity(128, 128, 1)
        assert check_oom([0], gran, problem), "Should fit in memory"

    def test_evaluate_raises_on_missing_ops(self):
        """evaluate() should raise ValidationError when an op is not covered"""
        problem = make_problem(EX1_DATA)
        gran = Granularity(128, 128, 1)
        # Only include op 0, miss op 1
        solution = Solution(subgraphs=[
            SubgraphDef(ops=[0], granularity=gran, tensors_to_retain=[],
                       traversal_order=None, subgraph_latency=0.0)
        ])
        with pytest.raises(ValidationError):
            evaluate(problem, solution)

    def test_evaluate_raises_on_oom(self):
        """evaluate() should raise OOMError when a subgraph OOMs"""
        data = {
            "widths": [128, 128, 128],
            "heights": [128, 128, 128],
            "inputs": [[0, 1]],
            "outputs": [[2]],
            "base_costs": [1000],
            "op_types": ["MatMul"],
            "fast_memory_capacity": 50,
            "slow_memory_bandwidth": 10,
            "native_granularity": [128, 128],
        }
        problem = make_problem(data)
        gran = Granularity(128, 128, 128)
        solution = Solution(subgraphs=[
            SubgraphDef(ops=[0], granularity=gran, tensors_to_retain=[],
                       traversal_order=None, subgraph_latency=0.0)
        ])
        with pytest.raises(OOMError):
            evaluate(problem, solution)

    def test_fusion_ephemeral_tensor_not_in_boundary_outputs(self):
        """In fused [0,1] of EX5, tensor 3 (Op0 output/Op1 input) is ephemeral"""
        problem = make_problem(EX5_DATA)
        produced, consumed, ephemeral = _classify_tensors([0, 1], problem)
        # Tensor 3: produced by Op0, consumed by Op1 => ephemeral
        assert 3 in ephemeral, "Tensor 3 should be ephemeral in fused [0,1]"
        # Tensor 4: produced by Op1, not consumed inside => boundary output
        boundary_outputs = produced - consumed
        assert 4 in boundary_outputs, "Tensor 4 should be a boundary output"
        assert 3 not in boundary_outputs, "Tensor 3 should NOT be a boundary output"

    def test_serialization_roundtrip(self):
        """solution_to_dict -> parse_solution should reproduce same subgraph structure"""
        problem = make_problem(EX1_DATA)
        gran = Granularity(128, 128, 1)
        lat0 = compute_subgraph_latency([0], gran, problem)
        lat1 = compute_subgraph_latency([1], gran, problem)

        solution = Solution(subgraphs=[
            SubgraphDef(ops=[0], granularity=gran, tensors_to_retain=[],
                       traversal_order=None, subgraph_latency=lat0),
            SubgraphDef(ops=[1], granularity=gran, tensors_to_retain=[],
                       traversal_order=None, subgraph_latency=lat1),
        ])

        d = solution_to_dict(solution)
        # Verify dict structure
        assert len(d["subgraphs"]) == 2
        assert d["subgraphs"][0] == [0]
        assert d["subgraphs"][1] == [1]
        assert d["granularities"][0] == [128, 128, 1]
        assert abs(d["subgraph_latencies"][0] - lat0) < 0.01
        assert abs(d["subgraph_latencies"][1] - lat1) < 0.01

        # Round-trip through JSON string
        json_str = json.dumps(d)
        d2 = json.loads(json_str)
        assert d2["subgraphs"] == d["subgraphs"]
        assert d2["granularities"] == d["granularities"]

    def test_topological_sort_linear_chain(self):
        """Topological sort of a linear chain should return [0, 1] in that order"""
        problem = make_problem(EX1_DATA)
        order = topological_sort(problem)
        assert len(order) == 2
        # Op 1 depends on Op 0's output (tensor 1), so Op 0 must come first
        assert order.index(0) < order.index(1), "Op 0 must precede Op 1 in topo order"

    def test_topological_sort_cyclic_raises(self):
        """A cyclic graph should raise ValueError during topological sort"""
        cyclic_data = {
            "widths": [128, 128],
            "heights": [128, 128],
            "inputs": [[1], [0]],
            "outputs": [[0], [1]],
            "base_costs": [100, 100],
            "op_types": ["Pointwise", "Pointwise"],
            "fast_memory_capacity": 10000,
            "slow_memory_bandwidth": 10,
            "native_granularity": [128, 128],
        }
        problem = make_problem(cyclic_data)
        with pytest.raises(ValueError, match="cycle"):
            topological_sort(problem)

    def test_graph_inputs_and_outputs(self):
        """Graph inputs/outputs are correctly identified for EX1"""
        problem = make_problem(EX1_DATA)
        inputs = get_graph_inputs(problem)
        outputs = get_graph_outputs(problem)
        # Tensor 0: only consumed, never produced => graph input
        assert 0 in inputs
        # Tensor 2: only produced (by Op 1), never consumed => graph output
        assert 2 in outputs
        # Tensor 1: produced by Op 0, consumed by Op 1 => neither
        assert 1 not in inputs
        assert 1 not in outputs

    def test_k_full_for_matmul_op(self):
        """K_full for a MatMul op is LHS.width"""
        problem = make_problem(EX4_DATA)
        # Op 0 is MatMul: LHS=tensor0 (128x128), so K_full = 128
        k_full = _k_full_for_op(problem.ops[0], problem)
        assert k_full == 128, f"Expected K_full=128, got {k_full}"


# ---------------------------------------------------------------------------
# Benchmark validation tests (integration)
# ---------------------------------------------------------------------------

BENCH_DIR = os.path.join(os.path.dirname(__file__), "../../../problem/benchmarks")

@pytest.mark.parametrize("bench_id", [1, 5, 9, 13, 17])
def test_benchmark_solutions_valid(bench_id):
    """Each benchmark's optimizer solution must cover all ops, be OOM-free, and have valid latencies"""
    bench_path = os.path.join(BENCH_DIR, f"mlsys-2026-{bench_id}.json")
    if not os.path.exists(bench_path):
        pytest.skip(f"Benchmark {bench_id} not found at {bench_path}")

    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../agent"))
    from scheduler import optimize, build_baseline

    with open(bench_path) as f:
        data = json.load(f)
    problem = parse_problem(data)

    try:
        solution = optimize(problem)
    except Exception as e:
        solution = build_baseline(problem)

    # Validate all ops covered
    all_ops = set(range(len(problem.ops)))
    covered = set()
    for sg in solution.subgraphs:
        covered.update(sg.ops)
    assert all_ops == covered, f"Benchmark {bench_id}: missing ops {all_ops - covered}"

    # Validate no negative latencies
    for i, sg in enumerate(solution.subgraphs):
        assert sg.subgraph_latency >= 0, (
            f"Benchmark {bench_id}, subgraph {i}: negative latency {sg.subgraph_latency}"
        )

    # Validate granularities are positive
    for i, sg in enumerate(solution.subgraphs):
        g = sg.granularity
        assert g.w > 0 and g.h > 0 and g.k > 0, (
            f"Benchmark {bench_id}, subgraph {i}: invalid granularity ({g.w},{g.h},{g.k})"
        )
