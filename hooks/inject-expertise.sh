#!/usr/bin/env bash
# Hook: SubagentStart
# Purpose: Auto-inject domain expertise into subagents based on their name
#
# When a subagent starts, this hook detects the domain from the agent name
# and prints the tips.md + expertise.yaml content so it's available in context.
#
# Matches: build|plan|improve|question|Explore
# Domains are auto-discovered from the experts/ directory.

set -euo pipefail

# Hooks run with cwd = project root (guaranteed by Claude Code).
# Using $PWD is robust regardless of symlink resolution of $0.
CLAUDE_DIR="$PWD/.claude"
EXPERTS_DIR="$CLAUDE_DIR/agents/experts"

# The agent name/type comes via stdin as JSON from Claude Code
INPUT=$(cat)
AGENT_INFO="$INPUT"

# Detect domain from agent name or prompt content
# Auto-discovers domains from experts/ directory
detect_domain() {
  local text="$1"
  for dir in "$EXPERTS_DIR"/*/; do
    [[ -d "$dir" ]] || continue
    local domain
    domain=$(basename "$dir")
    # Skip non-domain entries
    [[ -f "$EXPERTS_DIR/$domain/expertise.yaml" || -f "$EXPERTS_DIR/$domain/tips.md" ]] || continue
    if echo "$text" | grep -qi "$domain"; then
      echo "$domain"
      return 0
    fi
  done
  return 1
}

DOMAIN=$(detect_domain "$AGENT_INFO" || true)

if [[ -n "$DOMAIN" ]]; then
  # Inject tips first (compact operational facts — prevents repeated mistakes)
  if [[ -f "$EXPERTS_DIR/$DOMAIN/tips.md" ]]; then
    echo ""
    echo "=== $DOMAIN Quick Tips (read before doing anything) ==="
    cat "$EXPERTS_DIR/$DOMAIN/tips.md"
    echo "=== End $DOMAIN Tips ==="
  fi

  # Inject full expertise (deep domain knowledge)
  if [[ -f "$EXPERTS_DIR/$DOMAIN/expertise.yaml" ]]; then
    echo ""
    echo "=== $DOMAIN Domain Expertise (auto-injected) ==="
    cat "$EXPERTS_DIR/$DOMAIN/expertise.yaml"
    echo "=== End $DOMAIN Expertise ==="
    echo ""
  fi
fi
