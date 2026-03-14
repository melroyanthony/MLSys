# Error Catalog

This project is a CLI tool, not a web service. There are no HTTP status codes or API endpoints. This document catalogs the error conditions the scheduler can encounter and how they are reported.

---

## Error Categories

### Input Errors (raised during parsing)

| Error Code | Exception | Message Pattern | When |
|------------|-----------|-----------------|------|
| `INPUT_FILE_NOT_FOUND` | `FileNotFoundError` | `Problem file not found: {path}` | CLI given nonexistent path |
| `INPUT_MALFORMED_JSON` | `json.JSONDecodeError` | `Malformed JSON in problem file: {detail}` | Invalid JSON syntax |
| `INPUT_MISSING_FIELD` | `ValueError` | `Missing required field: {field}` | JSON lacks a required key |
| `INPUT_DIMENSION_MISMATCH` | `ValueError` | `widths has {n} entries but heights has {m}` | Array length inconsistency |
| `INPUT_UNKNOWN_OP_TYPE` | `ValueError` | `Unknown op_type: {type}` | Op type is not MatMul or Pointwise |
| `INPUT_INVALID_TENSOR_REF` | `ValueError` | `Op {k} references tensor {t} but only {n} tensors exist` | Tensor index out of range |

### DAG Errors (raised during graph analysis)

| Error Code | Exception | Message Pattern | When |
|------------|-----------|-----------------|------|
| `DAG_CYCLE_DETECTED` | `ValueError` | `Graph contains a cycle involving op {k}` | Topological sort fails |
| `DAG_DISCONNECTED_OP` | `ValueError` | `Op {k} has no input tensors and no output tensors` | Orphan operation |

### Scheduling Errors (raised during optimization)

| Error Code | Exception | Message Pattern | When |
|------------|-----------|-----------------|------|
| `SCHEDULE_OOM` | `RuntimeError` | `Subgraph {i} exceeds fast memory: working_set={ws} > capacity={cap}` | No valid granularity found |
| `SCHEDULE_UNCOVERED_OP` | `RuntimeError` | `Op {k} is not covered by any subgraph` | Internal logic error |
| `SCHEDULE_INVALID_TRAVERSAL` | `ValueError` | `Traversal order for subgraph {i} is not a valid permutation of [0, {n})` | Bad traversal order |

### Validation Errors (raised during solution evaluation)

| Error Code | Exception | Message Pattern | When |
|------------|-----------|-----------------|------|
| `EVAL_LATENCY_MISMATCH` | `ValueError` | `Subgraph {i}: reported latency {reported} != calculated {calculated}` | Self-check failed |
| `EVAL_SOLUTION_FILE_INVALID` | `ValueError` | `Solution JSON missing required field: {field}` | Bad solution file |
| `EVAL_DIMENSION_MISMATCH` | `ValueError` | `Subgraph {i} granularity w={w} does not divide output width={W}` | Invalid granularity |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success: solution produced and valid |
| 1 | Input error: file not found, malformed JSON, or invalid problem |
| 2 | Scheduling error: OOM or uncovered ops (should not happen for valid problems) |
| 3 | Validation error: solution failed self-check |
