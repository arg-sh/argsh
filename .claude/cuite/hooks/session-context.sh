#!/usr/bin/env bash
# Hook: SessionStart
# Purpose: Print available domains and recent activity on session start
#
# Gives Claude immediate context about what's available and what's been
# worked on recently, so it can route requests to the right domain.

set -euo pipefail

# Hooks run with cwd = project root (guaranteed by Claude Code).
# Using $PWD is robust regardless of symlink resolution of $0.
CLAUDE_DIR="$PWD/.claude"
PROJECT_DIR="$PWD"
EXPERTS_DIR="$CLAUDE_DIR/agents/experts"

echo ""
echo "=== Agent System Active ==="
echo ""

# Auto-discover domains from experts/ directory
if [[ -d "$EXPERTS_DIR" ]]; then
  DOMAINS=""
  for dir in "$EXPERTS_DIR"/*/; do
    [[ -d "$dir" ]] || continue
    name=$(basename "$dir")
    # Skip non-domain dirs (e.g. files, agent-teams-blueprint.md is a file not dir)
    [[ -f "$EXPERTS_DIR/$name/expertise.yaml" || -f "$EXPERTS_DIR/$name/tips.md" ]] || continue
    DOMAINS="${DOMAINS:+$DOMAINS | }$name"
  done
  if [[ -n "$DOMAINS" ]]; then
    echo "Domains: $DOMAINS"
  fi
fi

echo "Commands: /do <task> | /do-teams <task> | /improve [domain]"
echo ""

# Print active tuning knobs so all agents share the same expectations
TUNING="$CLAUDE_DIR/cuite/tuning.conf"
[[ -f "$TUNING" ]] || TUNING="$PWD/tuning.conf"  # dev/standalone fallback
if [[ -f "$TUNING" ]]; then
  echo "Project tuning (tuning.conf):"
  while IFS='=' read -r key value; do
    key=$(echo "$key" | xargs)
    value=$(echo "$value" | xargs)
    [[ -z "$key" || "$key" == \#* ]] && continue
    printf "  %-35s %s\n" "$key" "$value"
  done < "$TUNING"
  echo ""
fi

# Show recent git activity to prime domain detection
if command -v git &>/dev/null && git -C "$PROJECT_DIR" rev-parse --git-dir &>/dev/null 2>&1; then
  RECENT=$(git -C "$PROJECT_DIR" log --oneline -5 --no-decorate 2>/dev/null || true)
  if [[ -n "$RECENT" ]]; then
    echo "Recent commits:"
    echo "$RECENT"
    echo ""
  fi
fi

# Show any modified expertise files (indicates recent improvements)
if command -v git &>/dev/null && git -C "$PROJECT_DIR" rev-parse --git-dir &>/dev/null 2>&1; then
  MODIFIED=$(git -C "$PROJECT_DIR" diff --name-only HEAD 2>/dev/null | grep "expertise.yaml" || true)
  if [[ -n "$MODIFIED" ]]; then
    echo "Expertise files with uncommitted updates:"
    echo "$MODIFIED"
    echo "(Consider running /improve to capture learnings)"
    echo ""
  fi
fi

# Report blacklisted domains for user review
BLACKLIST="$CLAUDE_DIR/.cache/blocked-domains.txt"
if [[ -f "$BLACKLIST" ]]; then
  # Count non-comment, non-empty lines
  COUNT=$(grep -cvP '^\s*(#|$)' "$BLACKLIST" 2>/dev/null || echo 0)
  if (( COUNT > 0 )); then
    echo "Blacklisted domains ($COUNT) pending review:"
    grep -vP '^\s*(#|$)' "$BLACKLIST" | while IFS='|' read -r domain reason date; do
      domain=$(echo "$domain" | xargs)
      reason=$(echo "$reason" | xargs)
      date=$(echo "$date" | xargs)
      echo "  - $domain ($reason) [$date]"
    done
    echo "(Review at .claude/.cache/blocked-domains.txt)"
    echo ""
  fi
fi

echo "=== End Session Context ==="
