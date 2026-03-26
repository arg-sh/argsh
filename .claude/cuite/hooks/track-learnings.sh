#!/usr/bin/env bash
# Hook: PostToolUse
# Purpose: After file edits, check if there are patterns worth capturing
#
# Matches: Edit|Write
# Tracks which domains are being modified in a session.
# Creates a breadcrumb file so /improve knows what to update.

set -euo pipefail

# Hooks run with cwd = project root (guaranteed by Claude Code).
# Using $PWD is robust regardless of symlink resolution of $0.
CLAUDE_DIR="$PWD/.claude"
CACHE_DIR="$CLAUDE_DIR/.cache"
BREADCRUMB="$CACHE_DIR/session-domains.txt"
DOMAIN_MAP="$CLAUDE_DIR/domain-map.conf"

mkdir -p "$CACHE_DIR"

# Read the tool input (contains file_path for Edit/Write)
INPUT=$(cat)

# Extract file path from JSON input (jq first, grep fallback)
FILE_PATH=$(echo "$INPUT" | jq -r '.file_path // empty' 2>/dev/null) || \
  FILE_PATH=$(echo "$INPUT" | grep -oP '"file_path"\s*:\s*"([^"]+)"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)

if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

# Map file path to domain using domain-map.conf
detect_domain_from_path() {
  local path="$1"

  # Read domain-map.conf for project-specific path mappings
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
  # Append domain to session breadcrumb (dedup later)
  echo "$DOMAIN" >> "$BREADCRUMB"
  # Deduplicate in place
  sort -u "$BREADCRUMB" -o "$BREADCRUMB"
fi
