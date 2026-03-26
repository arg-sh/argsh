#!/usr/bin/env bash
# Hook: PreToolUse (scope-enforcement)
# Purpose: Enforce project boundaries, block destructive operations,
#          and gate outbound network access against the WebFetch whitelist.
#
# Matches: Edit|Write|Bash|WebFetch
# Blocks:
#   - File edits outside the project directory
#   - Known destructive bash patterns (rm -rf, force push, hard reset)
#   - Outbound network commands (curl, wget, nc, etc.) to non-whitelisted domains
#   - Remote script execution (curl|bash, eval, source <(wget)) - forces download-first review
# Verifies (review REQUIRED, red flags → block + blacklist):
#   - Package installs (npm, bun, cargo, pip, go, docker) - Claude must check before proceeding
# Informs:
#   - Which domain a file edit targets (context for Claude)

set -euo pipefail

# Hooks run with cwd = project root (guaranteed by Claude Code).
# Using $PWD is robust regardless of symlink resolution of $0.
PROJECT_DIR="$PWD"
SETTINGS_JSON="$PROJECT_DIR/.claude/settings.json"
BLACKLIST="$PROJECT_DIR/.claude/.cache/blocked-domains.txt"
DOMAIN_MAP="$PROJECT_DIR/.claude/domain-map.conf"

INPUT=$(cat)

# --- Allowed-domains whitelist (parsed from settings.json) ---
# Single source of truth: WebFetch(domain:...) entries in settings.json
load_allowed_domains() {
  if [[ ! -f "$SETTINGS_JSON" ]]; then
    return
  fi
  # Extract domains from WebFetch(domain:xxx) patterns
  grep -oP 'WebFetch\(domain:([^)]+)\)' "$SETTINGS_JSON" \
    | sed 's/WebFetch(domain:\(.*\))/\1/' \
    | sort -u
}

ALLOWED_DOMAINS=$(load_allowed_domains)

# Check if a hostname is in the whitelist
is_domain_allowed() {
  local host="$1"
  # Strip port if present
  host="${host%%:*}"
  # Strip trailing dot
  host="${host%.}"

  while IFS= read -r domain; do
    [[ -z "$domain" ]] && continue
    # Exact match or subdomain match (e.g. api.github.com matches github.com)
    if [[ "$host" == "$domain" || "$host" == *."$domain" ]]; then
      return 0
    fi
  done <<< "$ALLOWED_DOMAINS"
  return 1
}

# Check if a hostname is on the blacklist (agent-reported suspicious domains)
is_domain_blacklisted() {
  local host="$1"
  [[ ! -f "$BLACKLIST" ]] && return 1
  host="${host%%:*}"
  host="${host%.}"
  # Match first field (domain) in "domain | reason | date" format
  while IFS='|' read -r domain _reason _date; do
    domain=$(echo "$domain" | xargs)  # trim whitespace
    [[ -z "$domain" || "$domain" == \#* ]] && continue
    if [[ "$host" == "$domain" || "$host" == *."$domain" ]]; then
      return 0
    fi
  done < "$BLACKLIST"
  return 1
}

# Extract hostname from a URL or host:port string
extract_host() {
  local url="$1"
  # Strip protocol
  url="${url#*://}"
  # Strip path
  url="${url%%/*}"
  # Strip userinfo
  url="${url#*@}"
  # Strip port
  echo "${url%%:*}"
}

# Extract tool name - try jq first, fall back to grep
if command -v jq &>/dev/null; then
  TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null || true)
else
  TOOL_NAME=$(echo "$INPUT" | grep -oP '"tool_name"\s*:\s*"([^"]+)"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)
fi

# Map file path to domain using domain-map.conf
# Format: glob-pattern = domain-name (one per line, first match wins)
detect_domain_from_path() {
  local path="$1"

  # Always detect .claude/ as meta
  if [[ "$path" == *".claude/"* ]]; then
    echo "meta"
    return
  fi

  # Read domain-map.conf for project-specific path mappings
  if [[ -f "$DOMAIN_MAP" ]]; then
    while IFS='=' read -r pattern domain; do
      # Skip comments and empty lines
      [[ -z "$pattern" || "$pattern" == \#* ]] && continue
      pattern=$(echo "$pattern" | xargs)  # trim whitespace
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

case "$TOOL_NAME" in
  WebFetch)
    # Check blacklist for WebFetch URLs (even whitelisted domains can get blacklisted)
    if command -v jq &>/dev/null; then
      URL=$(echo "$INPUT" | jq -r '.tool_input.url // empty' 2>/dev/null || true)
    else
      URL=$(echo "$INPUT" | grep -oP '"url"\s*:\s*"([^"]+)"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)
    fi
    if [[ -n "$URL" ]]; then
      HOST=$(extract_host "$URL")
      if is_domain_blacklisted "$HOST"; then
        REASON=$(grep -i "^${HOST}" "$BLACKLIST" | head -1 | cut -d'|' -f2 | xargs)
        echo "BLOCK: $HOST is blacklisted (${REASON:-suspicious activity})"
        exit 0
      fi
      # Non-HTTPS red flag for WebFetch
      if [[ "$URL" == http://* ]] || [[ "$URL" == ftp://* ]] || [[ "$URL" == telnet://* ]]; then
        cat <<REDFLAG
WARNING: Non-HTTPS URL detected — this is a security red flag.
  URL: $URL

Plain HTTP transmits data unencrypted (MITM risk, no server verification).
You MUST acknowledge this risk before proceeding. Can you use https:// instead?
REDFLAG
      fi
    fi
    ;;

  Edit|Write)
    # Extract file path
    if command -v jq &>/dev/null; then
      FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null || true)
    else
      FILE_PATH=$(echo "$INPUT" | grep -oP '"file_path"\s*:\s*"([^"]+)"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)
    fi

    if [[ -z "$FILE_PATH" ]]; then
      exit 0
    fi

    # Resolve to absolute path for comparison
    REAL_PATH=$(realpath -m "$FILE_PATH" 2>/dev/null || echo "$FILE_PATH")

    # Block edits outside project directory
    if [[ "$REAL_PATH" != "$PROJECT_DIR"/* ]]; then
      echo "BLOCK: file is outside project scope: $FILE_PATH"
      exit 0
    fi

    # Inform which domain is being edited
    DOMAIN=$(detect_domain_from_path "$REAL_PATH")
    if [[ -n "$DOMAIN" ]]; then
      echo "Scope: editing $DOMAIN domain"
    fi
    ;;

  Bash)
    # Extract command
    if command -v jq &>/dev/null; then
      COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null || true)
    else
      COMMAND=$(echo "$INPUT" | grep -oP '"command"\s*:\s*"([^"]+)"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)
    fi

    if [[ -z "$COMMAND" ]]; then
      exit 0
    fi

    # Block destructive rm patterns (outside project)
    if echo "$COMMAND" | grep -qP 'rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|--recursive)\s+(/|~|\$HOME|\.\.)'; then
      echo "BLOCK: destructive rm outside project scope"
      exit 0
    fi

    # Block force push
    if echo "$COMMAND" | grep -qP 'git\s+push\s+.*--force\b'; then
      echo "BLOCK: git push --force requires explicit user approval"
      exit 0
    fi

    # Block hard reset
    if echo "$COMMAND" | grep -qP 'git\s+reset\s+--hard'; then
      echo "BLOCK: git reset --hard requires explicit user approval"
      exit 0
    fi

    # --- Supply chain verification ---
    # All remote code must be reviewed before execution. Burn tokens, not trust.
    # Pattern: BLOCK unsafe execution → force download-first → Claude reads code → proceed or blacklist.

    # Remote script execution (curl|bash, source <(wget ...), eval, etc.)
    # BLOCK the pipe pattern - force download → read → execute as separate steps.
    if echo "$COMMAND" | grep -qP '(curl|wget)\s.*\|\s*(ba)?sh' || \
       echo "$COMMAND" | grep -qP '(ba)?sh\s+<\(\s*(curl|wget)' || \
       echo "$COMMAND" | grep -qP 'eval\s.*\$\(\s*(curl|wget)' || \
       echo "$COMMAND" | grep -qP '(source|\.)\s+<\(\s*(curl|wget)'; then
      SCRIPT_URL=$(echo "$COMMAND" | grep -oP 'https?://[^\s"'"'"'|)]+' | head -1 || true)
      SCRIPT_HOST=$(extract_host "$SCRIPT_URL" 2>/dev/null || true)
      cat <<BLOCK_MSG
BLOCK: remote script execution without prior review
You MUST review remote code before executing. Use this safe pattern instead:
  1. curl -fsSL "${SCRIPT_URL:-URL}" -o /tmp/install-script.sh
  2. Read /tmp/install-script.sh (use the Read tool - review the FULL content)
  3. Verify: official source? what does it download? sudo? modifies PATH? phones home?
  4. Only if clean: bash /tmp/install-script.sh
  5. If red flags found, add to blacklist:
     echo "${SCRIPT_HOST:-unknown} | suspicious script${SCRIPT_URL:+ ($SCRIPT_URL)} | \$(date +%Y-%m-%d)" >> .claude/.cache/blocked-domains.txt
BLOCK_MSG
      exit 0
    fi

    # --- Package manager installs ---
    # Don't block, but Claude MUST actually check before proceeding.
    # The instructions tell Claude to fetch and read metadata - this costs tokens but catches attacks.

    # npm install / npm i / npx / yarn add / bun add
    # Skip lockfile installs: npm ci, bun install (no args) — deterministic, no new packages
    if echo "$COMMAND" | grep -qP '\bnpm\s+ci\b'; then
      : # npm ci installs from lockfile only — safe, skip supply chain check
    elif echo "$COMMAND" | grep -qP '\b(npm\s+(install|i)\s|npx\s|yarn\s+add\s|bun\s+add\s)'; then
      PKG=$(echo "$COMMAND" | grep -oP '(npm\s+(install|i)\s+|npx\s+|yarn\s+add\s+|bun\s+add\s+)\K[^\s-][^\s]*' | head -1 || true)
      if [[ -n "$PKG" && "$PKG" != "." && "$PKG" != ".." ]]; then
        cat <<VERIFY
SUPPLY_CHAIN_CHECK: npm/bun package${PKG:+ "$PKG"} - review REQUIRED before install.
You MUST check before proceeding (do not skip):
  1. WebFetch https://registry.npmjs.org/$PKG - check downloads, publish date, maintainer
  2. Look for "scripts": {"postinstall":...} - runs arbitrary code on install
  3. Check: is the name a known typosquat? (e.g. lodash vs 1odash, express vs expres)
  4. Is the repo URL legitimate and active?
If ANY red flag → DO NOT install. Block + blacklist:
  echo "npm:$PKG | suspicious package | \$(date +%Y-%m-%d)" >> .claude/.cache/blocked-domains.txt
VERIFY
      fi
    fi

    # cargo install / cargo add
    if echo "$COMMAND" | grep -qP '\bcargo\s+(install|add)\s'; then
      CRATE=$(echo "$COMMAND" | grep -oP 'cargo\s+(install|add)\s+\K[^\s-][^\s]*' | head -1 || true)
      if [[ -n "$CRATE" ]]; then
        cat <<VERIFY
SUPPLY_CHAIN_CHECK: crate${CRATE:+ "$CRATE"} - review REQUIRED before install.
You MUST check before proceeding (do not skip):
  1. WebFetch https://crates.io/api/v1/crates/$CRATE - check downloads, recent version, repo
  2. Check if it has a build.rs (runs arbitrary code at compile time)
  3. Check proc-macros - they execute at compile time too
  4. Is the author/org known? When was last publish?
If ANY red flag → DO NOT install. Block + blacklist:
  echo "crate:$CRATE | suspicious crate | \$(date +%Y-%m-%d)" >> .claude/.cache/blocked-domains.txt
VERIFY
      fi
    fi

    # pip install
    # Skip lockfile installs: pip install -r requirements.txt (deterministic, locked deps)
    if echo "$COMMAND" | grep -qP '\bpip3?\s+install\s+-r\s'; then
      : # pip install -r installs from requirements file — safe, skip supply chain check
    elif echo "$COMMAND" | grep -qP '\bpip3?\s+install\s'; then
      PKG=$(echo "$COMMAND" | grep -oP 'pip3?\s+install\s+\K[^\s-][^\s]*' | head -1 || true)
      if [[ -n "$PKG" ]]; then
        cat <<VERIFY
SUPPLY_CHAIN_CHECK: pip package${PKG:+ "$PKG"} - review REQUIRED before install.
You MUST check before proceeding (do not skip):
  1. WebFetch https://pypi.org/pypi/$PKG/json - check downloads, maintainer, repo
  2. Does it have a setup.py with custom install commands?
  3. Typosquat check: is the spelling exactly right? (requests vs requsets, urllib3 vs urlib3)
  4. When was last release? Is the project maintained?
If ANY red flag → DO NOT install. Block + blacklist:
  echo "pip:$PKG | suspicious package | \$(date +%Y-%m-%d)" >> .claude/.cache/blocked-domains.txt
VERIFY
      fi
    fi

    # docker pull / docker run
    if echo "$COMMAND" | grep -qP '\bdocker\s+(pull|run)\s'; then
      IMAGE=$(echo "$COMMAND" | grep -oP 'docker\s+(pull|run)\s+.*?\K[a-zA-Z0-9][-a-zA-Z0-9_./:]*' | head -1 || true)
      # Only check non-official images (contain slash or registry domain)
      if [[ -n "$IMAGE" && ("$IMAGE" == */* || "$IMAGE" == *"."*) ]]; then
        cat <<VERIFY
SUPPLY_CHAIN_CHECK: Docker image${IMAGE:+ "$IMAGE"} - review REQUIRED before pull/run.
You MUST check before proceeding (do not skip):
  1. Is this from a trusted registry? (docker.io library/*, ghcr.io, gcr.io)
  2. Check the image repo and Dockerfile source
  3. Is the publisher verified on Docker Hub?
  4. Prefer pinning by digest (@sha256:...) over mutable tags (:latest)
If ANY red flag → DO NOT pull/run. Block + blacklist:
  echo "docker:$IMAGE | untrusted image | \$(date +%Y-%m-%d)" >> .claude/.cache/blocked-domains.txt
VERIFY
      fi
    fi

    # go install (can run arbitrary code via go generate, init())
    if echo "$COMMAND" | grep -qP '\bgo\s+install\s'; then
      MOD=$(echo "$COMMAND" | grep -oP 'go\s+install\s+\K[^\s]+' | head -1 || true)
      if [[ -n "$MOD" ]]; then
        cat <<VERIFY
SUPPLY_CHAIN_CHECK: Go module${MOD:+ "$MOD"} - review REQUIRED before install.
You MUST check before proceeding (do not skip):
  1. WebFetch the module page on pkg.go.dev
  2. Check for go:generate directives and init() functions
  3. Is the import path from a known org? (github.com/golang, google, etc.)
  4. Check stars, last commit, and maintainer activity
If ANY red flag → DO NOT install. Block + blacklist:
  echo "go:$MOD | suspicious module | \$(date +%Y-%m-%d)" >> .claude/.cache/blocked-domains.txt
VERIFY
      fi
    fi

    # --- Outbound network egress check ---
    # Prevent prompt-injection bypass: WebFetch whitelist is useless if
    # Bash can curl/wget/nc to arbitrary hosts.
    # Covers: curl, wget, nc/ncat/netcat, httpie, python requests, node fetch
    # HTTPie pattern: `http GET/POST/PUT/DELETE/PATCH ...` (not bare "http" in strings)
    NETWORK_TOOLS='curl|wget|nc\b|ncat|netcat|http\s+(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)'

    if echo "$COMMAND" | grep -qPi "\b($NETWORK_TOOLS)"; then
      # Extract URLs/hosts from the command
      # Match http(s)://host, or bare host arguments after network tool names
      URLS=$(echo "$COMMAND" | grep -oP 'https?://[^\s"'"'"']+' || true)

      # Also catch bare-host patterns like: curl -X POST api.evil.com
      BARE_HOSTS=$(echo "$COMMAND" | grep -oP "(?<=\s)([a-zA-Z0-9][-a-zA-Z0-9]*\.)+[a-zA-Z]{2,}(?=[/\s:\"']|$)" || true)

      ALL_HOSTS=""
      for url in $URLS; do
        ALL_HOSTS+="$(extract_host "$url")"$'\n'
      done
      for host in $BARE_HOSTS; do
        ALL_HOSTS+="$host"$'\n'
      done

      # --- Non-HTTPS protocol red flag ---
      # Plain HTTP and non-HTTPS protocols (ftp://, telnet://, etc.) are red flags.
      # User must acknowledge the risk before proceeding.
      HTTP_URLS=$(echo "$COMMAND" | grep -oP 'http://[^\s"'"'"']+' || true)
      NON_HTTPS=$(echo "$COMMAND" | grep -oP '(ftp|telnet|gopher|ws)://[^\s"'"'"']+' || true)
      if [[ -n "$HTTP_URLS" || -n "$NON_HTTPS" ]]; then
        FLAGGED="${HTTP_URLS}${NON_HTTPS:+$'\n'$NON_HTTPS}"
        cat <<REDFLAG
WARNING: Non-HTTPS connection detected — this is a security red flag.
URLs using plaintext protocols (no TLS/SSL):
$(echo "$FLAGGED" | sed 's/^/  - /')

Plain HTTP and non-HTTPS protocols transmit data unencrypted. This means:
  - Credentials, tokens, and data can be intercepted (MITM)
  - Response content can be tampered with in transit
  - No server identity verification

You MUST acknowledge this risk before proceeding:
  - Is there a legitimate reason HTTPS is not available?
  - Can you use the HTTPS equivalent instead?
  - If you must proceed, confirm with the user that they accept the risk.
REDFLAG
      fi

      # Check each extracted host against blacklist first, then whitelist
      while IFS= read -r host; do
        [[ -z "$host" ]] && continue
        if is_domain_blacklisted "$host"; then
          REASON=$(grep -i "^${host}" "$BLACKLIST" | head -1 | cut -d'|' -f2 | xargs)
          echo "BLOCK: $host is blacklisted (${REASON:-suspicious activity})"
          exit 0
        fi
        if ! is_domain_allowed "$host"; then
          echo "BLOCK: outbound network to non-whitelisted domain: $host"
          echo "Allowed domains: ${ALLOWED_DOMAINS//$'\n'/, }"
          exit 0
        fi
      done <<< "$ALL_HOSTS"
    fi
    ;;
esac

exit 0
