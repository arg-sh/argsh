---
description: Universal entry point - delegates to appropriate workflow
argument-hint: <requirement>
allowed-tools: Read, Glob, Grep, Task, AskUserQuestion, TodoWrite
---

# `/do` - Universal Workflow Entry Point

Single command interface for all workflows. Analyzes requirements and orchestrates expert agents through plan-build-security-review-improve cycles.

**Pattern A (Implementation):**
```
plan-agent → spec
     ↓
User: "Proceed?" → [Yes] / [No]
     ↓
build-agent → implementation
     ↓
security-agent → security review (can ask user)
     ↓                  ↑
     ↓         fix loop (max 3x)
     ↓
review-agent → quality check + objective validation
     ↓                  ↑
     ↓         fix loop (max 3x)
     ↓
User: "Apply suggestions?" → [Yes] / [Skip]
     ↓
improve-agent → updates expertise
```

**Pattern B (Question):** `question-agent → answer`

**Pattern C (Simple):** `build-agent → result`

## CRITICAL: Orchestration-First Approach

**You are a dispatcher, not a worker.** Delegate everything to expert agents.

**Your ONLY responsibilities:**
1. Parse and classify requirements
2. Select the appropriate pattern (A, B, or C)
3. Spawn expert agents via Task tool
4. Wait for results
5. Synthesize and report outcomes

**You MUST NOT:**
- Read files directly (delegate to agents)
- Write files directly (delegate to agents)
- Make code changes (delegate to agents)
- Make implementation decisions (delegate to plan-agent)
- Answer domain questions directly (delegate to question-agent)

> **If you're about to use Read, Write, Edit, or Grep—STOP. Spawn an agent instead.**

## Step 1: Parse Arguments

Extract requirement from `$ARGUMENTS`. Capture the core requirement description.

## Step 2: Classify Requirement

### Expert Domain Detection (Dynamic Discovery)

Domains are discovered dynamically — not hardcoded. To identify the correct domain:

1. **Read `domains.md`** (at `.claude/domains.md`): Primary source. Lists each domain with description, keywords, paths, and build commands. Match the user's requirement against each domain's keywords and paths.

2. **Read `domain-map.conf`** (at `.claude/domain-map.conf`): Maps file path glob patterns to domain names. Use this to match specific file paths mentioned in the requirement.

3. **Scan `experts/` directory** (at `.claude/agents/experts/`): Each subdirectory is a domain. If `domains.md` is missing or incomplete, check each domain's `expertise.yaml` for additional context.

4. **Fallback**: If no domain matches, use the generic `build-agent` (Pattern C) or ask the user to clarify.

### Pattern Classification

**Pattern A - Implementation (Plan-Build-Review-Improve):**
- Verbs: fix, add, create, implement, update, configure, refactor
- Flow: plan-agent → user approval → build-agent → security-agent (fix loop) → review-agent (fix loop) → user acknowledges suggestions → improve-agent

**Pattern B - Question (Direct Answer):**
- Phrasing: "How do I...", "What is...", "Why...", "Explain..."
- Flow: question-agent → report answer

**Pattern C - Simple Workflow (Single Agent):**
- Verbs: format, lint, validate, check
- Flow: build-agent → report results

## Step 3: Execute Pattern

### Pattern A: Expert Implementation

**Phase 1 - Plan:**
```
Task(subagent_type: "<domain>-plan-agent", prompt: "USER_PROMPT: {requirement}")
```
Capture `spec_path` from output.

**Phase 2 - User Approval:**
```
AskUserQuestion: "Plan complete at {spec_path}. Proceed with implementation?"
Options: ["Yes, continue to build (Recommended)", "No, stop here - I'll review first"]
```

If user declines: Report spec location, exit gracefully.

**Phase 3 - Build:**
```
Task(subagent_type: "<domain>-build-agent", prompt: "SPEC: {spec_path}")
```
Capture files modified. If build fails → skip review and improve, report error.

**Phase 4 - Security Review:**
```
Task(subagent_type: "security-agent", prompt: |
  Review the changes just made for: {requirement}
  Domain: {domain}
  Files modified: {files_modified from build output}

  Your job:
  1. Read all modified files
  2. Run the security checklist from your agent definition
  3. Verify any new dependencies against their registries via WebFetch
  4. Ask the user about ambiguous security decisions via AskUserQuestion
  5. Report findings with severity levels

  You have READ access to all files. You CANNOT modify files — report
  what needs to change.
)
```

Process security review results:

- **PASS (no CRITICAL/HIGH):** Proceed to Phase 5
- **PASS WITH NOTES (MEDIUM/LOW only):** Include in final report, proceed to Phase 5
- **CRITICAL/HIGH issues found:** Enter the security fix loop

**Security Fix Loop:** If CRITICAL or HIGH issues were found:

1. Spawn build-agent to fix the flagged issues:
   ```
   Task(subagent_type: "<domain>-build-agent", prompt: |
     Fix these security issues found by the security reviewer:

     {issues with file:line and fix instructions}

     Files you may modify: {files_modified}
     SPEC: {spec_path}
   )
   ```
2. Re-run security review on the fixed files:
   ```
   Task(subagent_type: "security-agent", prompt: |
     Re-review only the previously flagged files after fixes:

     Files to re-review: {fixed files}
     Previous issues: {list of issues that were flagged}

     Verify each CRITICAL/HIGH issue is resolved.
     Check that fixes didn't introduce new issues.
   )
   ```
3. Repeat if re-review finds remaining issues (max 3 iterations)
4. If issues persist after 3 iterations: report to user via AskUserQuestion, ask whether to proceed or abort

**Phase 5 - Review and Validate:**
```
Task(subagent_type: "review-agent", prompt: |
  Review the changes just made for: {requirement}
  Domain: {domain}
  Files modified: {files_modified from build output}
  Spec: {spec_path}

  ORIGINAL REQUIREMENT: {requirement}

  Your job:
  1. Quality assessment (issues by severity)
  2. **Validate the original requirement was met** — run tests, check coverage,
     verify measurable goals. If objectives are not met, report what's missing
     and what fix is needed
  3. If an objective can't be met for a technical reason, ask the user via
     AskUserQuestion whether to accept the gap or require more work
  4. Tips Suggestions (operational facts for tips.md)
  5. Expertise Improvement Suggestions (learnings for expertise.yaml)
  6. New Agent Suggestions (only if genuine gap found)

  You have READ access to all files and can run build/test commands via Bash.
  You CANNOT modify files — report what needs to change.
  You CAN ask the user for clarification on ambiguous requirements via AskUserQuestion.
)
```

Process review results:

- **All OK / objectives met:** Proceed to Phase 6
- **BLOCKING issues or unmet objectives** with a clear fix: Enter the review fix loop
- **Objective not met** for technical reasons: Reviewer already asked the user — include the decision in the final report, proceed to Phase 6
- **WARNING issues only:** Include in the final report, proceed to Phase 6

**Review Fix Loop:** If BLOCKING issues or unmet objectives were found:

1. Spawn build-agent to fix the flagged issues:
   ```
   Task(subagent_type: "<domain>-build-agent", prompt: |
     Fix these issues found by the reviewer:

     {issues with file:line and fix instructions}

     Files you may modify: {files_modified}
     SPEC: {spec_path}
   )
   ```
2. Re-run review on the fixed files:
   ```
   Task(subagent_type: "review-agent", prompt: |
     Re-review after fixes. Check only the previously flagged files and
     re-check the objectives:

     ORIGINAL REQUIREMENT: {requirement}
     Files to re-review: {fixed files}
     Previous issues: {list of issues that were flagged}
     Objectives to verify: {measurable goals from the requirement}
   )
   ```
3. Repeat if re-review finds remaining issues (max 3 iterations)
4. If issues persist after 3 iterations: escalate to user via AskUserQuestion with full details

Capture the final review output. Present the full quality report to the user.

**Phase 6 - Acknowledge Suggestions:**

If the review contains tips, expertise, or agent suggestions:
```
AskUserQuestion: "The review agent suggests improvements. Apply them?"
Options: ["Yes, update tips + expertise (Recommended)", "Skip updates"]
```

If user skips or no suggestions: End workflow after review.

**Phase 7 - Improve (Only if user accepted):**
```
Task(subagent_type: "<domain>-improve-agent", prompt: |
  Review recent changes and update tips and expertise.

  REVIEW_FEEDBACK:
  {paste the Tips Suggestions, Expertise Improvement Suggestions, and New Agent Suggestions sections from the review output}
)
```
Non-blocking on failure. **Report tips.md changes to the user** so they can verify and adjust if the path isn't optimal.

### Pattern B: Expert Question

```
Task(subagent_type: "<domain>-question-agent", prompt: "USER_PROMPT: {requirement}")
```

### Pattern C: Simple Workflow

```
Task(subagent_type: "build-agent", prompt: "{requirement}")
```

## Step 4: Wait and Collect Results

**CRITICAL: Wait for ALL Task calls to complete before responding.**

Validation checkpoint:
- [ ] All spawned agents returned results
- [ ] Results are non-empty
- [ ] No pending Task calls

## Step 5: Report Results

### Pattern A Report

```markdown
## `/do` - Complete

**Requirement:** {requirement}
**Domain:** {detected domain}
**Status:** Success

### Workflow Stages

| Stage | Status | Key Output |
|-------|--------|------------|
| Plan | Complete | {spec_path} |
| Build | Complete | {file_count} files modified |
| Security | Complete | {PASS / PASS WITH NOTES / required N fix iterations} |
| Review | Complete | {issue_count} issues, {suggestion_count} expertise suggestions |
| Improve | Complete/Skipped | {Expert knowledge updated / User skipped} |

### Files Modified
{list from build-agent}

### Security Review
{PASS / PASS WITH NOTES / required N fix iterations}
{supply chain verification results}
{user decisions on ambiguous security items, if any}

### Review Summary
{quality assessment from review-agent}

### Tips & Expertise Updates
{what was added to tips.md — show exact additions so user can verify}
{what was captured in expertise.yaml, or "Skipped by user"}

### Next Steps
{context-specific suggestions}
```

### Pattern B Report

```markdown
## `/do` - Complete

**Requirement:** {requirement}
**Domain:** {domain}
**Type:** Question

### Answer
{answer from question-agent}
```

### Pattern C Report

```markdown
## `/do` - Complete

**Requirement:** {requirement}
**Status:** Success

### Results
{results from agent}
```

## Error Handling

- **Classification unclear**: Use AskUserQuestion with domain options
- **Plan fails**: Report error, exit (no spec to build from)
- **User declines plan**: Save spec location, exit gracefully (not an error)
- **Build fails**: Preserve spec, report error, skip security review, review, and improve
- **Security review fails**: Log error, proceed to general review (security is important but shouldn't block the entire workflow on agent failure)
- **Security fix loop exhausted (3 iterations)**: Ask user whether to proceed or abort
- **Review fails**: Log error, skip improve, workflow still succeeds (build output is valid)
- **Review fix loop exhausted (3 iterations)**: Ask user whether to accept current state or abort
- **User declines suggestions**: End workflow after review (not an error)
- **Improve fails**: Log error, workflow still succeeds

## Examples

```bash
/do "Add user auth endpoint"
# → backend domain (detected from domain-map.conf), Pattern A: plan → approve → build → security → review → acknowledge → improve

/do "How does caching work?"
# → detected domain, Pattern B: question-agent answers

/do "Lint all files"
# → no specific domain, Pattern C: build-agent runs linting
```
