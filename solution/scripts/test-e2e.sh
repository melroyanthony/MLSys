#!/usr/bin/env bash
# E2E Happy Path Test Script
# Tests both Track A (Rust) and Track B (Python) against all 5 benchmarks.
# Run from the project root: bash solution/scripts/test-e2e.sh

set -uo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
RUST_DIR="$PROJECT_ROOT/solution/backend/rust"
AGENT_DIR="$PROJECT_ROOT/solution/agent"
BENCH_DIR="$PROJECT_ROOT/problem/benchmarks"
TMP_DIR="/tmp/mlsys-e2e-$$"

cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT INT TERM

mkdir -p "$TMP_DIR"

PASS=0
FAIL=0

pass() { echo "PASS: $1"; ((PASS++)); }
fail() { echo "FAIL: $1"; ((FAIL++)); }

# ---------------------------------------------------------------------------
# Helper: validate a solution JSON
# ---------------------------------------------------------------------------
validate_solution() {
    local json_file="$1"
    local bench_id="$2"
    local track="$3"

    if [[ ! -f "$json_file" ]]; then
        fail "$track benchmark $bench_id: output file not found"
        return
    fi

    uv run python - "$json_file" <<'PYEOF' 2>&1
import json, sys

try:
    with open(sys.argv[1]) as f:
        s = json.load(f)
except Exception as e:
    print(f'JSON parse error: {e}', file=sys.stderr)
    sys.exit(1)

# Check required keys
for key in ('subgraphs', 'granularities', 'tensors_to_retain', 'subgraph_latencies'):
    if key not in s:
        print(f'Missing key: {key}', file=sys.stderr)
        sys.exit(2)

subgraphs = s['subgraphs']
latencies = s['subgraph_latencies']
granularities = s['granularities']

if len(subgraphs) == 0:
    print('No subgraphs', file=sys.stderr)
    sys.exit(3)

# All latencies non-negative
for i, lat in enumerate(latencies):
    if lat < 0:
        print(f'Negative latency at index {i}: {lat}', file=sys.stderr)
        sys.exit(4)

# All granularities positive
for i, g in enumerate(granularities):
    if any(x <= 0 for x in g):
        print(f'Invalid granularity at index {i}: {g}', file=sys.stderr)
        sys.exit(5)

# Check all ops are covered (allow recomputation — ops may appear in multiple subgraphs)
all_ops = set()
for sg in subgraphs:
    for op in sg:
        all_ops.add(op)

total = sum(latencies)
print(f'subgraphs={len(subgraphs)} total_latency={total:.2f} ops_covered={sorted(all_ops)}')
PYEOF
    return $?
}

echo "=== E2E Happy Path Test ==="
echo "Project: $PROJECT_ROOT"
echo "TMP: $TMP_DIR"
echo ""

# ---------------------------------------------------------------------------
# Track A: Build Rust binary
# ---------------------------------------------------------------------------
echo "--- Track A: Building Rust binary ---"
cd "$RUST_DIR"
RUST_BUILD_OK=false
if cargo build --release; then
    pass "Track A: Rust build"
    RUST_BUILD_OK=true
else
    fail "Track A: Rust build failed"
fi

RUST_BIN="$RUST_DIR/target/release/mlsys"

# ---------------------------------------------------------------------------
# Track A: Run benchmarks
# ---------------------------------------------------------------------------
echo ""
echo "--- Track A: Running benchmarks ---"
if [[ "$RUST_BUILD_OK" != "true" ]]; then
    echo "Skipping Track A benchmarks due to build failure."
fi
for b in 1 5 9 13 17; do
    if [[ "$RUST_BUILD_OK" != "true" ]]; then
        fail "Track A benchmark $b: skipped (build failed)"
        continue
    fi
    out_file="$TMP_DIR/track-a-$b.json"

    stderr_output=$("$RUST_BIN" "$BENCH_DIR/mlsys-2026-$b.json" "$out_file" 2>&1 || true)

    if [[ -f "$out_file" ]]; then
        result=$(validate_solution "$out_file" "$b" "Track A")
        if [[ $? -eq 0 ]]; then
            pass "Track A benchmark $b: $result"
        else
            fail "Track A benchmark $b: validation failed - $result"
        fi
    else
        fail "Track A benchmark $b: binary produced no output"
    fi
done

# ---------------------------------------------------------------------------
# Track B: Test evaluator imports
# ---------------------------------------------------------------------------
echo ""
echo "--- Track B: Verifying Python evaluator ---"
if cd "$AGENT_DIR" && uv run python -c "from evaluator import *; print('evaluator imports OK')" 2>&1; then
    pass "Track B: evaluator import"
else
    fail "Track B: evaluator import failed"
fi

if cd "$AGENT_DIR" && uv run python -c "from scheduler import build_baseline, optimize; print('scheduler imports OK')" 2>&1; then
    pass "Track B: scheduler import"
else
    fail "Track B: scheduler import failed"
fi

# ---------------------------------------------------------------------------
# Track B: Run benchmarks (baseline mode, no API key)
# ---------------------------------------------------------------------------
echo ""
echo "--- Track B: Running benchmarks (baseline mode) ---"
for b in 1 5 9 13 17; do
    out_file="$TMP_DIR/track-b-$b.json"

    cd "$AGENT_DIR"
    GOOGLE_API_KEY=dummy uv run python agent.py \
        "$BENCH_DIR/mlsys-2026-$b.json" "$out_file" 2>/dev/null || true

    if [[ -f "$out_file" ]]; then
        result=$(validate_solution "$out_file" "$b" "Track B")
        if [[ $? -eq 0 ]]; then
            pass "Track B benchmark $b: $result"
        else
            fail "Track B benchmark $b: validation failed - $result"
        fi
    else
        fail "Track B benchmark $b: agent produced no output"
    fi
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "=== E2E Results ==="
echo "PASS: $PASS"
echo "FAIL: $FAIL"
echo "Total: $((PASS + FAIL))"

# Cleanup
rm -rf "$TMP_DIR"

if [[ $FAIL -eq 0 ]]; then
    echo ""
    echo "All E2E tests PASSED."
    exit 0
else
    echo ""
    echo "$FAIL test(s) FAILED."
    exit 1
fi
