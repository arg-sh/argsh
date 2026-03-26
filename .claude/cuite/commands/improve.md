---
description: Review recent work and update domain expertise
argument-hint: [domain|all]
allowed-tools: Read, Glob, Grep, Edit, Bash, Task, TodoWrite
---

# `/improve` - Expertise Improvement Loop

Reviews recent git changes, extracts learnings, and updates expertise.yaml files.

Run after completing work to capture knowledge before it's lost.

## Usage

```
/improve           # Auto-detect domains from recent changes
/improve backend   # Improve only backend expertise
/improve all       # Improve all domains
```

## Step 1: Identify What Changed

```bash
git log --oneline -20 --name-only
```

Map changed files to domains using **dynamic discovery**:

1. **Read `domains.md`** (at `.claude/domains.md`): Primary source. Lists each domain with paths — match changed file paths against each domain's declared paths.

2. **Read `domain-map.conf`** (at `.claude/domain-map.conf`): Contains glob-pattern-to-domain mappings. Match each changed file path against these patterns.

3. **Check `session-domains.txt`** (at `.claude/.cache/session-domains.txt`): The `track-learnings.sh` hook writes domain breadcrumbs here during the session. Use this as a secondary signal for which domains were touched.

4. **Scan `experts/` directory** (at `.claude/agents/experts/`): Each subdirectory is a domain. Cross-reference changed files against each domain's declared paths in `expertise.yaml`.

If `$ARGUMENTS` specifies a domain, only process that domain.
If `$ARGUMENTS` is "all", process every domain found in `experts/`.
If empty, auto-detect from recent changes using the methods above.

## Step 2: For Each Affected Domain

Spawn the domain's improve-agent:

```
Task(
  subagent_type: "{domain}-improve-agent",
  prompt: |
    Review recent changes to the {domain} domain and update expertise.

    Recent commits affecting your domain:
    {git log output filtered to domain paths}

    Read the changed files and extract:
    1. New patterns worth documenting
    2. Gotchas or bugs that were discovered
    3. Build/test command changes
    4. Convention decisions made
    5. Things that were tried but didn't work

    Update .claude/agents/experts/{domain}/expertise.yaml with your findings.
    Be specific and actionable. Include exact commands and code examples.
    Remove any outdated entries you find.
)
```

Spawn multiple improve-agents in parallel if multiple domains changed.

## Step 3: Report

```markdown
## `/improve` - Complete

**Domains Updated:** {list}

### {Domain 1}
- Added: {new entries}
- Updated: {changed entries}
- Removed: {outdated entries}

### {Domain 2}
- ...

**Expertise files modified:**
- .claude/agents/experts/{domain}/expertise.yaml
```

## When to Run

- After completing a feature or bug fix
- After discovering a gotcha the hard way
- After a debugging session that revealed non-obvious behavior
- Periodically (weekly) to capture accumulated knowledge
- Before onboarding someone new to a domain
