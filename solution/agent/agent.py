#!/usr/bin/env python3
"""
MLSys 2026 Track B — Gemini-powered DAG Scheduler Agent.

CLI usage:
    python agent.py <input.json> <output.json>

The agent:
  1. Parses the input problem JSON
  2. Generates a locally-optimized baseline schedule (no API call)
  3. Writes the baseline to output immediately (safe fallback)
  4. Uses Gemini to reason about further optimizations
  5. Validates each Gemini suggestion locally
  6. Writes the best valid solution found within the time budget
"""

from __future__ import annotations

import json
import math
import os
import sys
import time
import traceback
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Local modules
# ---------------------------------------------------------------------------
from evaluator import (
    Granularity,
    OOMError,
    Problem,
    Solution,
    SubgraphDef,
    ValidationError,
    check_oom,
    compute_subgraph_latency,
    evaluate,
    parse_problem,
    solution_to_dict,
    topological_sort,
    _k_full_for_op,
    _output_tensor_for_subgraph,
)
from scheduler import build_baseline, optimize

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

TIMEOUT_SECONDS = 9 * 60  # 9 minutes (leave 1 min buffer)
GEMINI_MODEL = "gemini-2.5-flash"
MAX_GEMINI_RETRIES = 2
MAX_REFINEMENT_ROUNDS = 3

AGENT_DIR = Path(__file__).parent
PROMPTS_DIR = AGENT_DIR / "prompts"


# ---------------------------------------------------------------------------
# Prompt loading
# ---------------------------------------------------------------------------

def _load_prompt(name: str) -> str:
    path = PROMPTS_DIR / name
    if path.exists():
        return path.read_text(encoding="utf-8")
    return ""


# ---------------------------------------------------------------------------
# Problem-to-prompt serialization
# ---------------------------------------------------------------------------

def _problem_to_text(problem: Problem, prob_data: dict) -> str:
    """Render the problem as a human-readable description for the prompt."""
    lines = []
    lines.append("## Problem Input")
    lines.append(f"- Tensors: {len(problem.tensors)}")
    lines.append(f"- Ops: {len(problem.ops)}")
    lines.append(f"- Fast memory capacity: {problem.fast_memory_capacity}")
    lines.append(f"- Slow memory bandwidth: {problem.slow_memory_bandwidth}")
    lines.append(
        f"- Native granularity: {problem.native_granularity[0]}x{problem.native_granularity[1]}"
    )
    lines.append("")

    lines.append("### Tensors")
    for i, t in enumerate(problem.tensors):
        lines.append(f"  Tensor[{i}]: {t.width}x{t.height} ({t.width * t.height} elements)")

    lines.append("")
    lines.append("### Operations")
    for i, op in enumerate(problem.ops):
        lines.append(
            f"  Op[{i}]: {op.op_type}, inputs={op.inputs}, outputs={op.outputs}, "
            f"base_cost={op.base_cost}"
        )

    lines.append("")
    lines.append("### Raw JSON (for reference)")
    lines.append("```json")
    lines.append(json.dumps(prob_data, indent=2))
    lines.append("```")
    return "\n".join(lines)


def _solution_to_text(solution: Solution, total_latency: float) -> str:
    """Render a solution as a human-readable summary for the prompt."""
    lines = []
    lines.append(f"## Current Best Solution (total latency = {total_latency:.2f})")
    for i, sg in enumerate(solution.subgraphs):
        g = sg.granularity
        lines.append(
            f"  Subgraph[{i}]: ops={sg.ops}, gran=({g.w},{g.h},{g.k}), "
            f"retain={sg.tensors_to_retain}, latency={sg.subgraph_latency:.2f}"
        )
    lines.append("")
    lines.append("### Raw JSON")
    lines.append("```json")
    d = solution_to_dict(solution)
    lines.append(json.dumps(d, indent=2))
    lines.append("```")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Gemini API interface
# ---------------------------------------------------------------------------

def _call_gemini(system_prompt: str, user_prompt: str) -> Optional[str]:
    """
    Call the Gemini API with system + user prompts.
    Returns the text response, or None on failure.
    """
    try:
        from google import genai

        client = genai.Client()

        # Combine system and user into a single contents string
        # (google-genai SDK uses contents for the full conversation)
        full_prompt = (
            f"{system_prompt}\n\n---\n\n{user_prompt}\n\n"
            "Return ONLY valid JSON (no markdown fences, no explanation):"
        )

        response = client.models.generate_content(
            model=GEMINI_MODEL,
            contents=full_prompt,
        )
        return response.text

    except Exception as exc:
        print(f"[agent] Gemini API error: {exc}", file=sys.stderr)
        return None


# ---------------------------------------------------------------------------
# Response parsing
# ---------------------------------------------------------------------------

def _parse_gemini_response(text: str) -> Optional[dict]:
    """
    Extract and parse the JSON from Gemini's response.
    Handles responses wrapped in markdown code fences.
    """
    if text is None:
        return None

    # Strip markdown fences if present
    stripped = text.strip()
    if stripped.startswith("```"):
        lines = stripped.split("\n")
        # Remove first and last fence line
        inner_lines = []
        in_block = False
        for line in lines:
            if line.startswith("```") and not in_block:
                in_block = True
                continue
            if line.startswith("```") and in_block:
                in_block = False
                continue
            if in_block:
                inner_lines.append(line)
        stripped = "\n".join(inner_lines)

    try:
        return json.loads(stripped)
    except json.JSONDecodeError:
        # Try to find JSON object in the text
        start = text.find("{")
        end = text.rfind("}")
        if start != -1 and end != -1 and end > start:
            try:
                return json.loads(text[start : end + 1])
            except json.JSONDecodeError:
                pass
    return None


# ---------------------------------------------------------------------------
# Solution reconstruction from Gemini response
# ---------------------------------------------------------------------------

def _build_solution_from_dict(data: dict, problem: Problem) -> Optional[Solution]:
    """
    Reconstruct a Solution from Gemini's JSON response.
    Returns None if the response is structurally invalid.
    """
    try:
        sg_ops_list = data["subgraphs"]
        gran_list = data["granularities"]
        retain_list = data.get("tensors_to_retain", [[] for _ in sg_ops_list])
        trav_list = data.get("traversal_orders", [None for _ in sg_ops_list])

        subgraphs = []
        for i in range(len(sg_ops_list)):
            g = gran_list[i]
            sg = SubgraphDef(
                ops=[int(x) for x in sg_ops_list[i]],
                granularity=Granularity(int(g[0]), int(g[1]), int(g[2])),
                tensors_to_retain=[int(x) for x in retain_list[i]],
                traversal_order=[int(x) for x in trav_list[i]] if trav_list[i] is not None else None,
                subgraph_latency=0.0,
            )
            subgraphs.append(sg)

        solution = Solution(subgraphs=subgraphs)
        return solution
    except (KeyError, IndexError, TypeError, ValueError) as exc:
        print(f"[agent] Response parsing error: {exc}", file=sys.stderr)
        return None


# ---------------------------------------------------------------------------
# Local validation + latency recomputation
# ---------------------------------------------------------------------------

def _validate_and_score(solution: Solution, problem: Problem) -> Optional[float]:
    """
    Validate the solution locally and return its total latency.
    Returns None if the solution is invalid (OOM or coverage error).
    """
    try:
        import math
        from evaluator import compute_subgraph_latency as csl, _output_tensor_for_subgraph

        retained: set[int] = set()
        total = 0.0
        for sg in solution.subgraphs:
            if not check_oom(sg.ops, sg.granularity, problem, retained):
                return None
            # Validate traversal_order is a valid permutation if present
            if sg.traversal_order is not None:
                out_t = _output_tensor_for_subgraph(sg.ops, problem)
                num_tiles = (math.ceil(out_t.width / sg.granularity.w)
                             * math.ceil(out_t.height / sg.granularity.h))
                expected = set(range(num_tiles))
                if set(sg.traversal_order) != expected or len(sg.traversal_order) != num_tiles:
                    sg.traversal_order = None  # invalid, fall back to raster
            lat = csl(
                sg.ops, sg.granularity, problem, retained, sg.traversal_order,
                tensors_to_retain_after=set(sg.tensors_to_retain),
            )
            sg.subgraph_latency = lat
            total += lat
            retained = set(sg.tensors_to_retain)

        # Coverage check
        all_ops = set(range(len(problem.ops)))
        covered = set()
        for sg in solution.subgraphs:
            covered.update(sg.ops)
        if not all_ops.issubset(covered):
            return None

        return total
    except Exception as exc:
        print(f"[agent] Validation error: {exc}", file=sys.stderr)
        return None


# ---------------------------------------------------------------------------
# Write solution to file
# ---------------------------------------------------------------------------

def _write_solution(solution: Solution, output_path: str) -> None:
    d = solution_to_dict(solution)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(d, f, indent=2)


# ---------------------------------------------------------------------------
# Main agent loop
# ---------------------------------------------------------------------------

def run_agent(input_path: str, output_path: str) -> None:
    start_time = time.time()

    def elapsed() -> float:
        return time.time() - start_time

    def time_remaining() -> float:
        return TIMEOUT_SECONDS - elapsed()

    print(f"[agent] Reading problem: {input_path}", file=sys.stderr)

    with open(input_path, "r", encoding="utf-8") as f:
        prob_data = json.load(f)

    problem = parse_problem(prob_data)
    print(
        f"[agent] Problem: {len(problem.ops)} ops, {len(problem.tensors)} tensors, "
        f"fast_mem={problem.fast_memory_capacity}, bw={problem.slow_memory_bandwidth}",
        file=sys.stderr,
    )

    # ------------------------------------------------------------------ #
    # Step 1: Generate locally-optimized baseline (no API needed)         #
    # ------------------------------------------------------------------ #
    print("[agent] Building optimized local schedule...", file=sys.stderr)
    try:
        best_solution = optimize(problem)
    except Exception:
        print("[agent] Optimizer failed, falling back to baseline...", file=sys.stderr)
        traceback.print_exc(file=sys.stderr)
        best_solution = build_baseline(problem)

    best_latency = _validate_and_score(best_solution, problem)
    if best_latency is None:
        print("[agent] Local optimizer produced invalid solution. Using raw baseline.", file=sys.stderr)
        best_solution = build_baseline(problem)
        best_latency = sum(sg.subgraph_latency for sg in best_solution.subgraphs)

    print(f"[agent] Local best latency: {best_latency:.2f} (took {elapsed():.1f}s)", file=sys.stderr)

    # Write baseline immediately — safe fallback even if Gemini fails
    _write_solution(best_solution, output_path)
    print(f"[agent] Baseline written to {output_path}", file=sys.stderr)

    # ------------------------------------------------------------------ #
    # Step 2: Check if Gemini API is available                            #
    # ------------------------------------------------------------------ #
    api_key = os.environ.get("GOOGLE_API_KEY", "")
    if not api_key or api_key == "dummy":
        print("[agent] No GOOGLE_API_KEY — skipping Gemini optimization.", file=sys.stderr)
        return

    # ------------------------------------------------------------------ #
    # Step 3: Load prompts                                                #
    # ------------------------------------------------------------------ #
    system_prompt = "\n\n".join([
        _load_prompt("system.md"),
        _load_prompt("examples.md"),
        _load_prompt("strategies.md"),
    ])

    # ------------------------------------------------------------------ #
    # Step 4: Gemini optimization rounds                                  #
    # ------------------------------------------------------------------ #
    problem_text = _problem_to_text(problem, prob_data)

    for round_num in range(1, MAX_REFINEMENT_ROUNDS + 1):
        if time_remaining() < 60:
            print(f"[agent] Time budget exhausted after round {round_num - 1}.", file=sys.stderr)
            break

        print(f"[agent] Gemini round {round_num}/{MAX_REFINEMENT_ROUNDS} "
              f"(time remaining: {time_remaining():.0f}s)...", file=sys.stderr)

        solution_text = _solution_to_text(best_solution, best_latency)

        user_prompt = (
            f"{problem_text}\n\n"
            f"{solution_text}\n\n"
            f"The current total latency is **{best_latency:.2f}**. "
            f"Please analyze this DAG and produce an improved execution schedule "
            f"that minimizes total latency while keeping all working sets within "
            f"fast_memory_capacity={problem.fast_memory_capacity}.\n\n"
            f"Focus on:\n"
            f"1. Op fusion opportunities (eliminate intermediate tensor transfers)\n"
            f"2. Split-K for MatMul subgraphs under memory pressure\n"
            f"3. Tensor retention where downstream subgraph immediately needs the tensor\n"
            f"4. Granularity tuning to reach the roofline equilibrium point\n"
            f"5. Snake traversal order for MatMul tiles\n\n"
            f"Return ONLY the JSON solution object."
        )

        # Try Gemini with retries
        response_text = None
        for attempt in range(MAX_GEMINI_RETRIES):
            response_text = _call_gemini(system_prompt, user_prompt)
            if response_text is not None:
                break
            print(f"[agent]   API attempt {attempt + 1} failed, retrying...", file=sys.stderr)
            time.sleep(2)

        if response_text is None:
            print(f"[agent]   Round {round_num}: no response from Gemini.", file=sys.stderr)
            continue

        # Parse response
        response_dict = _parse_gemini_response(response_text)
        if response_dict is None:
            print(f"[agent]   Round {round_num}: could not parse Gemini response.", file=sys.stderr)
            print(f"[agent]   Response preview: {response_text[:500]}", file=sys.stderr)
            continue

        # Build solution object
        candidate = _build_solution_from_dict(response_dict, problem)
        if candidate is None:
            print(f"[agent]   Round {round_num}: malformed solution structure.", file=sys.stderr)
            continue

        # Validate and score
        candidate_latency = _validate_and_score(candidate, problem)
        if candidate_latency is None:
            print(f"[agent]   Round {round_num}: solution failed validation (OOM or missing ops).", file=sys.stderr)
            continue

        print(
            f"[agent]   Round {round_num}: Gemini latency={candidate_latency:.2f} "
            f"(current best={best_latency:.2f})",
            file=sys.stderr,
        )

        if candidate_latency < best_latency:
            best_latency = candidate_latency
            best_solution = candidate
            _write_solution(best_solution, output_path)
            print(
                f"[agent]   Improvement! New best: {best_latency:.2f}. Written to {output_path}",
                file=sys.stderr,
            )
        else:
            print(f"[agent]   No improvement in round {round_num}.", file=sys.stderr)

    print(
        f"[agent] Done. Final latency: {best_latency:.2f} "
        f"(total elapsed: {elapsed():.1f}s)",
        file=sys.stderr,
    )


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    if len(sys.argv) != 3:
        print("Usage: python agent.py <input.json> <output.json>", file=sys.stderr)
        sys.exit(1)

    input_path = sys.argv[1]
    output_path = sys.argv[2]

    if not Path(input_path).exists():
        print(f"Error: input file not found: {input_path}", file=sys.stderr)
        sys.exit(1)

    run_agent(input_path, output_path)


if __name__ == "__main__":
    main()
