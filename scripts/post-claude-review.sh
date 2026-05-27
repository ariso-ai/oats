#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is required}"
: "${PR:?PR is required}"

if [ ! -f claude-output.json ]; then
  echo "::warning::claude-output.json not found; nothing to post."
  exit 0
fi

result=$(jq -r '.result // empty' claude-output.json)
if [ -z "$result" ]; then
  echo "::warning::claude-output.json has no .result field; nothing to post."
  exit 0
fi

printf '%s\n' "$result" | gh pr comment "$PR" --body-file -
echo "Posted Claude review to PR #$PR"
