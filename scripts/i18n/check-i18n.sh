#!/usr/bin/env bash
set -euo pipefail

ALLOWED="assets/i18n/en.json"
DIR="assets/i18n/"
OUT="${VIOLATIONS_FILE:-violations.txt}"

: "${GH_TOKEN:?GH_TOKEN is required}"
: "${REPO:?REPO is required}"
: "${PR_NUMBER:?PR_NUMBER is required}"

: > "$OUT"

gh api --paginate "repos/${REPO}/pulls/${PR_NUMBER}/files" \
  --jq '.[] | .filename, (.previous_filename // empty)' \
  | awk -v dir="$DIR" -v allowed="$ALLOWED" \
      'index($0, dir) == 1 && $0 != allowed' \
  | sort -u > "$OUT"

if [ -s "$OUT" ]; then
  echo "::error::Only ${ALLOWED} may be changed in ${DIR}. Invalid changes:"
  cat "$OUT"
  exit 1
fi