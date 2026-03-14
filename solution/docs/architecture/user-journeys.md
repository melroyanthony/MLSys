# User Journeys

## Journey 1: Generate Optimized Schedule

### Actor: Contest Participant / Researcher
### Goal: Produce a valid, latency-minimized schedule JSON for a given problem

```mermaid
journey
    title Generate Optimized Schedule
    section Setup
      Install Python dependencies: 5: User
      Locate problem JSON file: 5: User
    section Execution
      Run scheduler CLI: 5: User
      Parser validates input: 4: System
      Baseline schedule generated: 4: System
      Optimizer refines schedule: 3: System
      Solution JSON written: 5: System
    section Validation
      Review reported latency: 4: User
      Submit to evaluator: 5: User
```

### Steps

| Step | Action | System Response | CLI Command / Module | Acceptance Criteria |
|------|--------|----------------|---------------------|---------------------|
| 1 | User runs `python -m mlsys_scheduler solve --problem path/to/problem.json --output path/to/solution.json` | CLI parses arguments | `cli.py` | Validates file exists and is readable |
| 2 | System parses problem JSON | Constructs `Problem` with tensors, ops, hardware params | `parser.py` | All fields populated, tensor/op counts match JSON arrays |
| 3 | System builds DAG and topological sort | Produces `DAGInfo` with adjacency, topo order, graph I/O | `dag.py` | DAG is valid (no cycles), all ops reachable |
| 4 | System generates baseline schedule | One subgraph per op, native granularity, no retention | `baseline.py` | Valid schedule, no OOM, all ops covered |
| 5 | System runs optimizer pipeline | Fusion, retention, split-K, granularity search | `optimizer/pipeline.py` | Latency <= baseline latency |
| 6 | System calculates final latencies | `subgraph_latencies` populated | `latency.py` | Matches roofline model to float precision |
| 7 | System writes solution JSON | Well-formed JSON on disk | `serializer.py` | JSON round-trips through validator |
| 8 | System prints summary to stdout | Total latency, subgraph count, improvement vs baseline | `cli.py` | Human-readable summary |

### Error Scenarios

- **File not found**: CLI prints error message and exits with code 1
- **Malformed JSON**: Parser raises `ValueError` with line/field information
- **Cyclic graph**: DAG module raises `ValueError("Graph contains a cycle")`
- **All granularities OOM for a subgraph**: Fallback to smallest possible granularity; if still OOM, raise `RuntimeError` (should not happen for valid problems)

---

## Journey 2: Validate Solution Against Evaluator

### Actor: Contest Participant
### Goal: Verify that a solution JSON is valid and check its latency

### Steps

| Step | Action | System Response | CLI Command | Acceptance Criteria |
|------|--------|----------------|-------------|---------------------|
| 1 | User runs `python -m mlsys_scheduler evaluate --problem problem.json --solution solution.json` | CLI loads both files | `cli.py` | Both files parsed successfully |
| 2 | System re-evaluates solution latency | Computes per-subgraph and total latency | `latency.py` | Latency matches `subgraph_latencies` in solution JSON |
| 3 | System checks constraints | OOM check, all-ops-covered check, valid traversal orders | `memory.py`, `dag.py` | Zero violations |
| 4 | System prints report | Total latency, per-subgraph breakdown, pass/fail | `cli.py` | Clear pass/fail with detail on any failures |

### Error Scenarios

- **OOM violation**: Reports which subgraph exceeds capacity, by how much
- **Missing ops**: Reports which op indices are not covered
- **Latency mismatch**: Reports expected vs actual for each mismatched subgraph

---

## Journey 3: Run All Benchmarks

### Actor: Contest Participant
### Goal: Produce and validate solutions for all 5 benchmark files

### Steps

| Step | Action | System Response | CLI Command | Acceptance Criteria |
|------|--------|----------------|-------------|---------------------|
| 1 | User runs `python -m mlsys_scheduler batch --benchmark-dir problem/benchmarks/ --output-dir solution/outputs/` | CLI discovers all `mlsys-2026-*.json` files | `cli.py` | Finds all 5 benchmarks |
| 2 | System processes each benchmark | Runs full pipeline per benchmark | `optimizer/pipeline.py` | Solution JSON written for each |
| 3 | System prints summary table | Per-benchmark latency, improvement vs baseline | `cli.py` | All 5 pass validation |

### Edge Cases

- **Benchmark directory missing**: Error with helpful message
- **One benchmark fails while others succeed**: Continue processing remaining; report failures at the end
- **Timeout on large benchmark**: Print partial result and warning (benchmark 17 may take longer)
