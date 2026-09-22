#!/usr/bin/env bash
# check-serde-sync.sh — Detect serde(rename) drift between Rust and TypeScript.
#
# Checks that every field-level #[serde(rename = "X")] in models.rs
# has a corresponding "X" field name in types.ts.
#
# Exit 0 = all synced, Exit 1 = drift detected.

set -euo pipefail

MODELS="src-tauri/src/models.rs"
TYPES="src/lib/types.ts"
ERRORS=0

if [[ ! -f "$MODELS" || ! -f "$TYPES" ]]; then
  echo "Error: must run from project root (AgentCabin/)"
  exit 2
fi

echo "Checking serde rename sync: $MODELS → $TYPES"
echo ""

# Extract field-level renames: #[serde(rename = "camelName")]
# Skip rename_all (struct-level) and tag = "type" patterns.
while IFS= read -r line; do
  # Extract the rename target (the camelCase name)
  rename_target=$(echo "$line" | sed -n 's/.*rename\s*=\s*"\([^"]*\)".*/\1/p')
  line_num=$(echo "$line" | cut -d: -f1)

  # Skip tag renames (rename = "type" is a serde tag, not a field name)
  [[ "$rename_target" == "type" ]] && continue

  # Check if this name appears in types.ts
  if ! grep -q "\\b${rename_target}\\b" "$TYPES" 2>/dev/null; then
    echo "  DRIFT: models.rs:${line_num} renames to \"${rename_target}\" — NOT found in types.ts"
    ERRORS=$((ERRORS + 1))
  fi
done < <(grep -n 'serde(.*rename\s*=' "$MODELS" | grep -v 'rename_all' | grep -v 'tag\s*=')

# Also check struct-level rename_all = "camelCase" structs.
# For these, every Rust field_name becomes fieldName in JSON.
# We check the struct names as a warning (can't easily check every field).
CAMEL_STRUCTS=$(grep -B2 'rename_all.*"camelCase"' "$MODELS" | grep 'pub struct' | sed 's/.*pub struct \([A-Za-z0-9_]*\).*/\1/' || true)
if [[ -n "$CAMEL_STRUCTS" ]]; then
  echo "Structs with rename_all = \"camelCase\" (manual review recommended):"
  echo "$CAMEL_STRUCTS" | while read -r struct; do
    if grep -q "$struct" "$TYPES" 2>/dev/null; then
      echo "  OK: $struct found in types.ts"
    else
      echo "  WARN: $struct not found in types.ts (may be Rust-internal)"
    fi
  done
fi

echo ""
if [[ $ERRORS -gt 0 ]]; then
  echo "FAIL: $ERRORS serde rename(s) not synced with types.ts"
  exit 1
else
  echo "OK: All field-level serde renames found in types.ts"
  exit 0
fi
