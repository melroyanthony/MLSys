#!/usr/bin/env bash
# .claude/hooks/scripts/commit-reminder.sh
#
# Stop hook: check for uncommitted changes and remind the user to commit
# before the session ends.
#
# Also performs a lightweight scan of staged/unstaged files for leftover
# debug statements (console.log, debugger, print()).
#
# Always exits 0 (non-blocking). Output goes to stderr so Claude Code
# surfaces it as a session-end notice.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "")"

if [[ -z "$REPO_ROOT" ]]; then
  # Not inside a git repository — nothing to do
  exit 0
fi

# ── 1. Uncommitted changes reminder ─────────────────────────────────────────

PORCELAIN="$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null)"

if [[ -n "$PORCELAIN" ]]; then
  CHANGED_COUNT="$(echo "$PORCELAIN" | grep -c '.' || true)"
  echo "" >&2
  echo "[commit-reminder] You have $CHANGED_COUNT file(s) with uncommitted changes:" >&2
  echo "$PORCELAIN" | head -20 | while IFS= read -r line; do
    echo "  $line" >&2
  done
  if [[ "$CHANGED_COUNT" -gt 20 ]]; then
    echo "  ... and $((CHANGED_COUNT - 20)) more" >&2
  fi
  echo "" >&2
  echo "  Consider committing your work:" >&2
  echo "    git add <files>" >&2
  echo "    git commit -m \"<message>\"" >&2
  echo "" >&2
fi

# ── 2. Debug artifact scan on modified files ────────────────────────────────

# Gather all modified files (staged + unstaged, tracked by git)
MODIFIED_FILES=()
while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  F="$REPO_ROOT/$line"
  [[ -f "$F" ]] && MODIFIED_FILES+=("$F")
done < <(
  git -C "$REPO_ROOT" diff --name-only 2>/dev/null
  git -C "$REPO_ROOT" diff --name-only --cached 2>/dev/null
)

# If no modified files, skip debug scan
if [[ ${#MODIFIED_FILES[@]} -eq 0 ]]; then
  exit 0
fi

# Deduplicate using sort -u
UNIQUE=()
while IFS= read -r F; do
  [[ -n "$F" ]] && UNIQUE+=("$F")
done < <(printf '%s\n' "${MODIFIED_FILES[@]}" | sort -u)

if [[ ${#UNIQUE[@]} -eq 0 ]]; then
  exit 0
fi

DEBUG_WARNINGS=()

for FILE in "${UNIQUE[@]}"; do
  EXT="${FILE##*.}"
  BASENAME="$(basename "$FILE")"

  case "$EXT" in
    ts|tsx|js|jsx|mjs|cjs)
      if grep -qn "console\.log\|debugger" "$FILE" 2>/dev/null; then
        DEBUG_WARNINGS+=("$FILE: contains console.log or debugger statement(s)")
      fi
      ;;
    py)
      if [[ "$BASENAME" != test_* ]] && [[ "$BASENAME" != *_test.py ]]; then
        if grep -qn "^[[:space:]]*print(" "$FILE" 2>/dev/null; then
          DEBUG_WARNINGS+=("$FILE: contains print() statement(s) in production code")
        fi
      fi
      ;;
  esac
done

if [[ ${#DEBUG_WARNINGS[@]} -gt 0 ]]; then
  echo "[commit-reminder] Leftover debug statements detected in modified files:" >&2
  for W in "${DEBUG_WARNINGS[@]}"; do
    echo "  - $W" >&2
  done
  echo "  Clean these up before committing to keep production code tidy." >&2
  echo "" >&2
fi

exit 0
