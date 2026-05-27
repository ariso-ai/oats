#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is required}"
: "${PR:?PR is required}"
: "${REPO:?REPO is required (owner/name)}"

# Escape hatch: humans can override the gate by adding a label.
if gh pr view "$PR" --repo "$REPO" --json labels --jq '.labels[].name' \
   | grep -qx 'override-ai-review'; then
  echo "PR #$PR has 'override-ai-review' label — bypassing AI gate."
  exit 0
fi

OWNER="${REPO%%/*}"
NAME="${REPO##*/}"

# --- Count unresolved CodeRabbit major findings ---------------------------
# Review-thread comments (the threaded inline comments CodeRabbit posts
# against specific diff lines) are the place its actionable findings live.
# A thread is "resolved" when someone clicks Resolve in the GitHub UI; that
# state is exposed via GraphQL's isResolved. We also treat a thread as
# resolved if any reply in it contains the words "resolved" or "addressed",
# to honor teams who use the text marker convention.

threads_json=$(gh api graphql -F owner="$OWNER" -F name="$NAME" -F number="$PR" -f query='
  query($owner: String!, $name: String!, $number: Int!) {
    repository(owner: $owner, name: $name) {
      pullRequest(number: $number) {
        reviewThreads(first: 100) {
          nodes {
            isResolved
            comments(first: 50) {
              nodes {
                author { login }
                body
              }
            }
          }
        }
      }
    }
  }
')

MAJOR_PATTERN='⚠️ Potential issue|🛑|security|data loss|race condition'
RESOLVED_PATTERN='resolved|addressed'

cr_majors=$(printf '%s' "$threads_json" | jq --arg major "$MAJOR_PATTERN" --arg resolved "$RESOLVED_PATTERN" '
  [
    .data.repository.pullRequest.reviewThreads.nodes[]
    | select(.isResolved == false)
    | . as $thread
    | $thread.comments.nodes
    | select(length > 0)
    | select(.[0].author.login // "" | startswith("coderabbit"))
    | select(.[0].body | test($major; "i"))
    | select([.[] | .body | test($resolved; "i")] | any | not)
  ] | length
')

# Issue-level comments from CodeRabbit (PR-wide summaries). We count any
# matching the major pattern that does not itself contain "resolved"/
# "addressed" — issue comments are flat, not threaded, so the plan's
# "in a reply" rule degrades to "in the body itself" here.
issue_comments_json=$(gh api "repos/$REPO/issues/$PR/comments" --paginate \
  --jq '[.[] | select(.user.login | startswith("coderabbit")) | .body]')

cr_issue_majors=$(printf '%s' "$issue_comments_json" | jq --arg major "$MAJOR_PATTERN" --arg resolved "$RESOLVED_PATTERN" '
  [ .[] | select(test($major; "i")) | select(test($resolved; "i") | not) ] | length
')

cr_total=$((cr_majors + cr_issue_majors))

# --- Parse latest Claude verdict block ------------------------------------
verdict_body=$(gh api "repos/$REPO/issues/$PR/comments" --paginate \
  --jq '[.[] | select(.body | contains("<!-- claude-verdict -->"))] | last | .body // ""')

claude_majors=0
if [ -n "$verdict_body" ]; then
  # Extract the JSON between ```json and ``` that follows the marker.
  json_block=$(printf '%s\n' "$verdict_body" \
    | awk '
        /<!-- claude-verdict -->/ {found=1}
        found && /```json/ {in_block=1; next}
        in_block && /```/ {in_block=0; found=0; exit}
        in_block {print}
      ')
  if [ -n "$json_block" ]; then
    if ! claude_majors=$(printf '%s' "$json_block" | jq '
      [ .findings[]
        | select(
            .severity == "high"
            or (
              .severity == "medium"
              and (.category == "concurrency"
                   or .category == "public_api"
                   or .category == "migration"
                   or .category == "entitlements")
            )
          )
      ] | length
    ' 2>/dev/null); then
      echo "::warning::Failed to parse Claude verdict JSON. Treating as 0 findings."
      claude_majors=0
    fi
  else
    echo "::warning::Found claude-verdict marker but no fenced JSON block. Treating as 0 findings."
  fi
else
  echo "::warning::No Claude verdict comment found on this PR. Treating as 0 findings."
fi

total=$((cr_total + claude_majors))

echo "CodeRabbit unresolved majors (review threads): $cr_majors"
echo "CodeRabbit unresolved majors (issue comments): $cr_issue_majors"
echo "Claude blocking findings:                       $claude_majors"
echo "Total blocking findings:                        $total"

if [ "$total" -gt 0 ]; then
  echo "::error::AI review gate failed: $total blocking finding(s). Address them, mark CodeRabbit threads resolved, or add the 'override-ai-review' label."
  exit 1
fi

echo "AI review gate passed."
