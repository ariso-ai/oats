#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is required}"
: "${PR:?PR is required}"
: "${REPO:?REPO is required (owner/name)}"

# Identities the gate trusts. Anything outside these is ignored.
#   - CODERABBIT_BOT_LOGIN: the exact GitHub App identity for CodeRabbit.
#     Username-prefix matching is unsafe — squatters can register
#     "coderabbit-…" handles.
#   - VERDICT_BOT_LOGIN: who is allowed to post the <!-- claude-verdict -->
#     block. This must be the workflow bot; any other user posting a
#     verdict marker is treated as a spoofing attempt and ignored.
#   - AI_REVIEW_RESOLVERS: comma-separated GitHub logins whose
#     "resolved"/"addressed" replies count as marking a CodeRabbit
#     thread as addressed. Click-to-resolve in the UI always wins
#     (GraphQL isResolved), this is just the textual override path.
CODERABBIT_BOT_LOGIN="${CODERABBIT_BOT_LOGIN:-coderabbitai[bot]}"
VERDICT_BOT_LOGIN="${VERDICT_BOT_LOGIN:-github-actions[bot]}"
AI_REVIEW_RESOLVERS="${AI_REVIEW_RESOLVERS:-shawnzhu}"

# Escape hatch: humans can override the gate by adding a label.
if gh pr view "$PR" --repo "$REPO" --json labels --jq '.labels[].name' \
   | grep -qx 'override-ai-review'; then
  echo "PR #$PR has 'override-ai-review' label — bypassing AI gate."
  exit 0
fi

OWNER="${REPO%%/*}"
NAME="${REPO##*/}"

# Turn "a,b,c" into a JSON array string for jq --argjson.
resolvers_json=$(printf '%s' "$AI_REVIEW_RESOLVERS" \
  | jq -R 'split(",") | map(gsub("^\\s+|\\s+$"; ""))')

# --- Count unresolved CodeRabbit major findings ---------------------------
# Review-thread comments (the threaded inline comments CodeRabbit posts
# against specific diff lines) are the place its actionable findings live.
# A thread is treated as resolved when either:
#   (a) GitHub's GraphQL marks it isResolved (someone clicked Resolve), or
#   (b) a reply from an allowed resolver contains "resolved"/"addressed".
# Other users' "resolved" replies are ignored to prevent suppression.
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

cr_majors=$(printf '%s' "$threads_json" | jq \
  --arg bot      "$CODERABBIT_BOT_LOGIN" \
  --arg major    "$MAJOR_PATTERN" \
  --arg resolved "$RESOLVED_PATTERN" \
  --argjson resolvers "$resolvers_json" '
  [
    .data.repository.pullRequest.reviewThreads.nodes[]
    | select(.isResolved == false)
    | .comments.nodes as $nodes
    | select(($nodes | length) > 0)
    # First (root) comment must be from the trusted CodeRabbit bot.
    | select(($nodes[0].author.login // "") == $bot)
    | select($nodes[0].body | test($major; "i"))
    # Any reply from an allowed resolver containing the text "resolved" or
    # "addressed" marks the thread as addressed.
    | select(
        [ $nodes[]
          | select(.author.login as $a | $resolvers | index($a))
          | .body
          | test($resolved; "i")
        ] | any | not
      )
  ] | length
')

# Issue-level comments from CodeRabbit (PR-wide summaries). Author must
# be the exact CodeRabbit bot. Issue comments are flat, not threaded, so
# we have no "reply" axis — count any unresolved major-pattern body.
issue_comments_json=$(gh api "repos/$REPO/issues/$PR/comments" --paginate \
  | jq -s --arg bot "$CODERABBIT_BOT_LOGIN" \
      'add // [] | [.[] | select(.user.login == $bot and .user.type == "Bot") | .body]')

cr_issue_majors=$(printf '%s' "$issue_comments_json" | jq \
  --arg major    "$MAJOR_PATTERN" \
  --arg resolved "$RESOLVED_PATTERN" '
  [ .[] | select(test($major; "i")) | select(test($resolved; "i") | not) ] | length
')

cr_total=$((cr_majors + cr_issue_majors))

# --- Parse latest Claude verdict block ------------------------------------
# The verdict comment MUST come from the workflow bot. A non-bot user
# could otherwise post a comment containing the verdict marker and bypass
# the gate by claiming "findings: []".
verdict_body=$(gh api "repos/$REPO/issues/$PR/comments" --paginate \
  | jq -sr --arg bot "$VERDICT_BOT_LOGIN" '
      add // []
      | [.[] | select(.user.login == $bot and (.body | contains("<!-- claude-verdict -->")))]
      | last
      | .body // ""
    ')

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
  echo "::warning::No Claude verdict comment from ${VERDICT_BOT_LOGIN} found on this PR. Treating as 0 findings."
fi

total=$((cr_total + claude_majors))

echo "Trusted identities:"
echo "  CodeRabbit bot:    $CODERABBIT_BOT_LOGIN"
echo "  Verdict bot:       $VERDICT_BOT_LOGIN"
echo "  Allowed resolvers: $AI_REVIEW_RESOLVERS"
echo
echo "CodeRabbit unresolved majors (review threads): $cr_majors"
echo "CodeRabbit unresolved majors (issue comments): $cr_issue_majors"
echo "Claude blocking findings:                       $claude_majors"
echo "Total blocking findings:                        $total"

if [ "$total" -gt 0 ]; then
  echo "::error::AI review gate failed: $total blocking finding(s). Address them, mark CodeRabbit threads resolved, or add the 'override-ai-review' label."
  exit 1
fi

echo "AI review gate passed."
