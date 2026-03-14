# ADR-001: Python as Implementation Language

## Status
Accepted

## Context
The MLSys DAG Scheduler is a contest submission that must read a problem JSON, compute an optimized execution schedule, and produce a solution JSON. The reference implementation and evaluator are in C++ (`mlsys.h`), but the contest allows any language for the solver.

Key considerations:
- **Development speed**: Contest is time-constrained (multi-hour to multi-day hackathon)
- **Complexity**: The scheduler involves graph algorithms, combinatorial search, and floating-point arithmetic -- not low-level systems programming
- **Data sizes**: At most 96 ops and 160 tensors. The scheduler performs arithmetic on small integers and floats, not heavy numerical computation
- **Evaluator**: The C++ `Evaluate()` function is the ground truth. We need our Python latency model to match it exactly, but we do not need to call C++ from Python

## Decision
Implement the scheduler in **Python 3.12+** using:
- `dataclasses` for typed data structures mirroring `mlsys.h`
- Standard library only (no NumPy, no external dependencies beyond `pytest` for testing)
- `uv` for package management per project conventions

## Consequences

### Positive
- **Rapid prototyping**: Python's expressiveness allows faster iteration on algorithm design
- **Type safety**: Type hints + `dataclasses` provide structure without boilerplate
- **Debugging**: Easy to print intermediate states, use interactive debugging
- **Testing**: `pytest` ecosystem is mature, parameterized tests are trivial
- **No build step**: Run directly, no compilation needed

### Negative
- **Performance ceiling**: Python is ~50-100x slower than C++ for tight loops. If the granularity search space is large, this could matter for benchmark 17 (96 ops)
- **Floating-point behavior**: Python `float` is IEEE 754 double (same as C++ `double`), but operator ordering differences could cause small divergences from `Evaluate()`

### Mitigations
- Profile early on benchmark 17. If runtime exceeds 5 minutes, limit the search space (powers-of-2 only, skip clearly suboptimal candidates)
- Validate all 5 PROBLEM.md examples against known latency values to catch any float divergence
- Pure Python (no NumPy) avoids dependency complexity and keeps the codebase simple

### Neutral
- Standard choice for algorithm contests and prototyping
- Team familiarity assumed
