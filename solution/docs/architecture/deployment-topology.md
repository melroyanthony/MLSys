# Deployment Topology

This is a local CLI tool. There is no server, container, or cloud deployment.

## Development Environment

```
Developer Machine
    |
    +-- Python 3.12+ (managed by uv)
    |
    +-- Project Root
        |
        +-- problem/                    # Input: problem JSONs and benchmarks
        |   +-- PROBLEM.md
        |   +-- example_problem.json
        |   +-- mlsys.h
        |   +-- benchmarks/
        |       +-- mlsys-2026-{1,5,9,13,17}.json
        |
        +-- solution/
            +-- src/
            |   +-- mlsys_scheduler/    # Python package
            |       +-- cli.py
            |       +-- models.py
            |       +-- parser.py
            |       +-- serializer.py
            |       +-- dag.py
            |       +-- latency.py
            |       +-- memory.py
            |       +-- baseline.py
            |       +-- optimizer/
            |           +-- pipeline.py
            |           +-- fusion.py
            |           +-- retention.py
            |           +-- splitk.py
            |           +-- granularity.py
            |           +-- traversal.py
            |
            +-- tests/                  # pytest test suite
            |   +-- test_parser.py
            |   +-- test_latency.py
            |   +-- test_memory.py
            |   +-- test_baseline.py
            |   +-- test_examples.py    # Regression tests against PROBLEM.md
            |   +-- test_optimizer.py
            |
            +-- outputs/                # Generated solution JSONs
            |   +-- mlsys-2026-1-solution.json
            |   +-- mlsys-2026-5-solution.json
            |   +-- ...
            |
            +-- pyproject.toml          # Project metadata, dependencies
```

## Running the Scheduler

```bash
# Install dependencies
cd solution
uv sync

# Run on a single problem
uv run python -m mlsys_scheduler solve \
    --problem ../problem/benchmarks/mlsys-2026-1.json \
    --output outputs/mlsys-2026-1-solution.json

# Evaluate a solution
uv run python -m mlsys_scheduler evaluate \
    --problem ../problem/benchmarks/mlsys-2026-1.json \
    --solution outputs/mlsys-2026-1-solution.json

# Run all benchmarks
uv run python -m mlsys_scheduler batch \
    --benchmark-dir ../problem/benchmarks/ \
    --output-dir outputs/

# Run tests
uv run pytest tests/ -v
```

## Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| Python | >= 3.12 | Runtime |
| pytest | >= 8.0 | Test framework |

No other dependencies. The scheduler is pure Python with no external library requirements.

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `MLSYS_LOG_LEVEL` | `INFO` | Logging verbosity (DEBUG, INFO, WARNING, ERROR) |
| `MLSYS_TIMEOUT` | `300` | Max seconds per benchmark (safety limit) |

## "Production" Context

In the contest context, "production" means:
1. Solutions are submitted as JSON files
2. The contest infrastructure runs the C++ `Evaluate()` function on our JSON output
3. Our Python scheduler runs locally on the contestant's machine only
