# Claude Code Agent Teams — Setup & Usage Guide

## Quick Start: Add to a New Project

### 1. Copy the blueprint

Copy `.claude/agents/experts/agent-teams-blueprint.md` into your new project's `.claude/` directory (create it if it doesn't exist):

```bash
mkdir -p /path/to/new-project/.claude/agents/experts
cp .claude/agents/experts/agent-teams-blueprint.md /path/to/new-project/.claude/agents/experts/
```

### 2. Enable agent teams

Create `.claude/settings.json` (shared) or `.claude/settings.local.json` (personal):

```json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  }
}
```

### 3. Ask Claude to generate the structure

Open Claude Code in the new project and say:

```
Analyze this project and create an agent team structure following
.claude/agents/experts/agent-teams-blueprint.md
```

Claude will:
1. Scan your project for languages, frameworks, and directory structure
2. Identify 3-7 expert domains based on code boundaries
3. Create the full agent/team structure:
   - 3 core agents (build, scout, review)
   - 5 templates (base, plan, build, improve, question)
   - Expertise YAML per domain (the knowledge base)
   - 4 agents per domain (plan/build/improve/question)
   - `/do` orchestrator command
   - `/do-teams` parallel execution command
   - `/improve` expertise maintenance command
   - Agent registry and hooks

### 4. Verify it worked

```bash
find .claude -type f | sort
```

You should see ~40 files across agents/, commands/, hooks/.

---

## Usage

### Commands

#### `/do <requirement>` — Single-domain tasks

The universal entry point. Classifies your request, picks the right domain, and orchestrates the plan-build-improve cycle.

**Implementation tasks** (fix/add/create/update):
```
/do "Add a rate limiter to the API"
/do "Fix the broken migration for users table"
/do "Refactor the auth module to use JWT"
```

Flow: `plan-agent` → **you approve** → `build-agent` → `review-agent` → **you acknowledge** → `improve-agent`

**Questions** (how/what/why/explain):
```
/do "How does the caching layer work?"
/do "What's the database schema for orders?"
/do "Explain the deployment pipeline"
```

Flow: `question-agent` (haiku, fast, cheap) → answer

**Simple tasks** (lint/format/validate):
```
/do "Lint the API module"
/do "Validate all YAML manifests"
```

Flow: `build-agent` → result

#### `/do-teams <requirement>` — Multi-domain parallel work

Spawns multiple independent Claude instances that work simultaneously. Use for cross-cutting changes.

```
/do-teams "Add user authentication with frontend login page, backend API, and tests"
/do-teams "Refactor the database schema and update all API endpoints"
/do-teams "Review security across the entire codebase"
```

Flow: Team lead creates team → spawns domain specialists → they work in parallel → lead reports results

**When to use which:**

| Scenario | Command | Why |
|----------|---------|-----|
| One thing in one domain | `/do` | Cheaper, sequential is fine |
| Question about code | `/do` | Single agent answers |
| Feature spanning 2+ domains | `/do-teams` | Parallel is faster |
| Large refactoring | `/do-teams` | Multiple specialists needed |
| Code review / audit | `/do-teams` | Independent perspectives |

#### `/improve [domain|all]` — Update expertise

Reviews recent work and updates the domain expertise files with learnings.

```
/improve           # Auto-detect from recent git changes
/improve rust      # Only the rust domain
/improve all       # All domains
```

**When to run:**
- After finishing a feature or bug fix
- After a debugging session that revealed something non-obvious
- After discovering a gotcha the hard way
- Periodically (weekly) to sweep for accumulated knowledge

---

### Direct Agent Usage

You can invoke any agent directly in conversation:

```
Use the backend-plan-agent to plan adding a new auth endpoint
```

```
Use the scout-agent to find all files that import the auth module
```

```
Use the review-agent to review my changes in the last 3 commits
```

**Available agents:**

| Agent | Access | Purpose |
|-------|--------|---------|
| `build-agent` | Full | General implementation |
| `scout-agent` | Read-only | Codebase exploration |
| `review-agent` | Read-only | Code review |
| `{domain}-plan-agent` | Read-only | Plan implementation |
| `{domain}-build-agent` | Full | Build from spec |
| `{domain}-improve-agent` | Edit expertise | Update knowledge |
| `{domain}-question-agent` | Read-only (haiku) | Answer questions |

---

## How Auto-Improvement Works

### Three loops run automatically:

**1. Session hooks (passive)**

- **SessionStart**: Shows available domains, recent activity, and pending blacklist entries for review
- **SubagentStart**: Injects matching `expertise.yaml` into any agent that spawns
- **PreToolUse** (`scope-enforcement.sh`): Enforces project boundaries, gates outbound network access against domain whitelist, blocks unsafe remote code execution (forces download-first review), triggers supply chain verification for package installs (npm, cargo, pip, go, docker)
- **PostToolUse** (`track-learnings.sh`): Tracks which domains are touched during a session
- **PostToolUse** (`validate-intent.sh`): Runs syntax checks on edited shell scripts, logs edits by domain
- **PostToolUse** (`detect-injection.sh`): After WebFetch, instructs Claude to review fetched content for prompt injection — suspicious domains get added to the blacklist

**2. Build cycle (via `/do` and `/do-teams`)**

- After every successful build, the `review-agent` runs automatically
- Produces quality assessment + expertise improvement suggestions + new agent suggestions
- User acknowledges suggestions before they're applied
- If accepted, the `improve-agent` runs with review feedback to update expertise

**3. Manual sweep (`/improve`)**
- Reviews recent git history, finds patterns worth documenting
- Updates expertise.yaml files with new knowledge
- Removes outdated entries

### The flywheel

```
You work on code
  → hooks track what domains were touched
  → /do runs review-agent after builds
  → review-agent produces quality report + improvement suggestions
  → user acknowledges suggestions
  → improve-agent updates expertise.yaml with review feedback
  → next agent spawn gets injected with better expertise
  → agents make better decisions
  → repeat
```

### The security loop

```
Agent fetches web content
  → detect-injection.sh reminds Claude to check for prompt injection
  → Claude reviews content for red flags
  → If suspicious → domain added to .claude/.cache/blocked-domains.txt
  → scope-enforcement.sh blocks future access (WebFetch + Bash network)
  → session-context.sh reports blacklist entries on next session start
  → User reviews and confirms/clears entries
```

---

## Customizing for Your Project

### Adding a new domain

1. Create the directory:
   ```bash
   mkdir -p .claude/agents/experts/my-domain/
   ```

2. Create `expertise.yaml` with your domain knowledge:
   ```yaml
   domain: my-domain
   version: "1.0"
   description: One-line summary

   overview: |
     What this domain covers and why it matters.

   key_paths:
     source: path/to/source/
     tests: path/to/tests/

   testing:
     framework: "jest | pytest | cargo test | etc"
     run_command: "npm test"

   coding_standards:
     - "Standard 1"
     - "Standard 2"

   critical_patterns:
     pattern_name: |
       The hard-won lesson. Be specific.
       Include exact commands and code examples.

   build:
     command: "npm run build"
     test: "npm test"
     lint: "npm run lint"
   ```

3. Create the 4 agents (copy from templates, replace `{domain}` with your domain name):
   ```bash
   cp .claude/agents/templates/plan-agent.md    .claude/agents/experts/my-domain/my-domain-plan-agent.md
   cp .claude/agents/templates/build-agent.md   .claude/agents/experts/my-domain/my-domain-build-agent.md
   cp .claude/agents/templates/improve-agent.md .claude/agents/experts/my-domain/my-domain-improve-agent.md
   cp .claude/agents/templates/question-agent.md .claude/agents/experts/my-domain/my-domain-question-agent.md
   ```
   Then edit each file: update `name`, `expertDomain`, paths, and commands.

4. Add domain keywords to `/do` command classification in `.claude/commands/do.md`

5. Add the 4 new agents to `.claude/agents/agent-registry.json`

6. Add path-to-domain mappings in `.claude/domain-map.conf`:
   ```
   *path/to/source/* = my-domain
   ```

7. Create the spec cache directory:
   ```bash
   mkdir -p .claude/.cache/specs/my-domain/
   ```

### Removing a domain

Delete the domain directory, remove from registry, remove from `/do` classification, remove from hooks.

### Tuning agent models

In each agent's frontmatter:
- `model: haiku` — Fast, cheap. Good for question-agents and simple lookups.
- `model: sonnet` — Balanced. Default for plan/build/improve agents.
- `model: opus` — Most capable. Reserve for team leads or complex orchestration.

### Writing good expertise

The expertise.yaml files are the **most important files** in the entire system. Better expertise = better agents = better code. Prioritize:

- **Hard-won lessons** — Things that caused bugs or cost hours. "useEffect cleanup runs before re-render, not on unmount" is worth 10 generic coding standards.
- **Exact commands** — Copy-pasteable, not pseudocode. `npm test -- --project=api`, not "run the tests".
- **Non-obvious behavior** — Things a competent developer wouldn't guess. "Middleware runs before route params are parsed" saves hours.
- **Convention decisions** — Why this approach over alternatives. Prevents agents from reinventing.
- **Anti-patterns** — What NOT to do and why. Just as valuable as positive patterns.

**Bad expertise entry:**
```yaml
testing: "Make sure to test your code"
```

**Good expertise entry:**
```yaml
critical_patterns:
  async_error_handling: |
    CRITICAL: Express error middleware requires 4 parameters (err, req, res, next).
    Missing any parameter makes Express skip the handler silently.
    Async route errors must be caught and passed to next() explicitly.
    This caused intermittent 500 errors in production for 2 weeks.
```

---

## File Structure Reference

```
.claude/
  settings.json                      # Project-level: permissions, hooks, domain whitelist
  settings.local.json                # User-level: env flags, personal permissions (gitignored)
  domain-map.conf                    # Path-to-domain mappings for hooks
  template.md                        # This guide
  teammates.md                       # Runtime workflow reference
  commands/
    do.md                            # /do — single-domain orchestrator
    do-teams.md                      # /do-teams — parallel team orchestrator
    improve.md                       # /improve — expertise maintenance
  hooks/
    session-context.sh               # SessionStart: domains, activity, blacklist report
    inject-expertise.sh              # SubagentStart: inject tips.md + expertise.yaml
    scope-enforcement.sh             # PreToolUse: project boundaries, network egress,
                                     #   supply chain verification, remote code review
    track-learnings.sh               # PostToolUse: track touched domains
    validate-intent.sh               # PostToolUse: syntax checks, edit logging
    detect-injection.sh              # PostToolUse: prompt injection detection for WebFetch
  agents/
    build-agent.md                   # Core: full write access
    scout-agent.md                   # Core: read-only exploration
    review-agent.md                  # Core: read-only code review
    agent-registry.json              # Index of all agents
    templates/                       # Copy these to create new domain agents
      base-agent.md
      plan-agent.md
      build-agent.md
      improve-agent.md
      question-agent.md
    experts/
      agent-teams-blueprint.md       # The meta-guide for Claude to generate this
      {domain}/                      # One per domain (e.g., backend, frontend, devops)
        tips.md                      # Quick operational facts (injected first)
        expertise.yaml               # Domain knowledge base (THE key file)
        {domain}-plan-agent.md       # Read-only, creates specs
        {domain}-build-agent.md      # Full access, builds from specs
        {domain}-improve-agent.md    # Updates expertise with learnings
        {domain}-question-agent.md   # Answers questions (haiku)
  .cache/
    specs/{domain}/                  # Plan-agent outputs land here
    session-domains.txt              # Tracks touched domains per session
    session-edits.log                # Edit timestamps by domain
    blocked-domains.txt              # Blacklisted domains (agent-reported, user-reviewed)
```

---

## Cost Guide

| Action | Model | Approx. Cost |
|--------|-------|-------------|
| `/do` question | haiku | ~$0.01 |
| `/do` implementation (plan+build+improve) | sonnet x3 | ~$0.10-0.30 |
| `/do-teams` with 3 teammates | sonnet x3 parallel | ~$0.30-0.80 |
| `/do-teams` with 5 teammates | sonnet x5 parallel | ~$0.50-1.50 |

Question-agents use haiku (10x cheaper). Only reach for `/do-teams` when the parallel speedup justifies the cost.

---

## Troubleshooting

**"Agent teams not available"**
→ Ensure `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` is in settings or env.

**Hooks not firing**
→ Check `settings.local.json` has the `hooks` section. Run `bash .claude/hooks/session-context.sh` manually to test.

**Agent doesn't know about my domain**
→ The SubagentStart hook matches by agent name. Ensure your agent name contains the domain keyword (e.g., `rust-build-agent` matches "rust").

**Expertise not injected**
→ Check that `expertise.yaml` exists at `.claude/agents/experts/{domain}/expertise.yaml`. Hooks auto-discover domains from the `experts/` directory.

**`/do` picks wrong domain**
→ Edit `.claude/commands/do.md` classification section to add more keywords for your domain.

**`/improve` doesn't find changes**
→ It checks `git log`. If you haven't committed, there's nothing to review. Commit first, then `/improve`.
