# Security Model

This is a single-user CLI tool that processes local JSON files. Track A (Rust
binary) has no network communication, no authentication, no multi-tenancy, and
no user-supplied code execution.

Track B (Python agent) makes HTTPS calls to the Gemini API
(`generativelanguage.googleapis.com`) when a `GOOGLE_API_KEY` environment
variable is set. No problem data or solution JSON is logged or persisted by the
API client beyond the scope of a single agent run. When `GOOGLE_API_KEY` is set
to `dummy` or omitted, the agent runs in local-only mode with zero network
traffic. Note: on API error paths, the agent may print a preview of the
Gemini response (first 500 chars) to stderr for debugging, which could include
partial solution JSON. This is not routine logging.

## Threat Model

| Threat | Applicable? | Mitigation |
|--------|-------------|------------|
| Injection attacks (SQL, command) | No | No database, no shell commands |
| Malicious input JSON | Low risk | JSON parser handles untrusted input safely; no `eval()` |
| Denial of service | Not applicable | Single-user local tool |
| Data exfiltration | Track B only | HTTPS to Gemini API; no credentials stored; `GOOGLE_API_KEY` read from environment only |
| Path traversal | Low risk | CLI accepts file paths; use `std::fs::canonicalize()` for safety |

## Input Validation

- All JSON parsing uses `serde_json` (no `eval`, no unsafe deserialization)
- All numeric fields are validated for expected types and ranges
- Tensor indices in `inputs` and `outputs` are bounds-checked against the tensor array length
- Op count consistency is verified across all parallel arrays

## No Secrets

Track A contains no API keys, passwords, tokens, or credentials. The `.env` file
pattern is not applicable for the Rust binary.

Track B reads `GOOGLE_API_KEY` from the environment at runtime. This key is never
written to disk, never logged, and never embedded in source code. The agent falls
back to local-only mode if the key is absent or set to `dummy`.
