#!/usr/bin/env bash
set -euo pipefail

MARKER="<!-- i18n-check -->"
FILE="${VIOLATIONS_FILE:-violations.txt}"

: "${GH_TOKEN:?GH_TOKEN is required}"
: "${REPO:?REPO is required}"
: "${PR_NUMBER:?PR_NUMBER is required}"
: "${FAILED:?FAILED is required}"

COMMENT_ID=$(gh api --paginate "repos/${REPO}/issues/${PR_NUMBER}/comments" \
  --jq ".[] | select(.user.type == \"Bot\" and (.body | contains(\"${MARKER}\"))) | .id" \
  | sed -n '1p')

if [ "$FAILED" = "true" ]; then
  PATHS=""
  if [ -f "$FILE" ]; then
    PATHS=$(sed 's/.*/- `&`/' "$FILE")
  fi

  BODY=$(printf '%s\n### ❌ Invalid changes in `assets/i18n`\n\nOnly `assets/i18n/en.json` may be modified in this directory. The following files must not be changed:\n\n%s\n\nPlease revert these changes and push again.\n' "$MARKER" "$PATHS")

  if [ -n "$COMMENT_ID" ]; then
    gh api -X PATCH "repos/${REPO}/issues/comments/${COMMENT_ID}" -f body="$BODY" > /dev/null
  else
    gh api -X POST "repos/${REPO}/issues/${PR_NUMBER}/comments" -f body="$BODY" > /dev/null
  fi
elif [ -n "$COMMENT_ID" ]; then
  gh api -X DELETE "repos/${REPO}/issues/comments/${COMMENT_ID}"
fi