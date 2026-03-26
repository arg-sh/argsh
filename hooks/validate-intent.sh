#!/usr/bin/env bash
# Hook: PostToolUse (validate-intent)
# Purpose: Validate file edits after execution and provide feedback
#
# Matches: Edit|Write
# Checks:
#   - Syntax validation for shell scripts (bash -n)
#   - Cargo check hint for Rust files
#   - Tracks domain edits (extends track-learnings breadcrumb)

set -euo pipefail

# Hooks run with cwd = project root (guaranteed by Claude Code).
# Using $PWD is robust regardless of symlink resolution of $0.
CLAUDE_DIR="$PWD/.claude"
CACHE_DIR="$CLAUDE_DIR/.cache"
DOMAIN_MAP="$CLAUDE_DIR/domain-map.conf"

mkdir -p "$CACHE_DIR"

INPUT=$(cat)

# Extract file path
if command -v jq &>/dev/null; then
  FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null || true)
else
  FILE_PATH=$(echo "$INPUT" | grep -oP '"file_path"\s*:\s*"([^"]+)"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)
fi

if [[ -z "$FILE_PATH" || ! -f "$FILE_PATH" ]]; then
  exit 0
fi

# Syntax validation for shell scripts
case "$FILE_PATH" in
  *.sh|*.bash)
    ERR_FILE=$(mktemp)
    if ! bash -n "$FILE_PATH" 2>"$ERR_FILE"; then
      echo "WARNING: syntax error in ${FILE_PATH##*/}:"
      cat "$ERR_FILE"
    fi
    rm -f "$ERR_FILE"
    ;;
esac

# Track edit count per domain in session (uses domain-map.conf)
detect_domain_from_path() {
  local path="$1"
  if [[ -f "$DOMAIN_MAP" ]]; then
    while IFS='=' read -r pattern domain; do
      [[ -z "$pattern" || "$pattern" == \#* ]] && continue
      pattern=$(echo "$pattern" | xargs)
      domain=$(echo "$domain" | xargs)
      [[ -z "$pattern" || -z "$domain" ]] && continue
      # shellcheck disable=SC2254
      case "$path" in
        $pattern) echo "$domain"; return ;;
      esac
    done < "$DOMAIN_MAP"
  fi
  echo ""
}

DOMAIN=$(detect_domain_from_path "$FILE_PATH")
if [[ -n "$DOMAIN" ]]; then
  EDIT_LOG="$CACHE_DIR/session-edits.log"
  echo "$(date +%H:%M:%S) $DOMAIN ${FILE_PATH##*/}" >> "$EDIT_LOG"
fi

exit 0
