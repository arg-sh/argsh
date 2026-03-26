---
description: Check and sync cuite framework with project configuration
argument-hint: [--apply]
allowed-tools: Read, Glob, Grep, Bash, Edit, Write, AskUserQuestion, TodoWrite
---

# `/cuite-sync` - Framework Sync Checker

Detects drift between the cuite subtree framework and your project-side configuration. Reports inconsistencies and optionally fixes them.

## When to Use

- After `bin/cuite pull` to check if project files need updating
- When domains.md, experts/, or domain-map.conf feel out of sync
- Periodically to verify framework health

## Step 1: Framework Health

### Symlink Integrity

Read the `LINKS` array from `.claude/cuite/bin/cuite` (lines 21-35) to know the expected symlinks. Then verify each one:

```bash
# For each expected symlink, check:
# 1. Does it exist?
# 2. Is it a symlink (not a regular file)?
# 3. Does it point to the correct target?
```

Report:
- OK: symlink exists and points correctly
- DRIFT: symlink points elsewhere
- MISSING: symlink doesn't exist
- WRONG TYPE: regular file where symlink expected

### Settings Hook Sync

Compare `.claude/settings.json` hooks section against `.claude/cuite/settings.json` hooks section.

```bash
# Requires jq
jq -S '.hooks // {}' .claude/settings.json
jq -S '.hooks // {}' .claude/cuite/settings.json
```

Report whether hooks match or differ.

### Upstream Changes

Check if the subtree has changes not yet pulled:

```bash
git log --oneline HEAD...cuite/main -- .claude/cuite/ 2>/dev/null | head -5
```

Report: "N commits behind upstream" or "Up to date".

## Step 2: Domain Consistency

### Cross-reference Three Sources

Build a unified view by reading all three domain sources:

| Source | How to Read |
|--------|-------------|
| `domains.md` | Parse `## {name}` headings from `.claude/domains.md` |
| `experts/` | List directories in `.claude/agents/experts/*/` that contain `expertise.yaml` or `tips.md` |
| `domain-map.conf` | Parse `pattern = domain` lines from `.claude/domain-map.conf` |

### Detect Issues

For each domain found in ANY source:

| Issue | Condition | Severity |
|-------|-----------|----------|
| **Ghost domain** | In `domains.md` but no `experts/` directory | HIGH |
| **Undocumented domain** | In `experts/` but not in `domains.md` | MEDIUM |
| **Unmapped domain** | In `domains.md` but no entries in `domain-map.conf` | MEDIUM |
| **Incomplete domain** | `experts/{domain}/` missing `tips.md` or `expertise.yaml` | HIGH |
| **Orphan mapping** | `domain-map.conf` references a domain not in `domains.md` or `experts/` | LOW |

### Agent File Check

For each domain in `experts/`, check if the 4 expected agent files exist:
- `{domain}-plan-agent.md`
- `{domain}-build-agent.md`
- `{domain}-improve-agent.md`
- `{domain}-question-agent.md`

Missing agent files are flagged as MEDIUM (agents work without them via template fallback, but are less effective).

## Step 3: Report

Present findings as a structured table:

```markdown
## /cuite-sync Report

### Framework Health
| Check | Status | Detail |
|-------|--------|--------|
| Symlinks | {OK/N broken} | {details} |
| Settings hooks | {In sync/Drifted} | {diff summary} |
| Upstream | {Up to date/N behind} | {latest commit} |

### Domain Consistency
| Domain | domains.md | experts/ | domain-map | agents | Issues |
|--------|-----------|----------|------------|--------|--------|
| {name} | {yes/no}  | {yes/no} | {yes/no}   | {N/4}  | {list} |

### Issues Found
| # | Severity | Issue | Fix |
|---|----------|-------|-----|
| 1 | HIGH | Ghost domain "foo" in domains.md | Create experts/foo/ with starter files |
| 2 | MEDIUM | Unmapped domain "bar" | Add path mappings to domain-map.conf |

### Summary
{N} issues found ({H} high, {M} medium, {L} low)
```

## Step 4: Apply Fixes

If `$ARGUMENTS` contains `--apply`, or ask the user:

```
AskUserQuestion: "Found {N} issues. Apply automatic fixes?"
Options: ["Yes, fix all (Recommended)", "Fix high-severity only", "No, just show the report"]
```

### Automatic Fixes

| Issue | Fix |
|-------|-----|
| Ghost domain | Create `experts/{domain}/` with starter `tips.md` + `expertise.yaml` (using info from `domains.md`) |
| Undocumented domain | Add `## {domain}` section to `domains.md` (read description from `expertise.yaml`) |
| Unmapped domain | Add path entries to `domain-map.conf` (derive from `domains.md` Paths field or `expertise.yaml` key_paths) |
| Incomplete domain | Create missing `tips.md` or `expertise.yaml` with starter content |
| Missing symlinks | Run equivalent of `bin/cuite link` |
| Drifted hooks | Offer to run `bin/cuite settings` |

### What Is NOT Auto-Fixed

- Orphan mappings (may be intentional — just flagged)
- Missing agent files (use `/cuite-init` or the blueprint to create these)
- Upstream changes (use `bin/cuite pull` manually)

## Step 5: Post-Fix Report

If fixes were applied:

```markdown
## Fixes Applied
- Created experts/{domain}/ with starter files
- Updated domains.md with {N} new entries
- Added {M} mappings to domain-map.conf

## Remaining Manual Steps
- Run `bin/cuite settings` to sync hooks
- Run `bin/cuite pull` to get upstream updates
- Run `/cuite-init` to generate missing agent files
```
