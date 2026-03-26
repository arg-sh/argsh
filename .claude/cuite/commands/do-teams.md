---
description: Team-based parallel execution with agent teams coordination
argument-hint: <requirement>
allowed-tools: Read, Glob, Grep, Task, SendMessage, TeamCreate, TeamDelete, TaskCreate, TaskUpdate, TaskList, AskUserQuestion, TodoWrite
---

# `/do-teams` - Team-Based Parallel Execution

Spawns a team of specialist agents that work in parallel on multi-domain tasks. Each teammate operates independently with its own context window, communicating through messages and a shared task list.

**Requires:** `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` in settings.

**Implementation Pattern:**
```
TeamCreate → initialize team workspace
     ↓
TaskCreate → break work into domain-scoped tasks
     ↓
Spawn specialists → one per domain
     ↓
[domain-a-build]  [domain-b-build]  [domain-c-build]
     ↓                 ↓                 ↓
          ↓↓↓ work in parallel ↓↓↓
     ↓
security-agent → security review (can ask user)
     ↓                  ↑
     ↓         fix loop (max 3x)
     ↓
Shut down specialists
     ↓
review-agent → quality check + objective validation
     ↓                  ↑
     ↓         fix loop (max 3x, re-spawns specialists)
     ↓
improve-agent → updates expertise per domain
     ↓
TeamDelete → clean up team workspace
```

**Council Pattern:**
```
Spawn analysts → one per domain (read-only)
     ↓
[domain-a-analyst]  [domain-b-analyst]  [domain-c-analyst]
     ↓                   ↓                   ↓
          ↓↓↓ analyze in parallel ↓↓↓
     ↓
Team lead → synthesize findings into unified report
```

## CRITICAL: You Are the Team Lead

You orchestrate. Teammates execute. You MUST NOT do implementation work directly.

**Your responsibilities:**
1. Create the team
2. Analyze the requirement and break it into tasks
3. Spawn the right teammates
4. Assign tasks with clear file ownership
5. Monitor progress via task list
6. Resolve conflicts and blockers
7. Spawn a reviewer to cross-validate all changes after specialists finish
8. Synthesize results and shut down the team

**You MUST NOT:**
- Read/write/edit files (teammates do this)
- Make implementation decisions (teammates decide within their domain)
- Message teammates unnecessarily (trust them to work autonomously)

---

## Step 1: Parse and Classify

Extract requirement from `$ARGUMENTS`. Determine which coordination pattern:

### Implementation Pattern
**Trigger:** "implement", "add", "create", "build", "fix", "update", "refactor"
- Spawns domain specialists as teammates
- Each teammate owns specific files
- Shared task list for coordination

### Council Pattern
**Trigger:** "analyze", "research", "review", "assess", "compare", "audit"
- Spawns domain experts for independent analysis
- Each provides perspective from their domain
- Lead synthesizes findings

---

## Step 2: Identify Domains and Teammates

### Dynamic Domain Discovery

Domains are discovered at runtime — not hardcoded. To identify which domains are involved:

1. **Read `domains.md`** (at `.claude/domains.md`): Primary source. Lists each domain with description, keywords, paths, and build commands. Match the user's requirement against each domain's keywords and paths.

2. **Read `domain-map.conf`** (at `.claude/domain-map.conf`): Maps file path glob patterns to domain names. Use this to match specific file paths mentioned in the requirement.

3. **Scan `experts/` directory** (at `.claude/agents/experts/`): Each subdirectory is a domain. If `domains.md` is missing or incomplete, check each domain's `expertise.yaml` for additional context.

For each matched domain, the specialist teammate is named `{domain}-specialist` (implementation pattern) or `{domain}-analyst` (council pattern).

### Cross-Domain Detection

If the requirement spans multiple domains, spawn one teammate per domain.

**Example:** "Add auth with API and frontend"
→ Domains: backend (API endpoint), frontend (login form)
→ Teammates: backend-specialist, frontend-specialist

---

## Step 3: Create Team and Tasks

### 3a: Create the Team

```
TeamCreate(
  team_name: "{project}-{slug}",
  description: "{brief description of the work}"
)
```

Use a short slug derived from the task (e.g., `myapp-user-auth`, `webapp-test-fix`). If no project name is obvious, just use `{slug}` (e.g., `user-auth`).

### 3b: Break Down Into Tasks

Create tasks with **clear file ownership** and **dependencies**.

```
TaskCreate(
  title: "Implement {component}",
  description: "Details of what to build...",
  owner: "{specialist-name}"  # optional: assign at creation or let teammates claim
)
```

**Task sizing:** 5-6 tasks per teammate is the sweet spot.

**Dependency example:**
```
TaskCreate(title: "Create database schema", ...)         # task-1
TaskCreate(title: "Build API endpoint", blocked_by: [1]) # task-2 waits for task-1
TaskCreate(title: "Write tests", blocked_by: [2])        # task-3 waits for task-2
```

### 3c: File Ownership Rules

**CRITICAL:** No two teammates may modify the same file.

Assign file paths explicitly in the task description:
```
TaskCreate(
  title: "Implement user registration endpoint",
  description: |
    FILE OWNERSHIP: You own these paths exclusively:
    - src/api/auth/register.ts
    - src/api/auth/register.test.ts

    Do NOT modify any files outside your ownership.
    ...
)
```

---

## Step 4: Spawn Teammates

### Implementation Pattern

Spawn domain specialists using **agent definitions** for expertise:

```
Task(
  subagent_type: "general-purpose",
  team_name: "{project}-{slug}",
  name: "backend-specialist",
  prompt: |
    You are a backend specialist working on: {requirement}

    EXPERTISE: Read .claude/agents/experts/backend/expertise.yaml for domain knowledge.

    YOUR FILE OWNERSHIP:
    - src/api/{specific files}

    Check TaskList for your assigned tasks. Claim unassigned tasks in your domain.
    When done with each task, mark it completed via TaskUpdate.
    When all your tasks are done, notify the team lead.
)
```

```
Task(
  subagent_type: "general-purpose",
  team_name: "{project}-{slug}",
  name: "frontend-specialist",
  prompt: |
    You are a frontend specialist working on: {requirement}

    EXPERTISE: Read .claude/agents/experts/frontend/expertise.yaml for domain knowledge.

    YOUR FILE OWNERSHIP:
    - src/ui/{specific files}

    Check TaskList for your assigned tasks. Work autonomously.
)
```

**Spawn all teammates in a SINGLE message** for parallel execution.

### Council Pattern

Spawn read-only analysts:

```
Task(
  subagent_type: "general-purpose",
  team_name: "{project}-{slug}",
  name: "backend-analyst",
  prompt: |
    You are a backend domain expert analyzing: {requirement}

    EXPERTISE: Read .claude/agents/experts/backend/expertise.yaml

    Provide your analysis from the backend perspective:
    - Impact on API endpoints and data models
    - Test implications
    - Performance and security concerns

    Send your findings to the team lead when done.
)
```

---

## Step 5: Monitor and Coordinate

### Wait for Teammates

Teammates work autonomously. Messages arrive automatically. You do NOT need to poll.

### Handle Blockers

If a teammate reports a blocker:
1. Check if another teammate can help
2. Use SendMessage to coordinate between teammates
3. If unresolvable, create new tasks or adjust assignments

### Track Progress

Use `TaskList` periodically to see overall progress.

### Resolve Conflicts

If teammates need to coordinate on shared boundaries:
```
SendMessage(
  type: "message",
  recipient: "frontend-specialist",
  content: "The backend-specialist has finalized the auth API schema. Use POST /api/auth/register with the payload defined in src/api/auth/types.ts.",
  summary: "Coordinate API contract"
)
```

### Peer Communication (opt-in)

By default, all specialist communication flows through you (the team lead). This is the safest pattern — it contains errors, prevents message loops, and keeps you in control of coordination.

However, for **tightly-coupled domain boundaries** (e.g., backend defines an API contract that frontend must consume), you can enable **one-shot peer messaging** between specific teammate pairs. This saves a relay hop through the lead and reduces your context pollution.

**When to enable peer messaging:**

- Two specialists share a contract boundary (API schema, shared types, config format)
- One specialist produces output another needs to consume immediately
- The information is factual (a file path, a type definition) — not a decision that needs lead oversight

**When NOT to enable it:**

- Tasks are independent (no shared boundary)
- The coordination requires a decision (the lead should decide)
- More than 2 specialists need the same information (use the lead as relay, or the lead can broadcast)

**Guard rails — include ALL of these in the specialist spawn prompt:**

```
PEER COMMUNICATION (enabled for this task):
You may send ONE-SHOT messages to {other-specialist-name} for:
- Sharing API contracts, type definitions, or file paths they need
- Reporting that a dependency they're waiting on is ready

Rules:
1. ONE-SHOT ONLY: Send info, do NOT expect or wait for a reply.
   If you need a response, ask the team lead instead.
2. NO REPLY CHAINS: If you receive a peer message, do NOT reply
   to the sender. Use the information and continue your work.
   If you need clarification, ask the team lead.
3. MAX 2 PEER MESSAGES per task. If you need more coordination,
   route through the team lead.
4. ALWAYS notify the team lead after sending a peer message:
   "Sent {summary} to {teammate}." so the lead stays informed.
```

**Example: enabling peer messaging in spawn prompt:**

```
Task(
  subagent_type: "general-purpose",
  team_name: "{project}-{slug}",
  name: "backend-specialist",
  prompt: |
    You are a backend specialist working on: {requirement}

    EXPERTISE: Read .claude/agents/experts/backend/expertise.yaml

    YOUR FILE OWNERSHIP:
    - src/api/{specific files}

    PEER COMMUNICATION (enabled for this task):
    You may send ONE-SHOT messages to frontend-specialist for:
    - Sharing API contracts, type definitions, or file paths they need
    - Reporting that a dependency they're waiting on is ready

    Rules:
    1. ONE-SHOT ONLY: Send info, do NOT expect or wait for a reply.
    2. NO REPLY CHAINS: If you receive a peer message, do NOT reply to the sender.
    3. MAX 2 PEER MESSAGES per task.
    4. ALWAYS notify the team lead after sending a peer message.

    To discover teammates: read ~/.claude/teams/{project}-{slug}/config.json

    Check TaskList for your assigned tasks.
)
```

**If you do NOT include the peer communication block in the spawn prompt, specialists will only communicate through you.** This is the default and recommended behavior for most tasks.

---

## Step 6: Security Review Loop

After all specialist tasks are complete, a security agent reviews changes before the general review gate. This loop can bounce fixes back to specialists until the security agent is satisfied.

### 6a: Confirm All Specialist Tasks Complete

```
TaskList  # All specialist tasks must be completed
```

### 6b: Create Security Review Task and Spawn Security Agent

```
TaskCreate(
  title: "Security review of all team changes",
  description: |
    Review ALL changes made by the team for security implications:

    1. Supply chain: Verify any new dependencies against registries
    2. OWASP top 10: Injection, auth, XSS, path traversal, secrets
    3. Configuration: Permissions, debug flags, hardcoded values
    4. Data: PII handling, logging, encryption

    FILES MODIFIED:
    {list all files reported as modified by teammates}

    TEAMMATES AND THEIR FILE OWNERSHIP:
    {list each teammate and their owned files}

    You may ask the user for clarification on ambiguous security decisions
    using AskUserQuestion.

    Report findings with severity levels. CRITICAL and HIGH issues must
    be fixed before proceeding.
  owner: "security-reviewer"
)
```

Spawn the security agent:

```
Task(
  subagent_type: "security-agent",
  team_name: "{project}-{slug}",
  name: "security-reviewer",
  prompt: |
    You are the security reviewer for this team's changes.

    Check TaskList for your assigned security review task. Read the full task
    description for the list of modified files and file ownership.

    Your job:
    1. Read all modified files and their diffs
    2. Run the security checklist from your agent definition
    3. Verify any new dependencies against their registries via WebFetch
    4. Ask the user about ambiguous security decisions via AskUserQuestion
    5. Report findings to the team lead with severity levels

    You have READ access to all files. You CANNOT modify files — report
    what needs to change and which specialist should fix it.
)
```

### 6c: Process Security Review Results

When the security reviewer reports:

- **No CRITICAL/HIGH issues (PASS):** Proceed to Step 7 (General Review)
- **CRITICAL/HIGH issues found:** Enter the fix loop (6d)
- **MEDIUM/LOW only (PASS WITH NOTES):** Include in final report, proceed to Step 7

### 6d: Security Fix Loop

If CRITICAL or HIGH issues were found:

**Re-delegate to specialists:** Send each issue to the relevant specialist via the team lead:

```
SendMessage(
  type: "message",
  recipient: "{domain}-specialist",
  content: "Security review found issues in your files. Fix these:\n\n{issues with file:line and fix instructions}",
  summary: "Security fixes required"
)
```

**Wait for specialist fixes.** Specialists fix the issues and report back.

**Re-review:** Send the security agent back for a focused re-review:

```
SendMessage(
  type: "message",
  recipient: "security-reviewer",
  content: "Specialists have applied fixes. Re-review only the flagged files:\n\n{list of files that were fixed}",
  summary: "Re-review after security fixes"
)
```

**Repeat** if the re-review finds remaining issues (max 3 iterations to prevent infinite loops).

If issues persist after 3 iterations: Report to user with details, ask whether to proceed or abort.

### 6e: Shut Down Security Reviewer

```
SendMessage(type: "shutdown_request", recipient: "security-reviewer", content: "Security review complete")
```

Wait for shutdown confirmation.

---

## Step 7: Review and Validate

**CRITICAL:** After security review passes, spawn a review agent to cross-validate all changes. This catches integration issues, file conflicts, and regressions that individual specialists cannot see.

### 7a: Shut Down Specialists

Shut down all specialist teammates **before** spawning the reviewer (frees resources, prevents conflicts):

```
SendMessage(type: "shutdown_request", recipient: "backend-specialist", content: "All tasks complete")
SendMessage(type: "shutdown_request", recipient: "frontend-specialist", content: "All tasks complete")
```

Wait for all shutdown confirmations.

### 7b: Create Review Task and Spawn Reviewer

Create a review task that covers all modified files from all teammates:

```
TaskCreate(
  title: "Cross-validate all team changes",
  description: |
    Review ALL changes made by the team for:

    1. **Integration issues**: Do changes across domains work together?
       - Do API contracts match what the frontend expects?
       - Do CI workflows correctly build/test the modified code?
       - Do configuration changes propagate correctly?

    2. **File conflicts**: Did any teammate accidentally modify files outside their ownership?

    3. **Build validation**: Run builds and tests across ALL modified domains.

    4. **Regression check**: Do existing tests still pass after all changes?

    5. **Consistency**: Are naming conventions, error handling patterns, and code style
       consistent across the changes?

    TEAMMATES AND THEIR FILE OWNERSHIP:
    {list each teammate and their owned files from the task descriptions}

    FILES MODIFIED:
    {list all files reported as modified by teammates}

    Report findings as:
    - BLOCKING: Issues that must be fixed before merge (bugs, build failures, conflicts)
    - WARNING: Issues that should be addressed but don't block (style, minor improvements)
    - OK: Areas that passed validation

    Fix any BLOCKING issues directly. Report WARNING issues for the team lead.

    Additionally, produce these sections in your report:

    ## Expertise Improvement Suggestions
    Learnings from this review that should be captured in domain expertise.
    Only genuine, non-obvious insights — not filler.

    1. **{domain} — {pattern_name}**: {description}
       - Why: {what makes this worth documenting}
       - Suggested entry: `{exact text for expertise.yaml}`

    ## New Agent Suggestions
    Only if a genuine gap was found. Otherwise: "No new agents suggested."

    1. **{proposed-agent-name}**: {what it would do}
       - Gap: {what current agents can't handle}
       - Recommended domain: {which expertise.yaml}
  owner: "reviewer"
)
```

Spawn the reviewer:

```
Task(
  subagent_type: "review-agent",
  team_name: "{project}-{slug}",
  name: "reviewer",
  prompt: |
    You are a cross-domain reviewer validating changes made by a team of specialists.

    Check TaskList for your assigned review task. Read the full task description from the task list.

    ORIGINAL REQUIREMENT: {requirement}

    Your job:
    1. Read `git diff` or the modified files to understand all changes
    2. Verify builds compile and tests pass across ALL modified domains
    3. Check for integration issues between domains
    4. Check for file ownership violations
    5. **Validate the original requirement was met** — run tests, check coverage,
       verify measurable goals. If objectives are not met, report what's missing
       and which specialist should fix it
    6. If an objective can't be met for a technical reason, ask the user via
       AskUserQuestion whether to accept the gap or require more work
    7. Produce "Expertise Improvement Suggestions" — learnings worth capturing in expertise.yaml
    8. Produce "New Agent Suggestions" — only if a genuine coverage gap was found
    9. Report all findings to the team lead

    You have READ access to all files and can run build/test commands via Bash.
    You CANNOT modify files — report what needs to change and which specialist should fix it.
    You CAN ask the user for clarification on ambiguous requirements via AskUserQuestion.
)
```

### 7c: Process Review Results

When the reviewer reports back:

- **All OK / objectives met**: Proceed to Step 7d (shut down reviewer)
- **BLOCKING issues found**: Enter the review fix loop (7c-i)
- **Objective not met** with a clear fix: Enter the review fix loop (7c-i)
- **Objective not met** for technical reasons: Reviewer already asked the user — include the decision in the final report
- **WARNING issues only**: Include in the final report under "Recommended Follow-ups", proceed to 7d

#### 7c-i: Review Fix Loop

If the reviewer reports BLOCKING issues or unmet objectives that specialists can fix:

**Re-spawn the relevant specialist(s)** (they were shut down in 7a):

```
Task(
  subagent_type: "general-purpose",
  team_name: "{project}-{slug}",
  name: "{domain}-specialist",
  prompt: |
    You are being re-spawned to address review findings.

    EXPERTISE: Read .claude/agents/experts/{domain}/expertise.yaml for domain knowledge.

    FIX THESE ISSUES:
    {issues from reviewer with file:line and fix instructions}

    YOUR FILE OWNERSHIP (same as before):
    - {files}

    Fix the issues and report back when done.
)
```

**Wait for specialist fixes.** Specialists fix the issues and report back.

**Shut down re-spawned specialists** after they complete their fixes.

**Re-review:** Send the reviewer back for focused re-review:

```
SendMessage(
  type: "message",
  recipient: "reviewer",
  content: "Specialists have applied fixes. Re-review only the flagged files and re-check the objectives:\n\n{list of files fixed and objectives to verify}",
  summary: "Re-review after fixes"
)
```

**Repeat** if the re-review finds remaining issues (max 3 iterations to prevent infinite loops).

If issues persist after 3 iterations: escalate to user via `AskUserQuestion` with full details, ask whether to accept current state or abort.

### 7d: Shut Down Reviewer

```
SendMessage(type: "shutdown_request", recipient: "reviewer", content: "Review complete")
```

Wait for shutdown confirmation.

### 7e: Acknowledge Expertise Suggestions

If the reviewer produced "Expertise Improvement Suggestions" or "New Agent Suggestions":

```
AskUserQuestion: "The reviewer suggests expertise improvements. Apply them?"
Options: ["Yes, update expertise (Recommended)", "Skip expertise update"]
```

If user skips or no suggestions: Proceed to Step 8.

### 7f: Improve (Only if user accepted)

Spawn improve-agents for each affected domain, passing the review feedback:

```
Task(
  subagent_type: "<domain>-improve-agent",
  prompt: |
    Review recent changes and update expertise.

    REVIEW_FEEDBACK:
    {paste the domain-relevant Expertise Improvement Suggestions from the reviewer}
)
```

Spawn all domain improve-agents **in parallel** (one per affected domain). Wait for all to complete.

---

## Step 8: Clean Up and Report

### 8a: Clean Up

```
TeamDelete  # Removes team and task directories
```

### 8b: Report Results

```markdown
## `/do-teams` - Complete

**Requirement:** {requirement}
**Team:** {project}-{slug}
**Teammates:** {count} specialists across {domains}
**Status:** Success

### Work Summary

| Teammate | Domain | Tasks | Files Modified |
|----------|--------|-------|----------------|
| backend-specialist | backend | 3/3 | src/api/auth/register.ts, src/api/auth/register.test.ts |
| frontend-specialist | frontend | 2/2 | src/ui/login/LoginForm.tsx, src/ui/login/LoginForm.test.tsx |

### Files Modified
- {full list from all teammates}

### Security Review
- {PASS / PASS WITH NOTES / required N fix iterations}
- {supply chain verification results}
- {user decisions on ambiguous security items, if any}

### Review Results
- {BLOCKING issues found and fixed, if any}
- {build/test validation results per domain}

### Tips & Expertise Updates
- {what was added to tips.md per domain — show exact additions so user can verify}
- {what was captured in expertise.yaml, or "Skipped by user"}

### Recommended Follow-ups
- {WARNING issues from reviewer}
- {New agent suggestions from reviewer, if any}
- {context-specific suggestions}
```

---

## Error Handling

### Teammate Fails

- Check error message from teammate
- If recoverable: create corrective task, assign to same or different teammate
- If unrecoverable: shut down team, report partial results, preserve completed work

### Teammate Goes Idle Unexpectedly

Idle is NORMAL between turns. Only investigate if:
- Teammate has been idle for extended period with incomplete tasks
- Send a message to check status before assuming failure

### File Conflict

If two teammates accidentally modify the same file:
1. Pause both teammates
2. Determine who should own the file
3. Have the non-owner revert their changes
4. Resume work

### Dependency Deadlock

If tasks are blocked in a cycle:
1. Identify the cycle via TaskList
2. Break the cycle by removing a dependency
3. Create a coordination task for the previously-blocked work

---

## Examples

### Example 1: Cross-Domain Feature

```bash
/do-teams "Add auth with API and frontend"
```

**Classification:** Implementation pattern, domains: backend + frontend

**Team:** myapp-user-auth
**Teammates:**
- `backend-specialist`: Implement auth API endpoint
- `frontend-specialist`: Build login form and connect to API

**Tasks:**
1. [backend] Create auth schema and types → backend-specialist
2. [backend] Implement registration endpoint → backend-specialist
3. [backend] Write API tests → backend-specialist
4. [frontend] Build login form component → frontend-specialist (blocked_by: [1])
5. [frontend] Connect form to auth API → frontend-specialist (blocked_by: [2])
6. [frontend] Write component tests → frontend-specialist

### Example 2: Architecture Review (Council)

```bash
/do-teams "Review security of auth flow"
```

**Classification:** Council pattern, domains: backend + frontend + devops

**Team:** auth-security-review
**Teammates:**
- `backend-analyst`: Review auth logic, token handling, input validation
- `frontend-analyst`: Review credential handling, XSS prevention, storage
- `devops-analyst`: Review CI secrets handling, deployment security

Each analyst sends findings. Lead synthesizes into unified security report.

### Example 3: Parallel Testing

```bash
/do-teams "Fix all failing tests"
```

**Team:** test-fix
**Teammates:**
- `backend-specialist`: Run backend tests, fix failures
- `frontend-specialist`: Run frontend tests, fix failures
- `devops-specialist`: Run CI/integration tests, fix failures

All work in parallel on independent test suites.

---

## Cost Awareness

Agent teams use significantly more tokens than solo sessions or subagents.

| Configuration | Cost Multiplier | Use When |
|---------------|----------------|----------|
| `/do` (subagents) | ~2x | Single-domain tasks |
| `/do-teams` 2 teammates | ~3x | Two-domain feature |
| `/do-teams` 3 teammates | ~4-5x | Cross-cutting change |
| `/do-teams` 5 teammates | ~6-8x | Major refactoring |

**Optimize by:**
- Planning first with `/do` Pattern A (plan-agent), then executing with `/do-teams`
- Only spawning teammates for domains that actually need changes
- Keeping task descriptions focused (less context = fewer tokens)
- Shutting down teammates as soon as their work is complete
