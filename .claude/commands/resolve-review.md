---
description: "Fetch PR review comments, categorize fixes, apply them, commit, push, and reply"
allowed-tools: Read, Glob, Grep, Edit, Write, Bash, AskUserQuestion
---

# Resolve Review

Automated workflow for addressing PR review comments.

## Input
$ARGUMENTS — PR number. If empty, uses the current branch's PR.

## Process

1. **Determine PR number**:
   - If $ARGUMENTS is a number, use it
   - If empty, detect from current branch: `gh pr view --json number -q .number`

2. **Fetch review comments**:
   ```bash
   REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
   gh api "repos/${REPO}/pulls/${PR_NUMBER}/comments" --jq '.[] | "### \(.path):\(.line)\n\(.body)\n"'
   ```

3. **Categorize each comment**:
   - **Fix**: Legitimate issue that should be addressed (shell bugs, count mismatches, misleading docs, security issues)
   - **Acknowledge**: Valid observation but intentional design decision — reply with rationale
   - **Dismiss**: False positive, already addressed, or low-confidence — reply with explanation

4. **For each "Fix" comment**:
   - Read the referenced file and line
   - Apply the fix
   - Track what was changed

5. **Commit all fixes**:
   ```bash
   git add -A
   git commit -m "fix: address review findings on PR #N

   - [fix 1 description]
   - [fix 2 description]

   Refs #[issue]"
   ```

6. **Push to PR branch**:
   ```bash
   git push origin <branch>
   ```

7. **Reply to PR** with summary table:
   ```bash
   gh pr comment N --body "## Addressing Review (M comments)

   ### Fixed (X)
   | # | Finding | Fix |
   |---|---------|-----|
   | 1 | [finding] | [what was done] |

   ### Acknowledged (Y)
   | # | Finding | Response |
   |---|---------|----------|
   | 1 | [finding] | [rationale] |

   ### Dismissed (Z)
   | # | Finding | Reason |
   |---|---------|--------|
   | 1 | [finding] | [why dismissed] |"
   ```

## Output
- Fixes committed and pushed
- Review response comment posted on PR
- Summary of actions taken
