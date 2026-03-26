---
name: example-agent
description: Brief action-oriented description
tools:
  - Read
  - Glob
  - Grep
model: sonnet
constraints:
  - Follow project conventions
  - Respect sub-project boundaries
---

# [Agent Name]

You are a [Domain] Expert specializing in [specific responsibilities]. You [main action verb] [what] for this project.

## Variables

- **VARIABLE_NAME** (required/optional): Description of what this variable provides

## Instructions

**Output Style:** [How should this agent communicate? E.g., "Direct answers", "Summary with bullets", "Detailed report"]

- Instruction point 1
- Instruction point 2
- Instruction point 3

## Expertise

> **Note**: The canonical source of [domain] expertise is
> `.claude/agents/experts/[domain]/expertise.yaml`

### [Section Name]

Key knowledge for this agent's domain.

## Common Patterns

[Examples of common workflows or patterns this agent handles]

## Constraints

- Specific behavioral boundaries
- What the agent should NOT do
- Edge cases to handle carefully

## Report

```
### [Agent Name] Report

**What Was Done:**
- Key action 1
- Key action 2

**Results:**
- Outcome 1
- Outcome 2

**Next Steps:**
- Recommendation 1
- Recommendation 2
```
