---
description: Fast workflow without security/review fix loops
argument-hint: <requirement>
allowed-tools: Read, Glob, Grep, Task, AskUserQuestion, TodoWrite
---

# `/do-quick` - Fast Single-Domain Workflow

Same as `/do` Pattern A but without security review or fix loops. Use when you trust the domain and want speed over thoroughness.

```
plan-agent → spec
     ↓
User: "Proceed?" → [Yes] / [No]
     ↓
build-agent → implementation
     ↓
review-agent → quality report + suggestions
     ↓
User: "Apply suggestions?" → [Yes] / [Skip]
     ↓
improve-agent → updates expertise
```

## CRITICAL: Orchestration-First Approach

**You are a dispatcher, not a worker.** Delegate everything to expert agents.

**You MUST NOT:**
- Read files directly (delegate to agents)
- Write files directly (delegate to agents)
- Make code changes (delegate to agents)
- Make implementation decisions (delegate to plan-agent)

> **If you're about to use Read, Write, Edit, or Grep—STOP. Spawn an agent instead.**

## Step 1: Parse and Classify

Extract requirement from `$ARGUMENTS`.

### Domain Detection

1. **Read `domains.md`** (at `.claude/domains.md`): Primary source. Match the user's requirement against each domain's keywords and paths.

2. **Read `domain-map.conf`** (at `.claude/domain-map.conf`): Match specific file paths mentioned in the requirement.

3. **Scan `experts/` directory** (at `.claude/agents/experts/`): Fallback if `domains.md` is missing or incomplete.

4. **Fallback**: If no domain matches, use the generic `build-agent` or ask the user to clarify.

### Pattern Classification

**If the requirement is a question** ("How do I...", "What is...", "Explain..."):
```
Task(subagent_type: "<domain>-question-agent", prompt: "USER_PROMPT: {requirement}")
```
Report the answer and exit.

**If the requirement is a simple task** (format, lint, validate, check):
```
Task(subagent_type: "build-agent", prompt: "{requirement}")
```
Report the result and exit.

**Otherwise:** Continue with the implementation flow below.

## Step 2: Plan

```
Task(subagent_type: "<domain>-plan-agent", prompt: "USER_PROMPT: {requirement}")
```
Capture `spec_path` from output.

## Step 3: User Approval

```
AskUserQuestion: "Plan complete at {spec_path}. Proceed with implementation?"
Options: ["Yes, continue to build (Recommended)", "No, stop here - I'll review first"]
```

If user declines: Report spec location, exit gracefully.

## Step 4: Build

```
Task(subagent_type: "<domain>-build-agent", prompt: "SPEC: {spec_path}")
```
Capture files modified. If build fails → report error, exit.

## Step 5: Review

```
Task(subagent_type: "review-agent", prompt: |
  Review the changes just made for: {requirement}
  Domain: {domain}
  Files modified: {files_modified from build output}
  Spec: {spec_path}

  Produce your full review including:
  1. Quality assessment (issues by severity)
  2. Tips Suggestions (operational facts for tips.md)
  3. Expertise Improvement Suggestions (learnings for expertise.yaml)
  4. New Agent Suggestions (only if genuine gap found)
)
```
Present the quality report to the user. No fix loop — issues are reported as recommendations.

## Step 6: Acknowledge Suggestions

If the review contains tips, expertise, or agent suggestions:
```
AskUserQuestion: "The review agent suggests improvements. Apply them?"
Options: ["Yes, update tips + expertise (Recommended)", "Skip updates"]
```

If user skips or no suggestions: End workflow.

## Step 7: Improve (Only if user accepted)

```
Task(subagent_type: "<domain>-improve-agent", prompt: |
  Review recent changes and update tips and expertise.

  REVIEW_FEEDBACK:
  {paste the Tips Suggestions, Expertise Improvement Suggestions, and New Agent Suggestions sections from the review output}
)
```
Non-blocking on failure. **Report tips.md changes to the user** so they can verify.

## Report

```markdown
## `/do-quick` - Complete

**Requirement:** {requirement}
**Domain:** {detected domain}
**Status:** Success

### Workflow Stages

| Stage | Status | Key Output |
|-------|--------|------------|
| Plan | Complete | {spec_path} |
| Build | Complete | {file_count} files modified |
| Review | Complete | {issue_count} issues, {suggestion_count} expertise suggestions |
| Improve | Complete/Skipped | {Expert knowledge updated / User skipped} |

### Files Modified
{list from build-agent}

### Review Summary
{quality assessment from review-agent}

### Tips & Expertise Updates
{what was added to tips.md — show exact additions so user can verify}
{what was captured in expertise.yaml, or "Skipped by user"}

### Next Steps
{context-specific suggestions}
```

## Error Handling

- **Classification unclear**: Use AskUserQuestion with domain options
- **Plan fails**: Report error, exit
- **User declines plan**: Save spec location, exit gracefully
- **Build fails**: Preserve spec, report error, skip review and improve
- **Review fails**: Log error, skip improve, workflow still succeeds
- **User declines suggestions**: End workflow after review
- **Improve fails**: Log error, workflow still succeeds

## When to Use `/do-quick` vs `/do`

| | `/do-quick` | `/do` |
|---|---|---|
| Security review | No | Yes (with fix loop) |
| Objective validation | No | Yes (with fix loop) |
| Review fix loop | No | Yes (max 3 iterations) |
| Speed | Fast | Thorough |
| Use when | Trusted domain, quick iteration, prototyping | Production code, new dependencies, security-sensitive changes |
