#!/usr/bin/env bash
# .claude/hooks/scripts/debug-check.sh
#
# PostToolUse hook (Edit): scan recently modified files for debug artifacts
# that should not land in production code.
#
# Scans for:
#   - console.log / debugger in .ts/.tsx/.js/.jsx files
#   - print( in .py files (excluding test files)
#   - TODO / FIXME markers in any modified file
#
# Always exits 0 (non-blocking). Warnings go to stderr.

set -euo pipefail

# Collect files to inspect: prefer CLAUDE_FILE_PATH (single file from hook
# context), fall back to git diff for a broader sweep.
FILES=()

if [[ -n "${CLAUDE_FILE_PATH:-}" ]] && [[ -f "${CLAUDE_FILE_PATH}" ]]; then
  FILES+=("$CLAUDE_FILE_PATH")
else
  # Read git-tracked changed files (unstaged + staged)
  REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "$(pwd)")"
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    F="$REPO_ROOT/$line"
    [[ -f "$F" ]] && FILES+=("$F")
  done < <(git -C "${REPO_ROOT}" diff --name-only 2>/dev/null; git -C "${REPO_ROOT}" diff --name-only --cached 2>/dev/null)
fi

# Deduplicate (bash 3.2-compatible: no associative arrays required)
UNIQUE=()
for F in "${FILES[@]:-}"; do
  [[ -z "$F" ]] && continue
  ALREADY_SEEN=0
  if [[ ${#UNIQUE[@]} -gt 0 ]]; then
    for U in "${UNIQUE[@]}"; do
      if [[ "$U" == "$F" ]]; then
        ALREADY_SEEN=1
        break
      fi
    done
  fi
  if [[ "$ALREADY_SEEN" -eq 0 ]]; then
    UNIQUE+=("$F")
  fi
done
if [[ ${#UNIQUE[@]} -gt 0 ]]; then
  FILES=("${UNIQUE[@]}")
else
  FILES=()
fi

if [[ ${#FILES[@]} -eq 0 ]]; then
  exit 0
fi

WARNINGS=()

for FILE in "${FILES[@]}"; do
  EXT="${FILE##*.}"
  BASENAME="$(basename "$FILE")"

  case "$EXT" in
    ts|tsx|js|jsx|mjs|cjs)
      # console.log — noisy in production; should use structured logger
      if grep -qn "console\.log" "$FILE" 2>/dev/null; then
        LINES="$(grep -n "console\.log" "$FILE" | head -5 | awk '{print "    " $0}')"
        WARNINGS+=("console.log found in $FILE:\n$LINES")
      fi
      # debugger statement — must never ship
      if grep -qwn "debugger" "$FILE" 2>/dev/null; then
        LINES="$(grep -wn "debugger" "$FILE" | head -5 | awk '{print "    " $0}')"
        WARNINGS+=("debugger statement found in $FILE:\n$LINES")
      fi
      ;;

    py)
      # Skip test files (test_*.py and *_test.py)
      if [[ "$BASENAME" != test_* ]] && [[ "$BASENAME" != *_test.py ]]; then
        if grep -qn "^[[:space:]]*print(" "$FILE" 2>/dev/null; then
          LINES="$(grep -n "^[[:space:]]*print(" "$FILE" | head -5 | awk '{print "    " $0}')"
          WARNINGS+=("print() statement found in production Python file $FILE:\n$LINES")
        fi
      fi
      ;;
  esac

  # TODO / FIXME markers — flag in all supported source files
  case "$EXT" in
    py|ts|tsx|js|jsx|mjs|cjs|go|sh|yaml|yml|json)
      if grep -qwiEn "TODO|FIXME" "$FILE" 2>/dev/null; then
        LINES="$(grep -wiEn "TODO|FIXME" "$FILE" | head -5 | awk '{print "    " $0}')"
        WARNINGS+=("TODO/FIXME marker found in $FILE:\n$LINES")
      fi
      ;;
  esac
done

if [[ ${#WARNINGS[@]} -gt 0 ]]; then
  echo "" >&2
  echo "[debug-check] Debug artifact warnings (review before shipping):" >&2
  for W in "${WARNINGS[@]}"; do
    echo -e "  - $W" >&2
  done
  echo "" >&2
fi

exit 0
