# Error Catalog

This project is a CLI tool, not a web service. There are no HTTP status codes or API endpoints. This document catalogs the error conditions the scheduler can encounter and how they are reported.

---

## Error Categories

### Input Errors (returned during parsing / file I/O)

| Error Code | Rust Error Type | Message Pattern | When |
|------------|----------------|-----------------|------|
| `INPUT_FILE_NOT_FOUND` | `std::io::Error` | `Error reading input file '{path}': {io_error}` | CLI given nonexistent path |
| `INPUT_MALFORMED_JSON` | `serde_json::Error` | `Error parsing problem: {serde_error}` | Invalid JSON syntax |
| `INPUT_MISSING_FIELD` | `Result<T, String>` with descriptive message | `Missing required field: {field}` | JSON lacks a required key |
| `INPUT_DIMENSION_MISMATCH` | `Result<T, String>` with descriptive message | `widths has {n} entries but heights has {m}` | Array length inconsistency |
| `INPUT_UNKNOWN_OP_TYPE` | `Result<T, String>` with descriptive message | `Unknown op_type: {type}` | Op type is not MatMul or Pointwise |
| `INPUT_INVALID_TENSOR_REF` | `Result<T, String>` with descriptive message | `Op {k} references tensor {t} but only {n} tensors exist` | Tensor index out of range |

### DAG Errors (returned during graph analysis)

| Error Code | Rust Error Type | Message Pattern | When |
|------------|----------------|-----------------|------|
| `DAG_CYCLE_DETECTED` | `Result<DagInfo, String>` | `DAG has a cycle` | Topological sort fails (Kahn's algorithm detects unresolvable in-degrees) |
| `DAG_INVALID_TENSOR_REF` | `Result<DagInfo, String>` | `Op {k} references output tensor {t} but only {n} tensors exist` | Tensor index out of range during DAG build |

### Scheduling Errors (handled internally by optimizer)

| Error Code | Handling | Message Pattern | When |
|------------|----------|-----------------|------|
| `SCHEDULE_OOM` | Emergency OOM fix stage reduces granularity; no panic | Logged to stderr if no valid granularity found | Working set exceeds fast_memory_capacity for all candidates |
| `SCHEDULE_UNCOVERED_OP` | Internal invariant; pipeline guarantees coverage | N/A | Should not occur for valid problems |
| `SCHEDULE_INVALID_TRAVERSAL` | Traversal module skips invalid orders silently | N/A | Bad traversal order produced by traversal optimizer |

### Validation Errors (evaluate subcommand)

| Error Code | Rust Error Type | Message Pattern | When |
|------------|----------------|-----------------|------|
| `EVAL_LATENCY_MISMATCH` | `Result<T, String>` with descriptive message | Printed via `println!("FAIL: {}")` | Reported latency does not match recalculated latency |
| `EVAL_SOLUTION_FILE_INVALID` | `serde_json::Error` | `Error parsing solution: {serde_error}` | Solution JSON missing or malformed |
| `EVAL_OOM_VIOLATION` | `Result<T, String>` with descriptive message | `FAIL: {detail}` | Subgraph working set exceeds capacity |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success: solution produced and valid |
| 1 | Any error: file not found, malformed JSON, invalid problem, cyclic DAG, or evaluation failure |

All errors are reported via `eprintln!` to stderr, then `std::process::exit(1)`. The evaluate subcommand prints `PASS` or `FAIL: {reason}` to stdout and exits with code 1 on failure.
