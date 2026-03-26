# Claude Code Agent Teams — Teammate Workflow Reference

This document explains how agent teams work. It complements [template.md](template.md) (setup guide) with runtime behavior.

---

## Agent Roles

### Core Agents (always available)

| Agent | Access | Purpose |
|-------|--------|---------|
| `build-agent` | Full (Read/Write/Edit/Bash) | General implementation across all domains |
| `scout-agent` | Read-only | Codebase exploration and research |
| `review-agent` | Read-only + Task | Code review, quality gate, improvement suggestions |

### Domain Agents (per domain — auto-discovered from experts/ directory)

| Agent | Access | Model | Purpose |
|-------|--------|-------|---------|
| `{domain}-plan-agent` | Read-only | sonnet | Plans implementations, produces specs |
| `{domain}-build-agent` | Full | sonnet | Builds from specs, implements features |
| `{domain}-improve-agent` | Read + Edit | sonnet | Updates expertise.yaml with learnings |
| `{domain}-question-agent` | Read-only | haiku | Answers domain questions (fast, cheap) |

---

## Workflows

### `/do` — Single-Domain (Sequential)

```
User: /do "Add user registration endpoint"

┌─────────────────────────────────────────────────┐
│ /do dispatcher classifies:                       │
│   Domain: backend                                │
│   Pattern: A (Implementation)                    │
└──────────────────────┬──────────────────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Phase 1: Plan              │
        │  backend-plan-agent         │
        │  → produces spec at         │
        │    .cache/specs/backend/... │
        └──────────────┬──────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Phase 2: User Approval     │
        │  "Proceed with build?"      │
        │  [Yes] / [No, review first] │
        └──────────────┬──────────────┘
                       │ (user approves)
        ┌──────────────▼──────────────┐
        │  Phase 3: Build             │
        │  backend-build-agent        │
        │  → implements from spec     │
        │  → runs tests               │
        └──────────────┬──────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Phase 4: Review            │
        │  review-agent               │
        │  → quality assessment       │
        │  → tips suggestions         │
        │  → expertise suggestions    │
        │  → new agent suggestions    │
        └──────────────┬──────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Phase 5: Acknowledge       │
        │  "Apply suggestions?"       │
        │  [Yes] / [Skip]            │
        └──────────────┬──────────────┘
                       │ (user accepts)
        ┌──────────────▼──────────────┐
        │  Phase 6: Improve           │
        │  backend-improve-agent      │
        │  → updates expertise.yaml   │
        │  → uses REVIEW_FEEDBACK     │
        └─────────────────────────────┘
```

**Pattern B (Questions):** `{domain}-question-agent` answers directly. No plan/build/review.

**Pattern C (Simple):** `build-agent` runs lint/format/validate. No plan/review.

---

### `/do-teams` — Multi-Domain (Parallel)

```
User: /do-teams "Add auth endpoint with frontend login form and CI tests"

┌─────────────────────────────────────────────────┐
│ Team Lead classifies:                            │
│   Domains: backend + frontend + devops           │
│   Pattern: Implementation                        │
│   Team: myapp-user-auth                          │
└──────────────────────┬──────────────────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Step 1-3: Setup            │
        │  TeamCreate + TaskCreate    │
        │  Break into tasks with      │
        │  file ownership per domain  │
        └──────────────┬──────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Step 4: Spawn Specialists  │  ← All spawn in ONE message (parallel)
        │  ┌────────────────────────┐ │
        │  │ backend-specialist     │ │  Works on src/api/*.ts
        │  │ frontend-specialist   │ │  Works on src/ui/*.tsx
        │  │ devops-specialist      │ │  Works on .github/workflows/*.yml
        │  └────────────────────────┘ │
        └──────────────┬──────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Step 5: Monitor            │
        │  Teammates work, send msgs  │
        │  Lead resolves blockers     │
        │  TaskList tracks progress   │
        └──────────────┬──────────────┘
                       │ (all tasks complete)
        ┌──────────────▼──────────────┐
        │  Step 6a: Shut Down         │
        │  Specialists                │
        └──────────────┬──────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Step 6b: Review            │
        │  Cross-domain reviewer      │
        │  → integration issues       │
        │  → build/test validation    │
        │  → file ownership checks    │
        │  → tips suggestions         │
        │  → expertise suggestions    │
        │  → new agent suggestions    │
        └──────────────┬──────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Step 6c: Acknowledge       │
        │  "Apply suggestions?"       │
        │  [Yes] / [Skip]            │
        └──────────────┬──────────────┘
                       │ (user accepts)
        ┌──────────────▼──────────────┐
        │  Step 6d: Improve           │
        │  Spawn improve-agents per   │
        │  domain in parallel with    │
        │  REVIEW_FEEDBACK            │
        └──────────────┬──────────────┘
                       │
        ┌──────────────▼──────────────┐
        │  Step 6e: Shut down reviewer│
        │  Step 7: TeamDelete + report│
        └─────────────────────────────┘
```

---

### `/improve` — Expertise Maintenance (Standalone)

```
User: /improve backend

┌─────────────────────────────────────────────────┐
│ Check git log for recent changes                 │
│ Map changed files to domains                     │
│ Spawn backend-improve-agent                         │
│   → reviews git history                          │
│   → extracts learnings                           │
│   → updates expertise.yaml                       │
└─────────────────────────────────────────────────┘
```

No `REVIEW_FEEDBACK` in standalone mode — the improve-agent does its own analysis.

---

## How Expertise Flows

```
tips.md (quick operational facts) + expertise.yaml (deep knowledge)
       │
       ▼
inject-expertise.sh (SubagentStart hook) — tips first, then expertise
       │
       ▼ injects into every agent spawn
plan-agent / build-agent / question-agent
       │
       ▼ agents avoid repeated mistakes, make better decisions
build-agent produces code + updates tips.md if it discovers new facts
       │
       ▼
review-agent reviews quality + suggests tips + expertise + agents
       │
       ▼ user acknowledges
improve-agent updates tips.md + expertise.yaml with REVIEW_FEEDBACK
       │
       ▼ every agent self-reflects: what blocked me? tips.md still accurate?
       │
       ▼ loop: better tips → fewer wasted turns → better agents
```

---

## Communication Rules

### In `/do` (subagents)

- Agents are spawned via `Task` tool — they return results directly
- No inter-agent messaging needed
- /do dispatcher orchestrates sequentially

### In `/do-teams` (teammates)

- **Team lead** orchestrates via `SendMessage`, `TaskCreate`, `TaskUpdate`
- **Teammates** communicate via `SendMessage` to team lead or peers
- **Idle is normal** — teammates go idle between turns, wake on message
- **File ownership** is sacred — no two teammates touch the same file
- **Shutdown** is explicit — lead sends `shutdown_request`, teammate approves

---

## The Review Gate

The review-agent sits between build and improve in both `/do` and `/do-teams`. It produces:

1. **Quality Assessment**: Issues by severity (CRITICAL/HIGH/MEDIUM/LOW), positive aspects, recommendations
2. **Expertise Improvement Suggestions**: Non-obvious learnings worth capturing in expertise.yaml — each with domain, pattern name, reason, and exact suggested entry
3. **New Agent Suggestions**: Only when a genuine gap is found (prefer extending existing domains over creating new agents)

The user sees all three and decides whether to apply suggestions. This ensures:
- Human oversight on what gets captured in expertise
- No auto-pollution of expertise with trivial info
- Opportunity to course-correct agent behavior

---

## Self-Reflection

Every agent (build, review, improve) reflects before finishing:

- **What blocked me?** Wrong paths, missing env vars, unknown tool locations
- **Is tips.md still accurate?** Remove stale entries, fix wrong ones
- **1-2 lines feedback**: What to do differently next time

This output is included in the agent's report so the user sees it. Over time, self-reflection drives tips.md accuracy and reduces wasted turns.

---

## Security Hooks (Active During All Workflows)

| Hook | When | What |
|------|------|------|
| `scope-enforcement.sh` | PreToolUse | Blocks out-of-scope edits, gates network, verifies supply chain |
| `validate-intent.sh` | PostToolUse | Syntax checks on shell scripts |
| `detect-injection.sh` | PostToolUse | Checks fetched web content for prompt injection |
| `track-learnings.sh` | PostToolUse | Records which domains were touched |
| `inject-expertise.sh` | SubagentStart | Injects expertise.yaml into agents |
| `session-context.sh` | SessionStart | Reports domains, activity, blacklist status |

These run for **all agents** in **all workflows** — they are not optional.
