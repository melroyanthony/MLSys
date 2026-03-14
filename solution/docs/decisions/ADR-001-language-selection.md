# ADR-001: Rust for Track A, Python for Track B

## Status
Accepted (supersedes original Python-only decision)

## Context
The MLSys 2026 contest has two tracks:
- **Track A (Systems)**: Deliver a compiled binary `mlsys` that reads problem JSON and writes solution JSON. Source in "C++/Rust/etc." explicitly allowed.
- **Track B (Agents)**: Deliver a Python `agent.py` using Google Gemini API exclusively.

Key considerations:
- Track A timeout is 2-120 seconds depending on benchmark size — performance matters
- Track B timeout is 10 minutes per benchmark — Python + API latency is fine
- The reference evaluator is C++ (`mlsys.h`) — Rust matches C++ performance and memory safety
- Data sizes: up to 96 ops, 160 tensors — algorithmic efficiency matters more than raw speed, but Rust eliminates any performance concerns

## Decision

### Track A: Rust
- Pure Rust implementation with `serde_json` for I/O
- Compile to statically linked binary named `mlsys`
- Zero-copy JSON parsing where possible
- Module structure mirrors the algorithm pipeline

### Track B: Python
- Python 3.12+ script `agent.py`
- Uses `google-genai` SDK for Gemini API calls
- Agent reasons about the problem and generates solution JSON
- Prompts and few-shot examples in `prompts/` directory

## Consequences

### Positive
- **Rust**: Zero-cost abstractions, fearless concurrency for parallel search, guaranteed memory safety, matches C++ performance
- **Rust**: Strong type system with enums/pattern matching maps naturally to op types and scheduling decisions
- **Rust**: Static linking produces a single binary — no runtime dependencies
- **Python (Track B)**: Natural fit for API calls and prompt engineering
- **Both**: Each track uses the language best suited to its constraints

### Negative
- **Rust**: Slower iteration speed than Python during development
- **Two codebases**: Must maintain separate implementations (but they solve different sub-problems)

### Mitigations
- Rust's compiler catches many bugs at compile time, offsetting slower iteration
- Track A and Track B have distinct deliverable formats — no shared code needed
- The algorithm logic is the same; only the implementation language differs
