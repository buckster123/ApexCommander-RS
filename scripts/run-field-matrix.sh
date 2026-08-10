#!/usr/bin/env bash
# Run the compositor field matrix inside the *current* graphical session.
# Saves JSON + markdown under docs/field-evidence/ for the docs ledger.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FAMILY="${XDG_CURRENT_DESKTOP:-unknown}"
FAMILY="$(printf '%s' "$FAMILY" | tr '[:upper:]' '[:lower:]' | tr ':/ ' '---' | tr -cd 'a-z0-9._-')"
STYPE="${XDG_SESSION_TYPE:-unknown}"
TAG="${FAMILY}-${STYPE}"
OUT_DIR="$ROOT/docs/field-evidence"
mkdir -p "$OUT_DIR"

echo "==> Building apex-harness-cli"
cargo build -q -p apex-harness-cli

JSON_OUT="$OUT_DIR/${TAG}.json"
MD_OUT="$OUT_DIR/${TAG}.md"

echo "==> field-report → $JSON_OUT"
# markdown section on stderr, JSON on stdout
cargo run -q -p apex-harness-cli -- field-report --markdown \
  2> "$MD_OUT" \
  > "$JSON_OUT"

echo "==> Markdown summary:"
cat "$MD_OUT"
echo
echo "==> Done. Paste/update docs/field-matrix.md from $MD_OUT"
echo "    JSON evidence: $JSON_OUT"

# Exit non-zero if the report itself failed
python3 - "$JSON_OUT" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
sys.exit(0 if r.get("ok") else 2)
PY
