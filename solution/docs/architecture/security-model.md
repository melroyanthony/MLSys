# Security Model

This is a single-user CLI tool that processes local JSON files. There is no network communication, no authentication, no multi-tenancy, and no user-supplied code execution.

## Threat Model

| Threat | Applicable? | Mitigation |
|--------|-------------|------------|
| Injection attacks (SQL, command) | No | No database, no shell commands |
| Malicious input JSON | Low risk | JSON parser handles untrusted input safely; no `eval()` |
| Denial of service | Not applicable | Single-user local tool |
| Data exfiltration | Not applicable | No network, no secrets |
| Path traversal | Low risk | CLI accepts file paths; use `std::fs::canonicalize()` for safety |

## Input Validation

- All JSON parsing uses `serde_json` (no `eval`, no unsafe deserialization)
- All numeric fields are validated for expected types and ranges
- Tensor indices in `inputs` and `outputs` are bounds-checked against the tensor array length
- Op count consistency is verified across all parallel arrays

## No Secrets

This project contains no API keys, passwords, tokens, or credentials. The `.env` file pattern is not applicable.
