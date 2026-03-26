# 🥴 CUITE

### **The Claude sUITE Framework**

*A high-velocity drunken bender for your workflow.*

<pre>
      .----------------.
     |   _          _   |
     |  | |        | |  |
     |  | |   __   | |  |    "Code responsibly.
     |  | |__/  \__| |  |     Or don't."
     |  |____________|  |
      '----------------'
</pre>

<div align="center">

![Stability](https://img.shields.io/badge/Stability-Hazardous-red)
![Vibe](https://img.shields.io/badge/Vibe-Immaculate-brightgreen)
![Logic](https://img.shields.io/badge/Reasoning-Anthropic-blue)
![Claude Code](https://img.shields.io/badge/Powered%20by-Claude%20Code-orange)

---

**Cuite** (pronounced *kweet* / /kɥit/) is the unhinged way to orchestrate teams of Claude Code agents.
It turns complex development tasks into a seamless, multi-agent session where the only thing
getting hammered is your technical debt.

[**Documentation**](https://cuite.quest) · [**API Reference**](https://cuite.quest/api.html) · [**CLI Reference**](https://cuite.quest/cli.html)

</div>

## 🍸 The Workflow

1. **The Pour:** Define your objectives and feed the context to the primary agent.
2. **The Mix:** Cuite spins up a team of specialized Claude instances (Frontend, Backend, QA).
3. **Bottoms Up:** The agents iterate in a high-speed loop, peer-reviewing and committing code.
4. **The Hangover:** You wake up to a fully realized feature and a clean `git log`.

---

## 🤮 The Aftermath (The Serious Stuff)

Don't let the name fool you. **Cuite** is a rigorous, reusable framework designed to give Claude Code structured expertise and safety rails.

### 🛠 Key Features

* **Domain-Specialized Agents:** Automatically context-switches between specialized personas (Plan, Build, Improve, and Question) based on the file path.
* **Security Hooks (The Bouncer):**
  * Supply chain verification for new dependencies.
  * Network egress whitelisting.
  * Scope enforcement & Prompt Injection detection.
* **Self-Improving Expertise:** Agents don't just work; they learn. Post-run summaries update your domain knowledge base automatically.
* **Orchestration Commands:**
  * `/do` — Full task execution with security + review loops.
  * `/do-quick` — Fast lane without security/review fix loops.
  * `/do-teams` — Parallelized agent coordination.
  * `/improve` — Dedicated maintenance and refactor mode.
  * `/cuite-init` — Auto-detect domains and bootstrap everything.
  * `/cuite-sync` — Check framework-project consistency.

## Quick Start

Cuite lives at `.claude/cuite/` via git subtree. Symlinks bridge it to `.claude/` where Claude Code expects files. Your project-specific settings and domain experts stay outside the subtree — clean push and pull. Domain expertise grows in your project repo automatically as agents work.

**Fork first** if you want to customize the framework itself (hooks, commands, templates) or contribute improvements back.

```bash
# Run from your project root
curl -fsSL https://cuite.quest/install.sh | bash
```

Or manually:

```bash
cd /path/to/your-project
git clone https://github.com/fentas/cuite.git /tmp/cuite  # or your fork
/tmp/cuite/bin/cuite init
rm -rf /tmp/cuite
```

Then bootstrap your domains:

```text
/cuite-init
```

This scans your repo, detects languages and sub-projects, proposes domains, and on approval generates `domains.md`, `domain-map.conf`, and starter expertise files.

**Or customize manually:**

```bash
$EDITOR CLAUDE.md                    # project structure table
$EDITOR .claude/domains.md           # domain registry (keywords, paths, commands)
$EDITOR .claude/domain-map.conf      # path-to-domain mappings
$EDITOR .claude/settings.json        # WebFetch whitelist domains
```

**Update framework:** `.claude/cuite/bin/cuite pull`
**Push improvements:** `.claude/cuite/bin/cuite push`
**Check consistency:** `/cuite-sync`

See [docs/git-subtree.md](docs/git-subtree.md) for the full hybrid layout, CLI reference, and FAQ.

## Structure

This repo root = what becomes `.claude/cuite/` in consuming projects via `git subtree --prefix .claude/cuite`.

```text
cuite repo root (→ .claude/cuite/ in your project)
  bin/
    cuite                          # Management CLI
  CLAUDE.md                        # Template — copy to project root
  README.md                        # This file
  docs/
    git-subtree.md                 # Hybrid layout guide, CLI ref, FAQ
  settings.json                    # Template: permissions + hooks wiring
  settings.local.json              # Template: local env flags
  domains.md                       # Template: domain registry
  domain-map.conf                  # Template: path-to-domain mappings
  template.md                      # Full setup & usage guide
  teammates.md                     # Runtime workflow reference
  commands/
    do.md                          # /do — single-domain orchestrator (full)
    do-quick.md                    # /do-quick — fast lane (no security/review loops)
    do-teams.md                    # /do-teams — parallel team orchestrator
    improve.md                     # /improve — expertise maintenance
    cuite-init.md                  # /cuite-init — domain bootstrapper
    cuite-sync.md                  # /cuite-sync — framework sync checker
  hooks/
    scope-enforcement.sh           # Project boundaries, network, supply chain
    validate-intent.sh             # Syntax checks on edits
    detect-injection.sh            # Prompt injection detection
    track-learnings.sh             # Domain breadcrumbs
    inject-expertise.sh            # Auto-inject tips + expertise into agents
    session-context.sh             # Session start context
  agents/
    build-agent.md                 # Core: general implementation
    scout-agent.md                 # Core: read-only exploration
    review-agent.md                # Core: quality gate + improvement loop
    agent-registry.json            # Agent index
    templates/                     # Copy to create new domain agents
    experts/
      agent-teams-blueprint.md     # Meta-guide for auto-generation
      {domain}/
        tips.md                    # Quick operational facts
        expertise.yaml             # Deep domain knowledge
        {domain}-{plan,build,improve,question}-agent.md
```

### In your project (after `cuite init`)

```text
project/
  CLAUDE.md                          real (customized)
  .claude/
    cuite/                           git subtree (framework)
    hooks -> cuite/hooks             symlink
    commands -> cuite/commands        symlink
    agents/
      *.md -> cuite/agents/*.md      symlinks (core agents)
      templates -> cuite             symlink
      experts/
        my-domain/                   real (your domains)
    settings.json                    real (your whitelist + hooks)
    domains.md                       real (your domain registry)
    domain-map.conf                  real (your path mappings)
```

## Key Design Decisions

* **Hybrid subtree + symlinks** — framework is pushable, project files stay local
* **Hooks use `$PWD`** — robust path resolution regardless of symlink traversal
* **Hooks auto-discover domains** from `experts/` directory — no hardcoded lists
* **`domains.md` is the primary domain registry** — keywords, paths, commands in one file outside the subtree
* **Path-to-domain mappings** live in `domain-map.conf` — glob patterns for hooks
* **Security is non-negotiable** — supply chain, version verification, network egress, HTTP red flags
* **tips.md injected before expertise.yaml** — compact facts prevent repeated discovery loops
* **Review gate between build and improve** — user acknowledges before expertise updates
* **Self-reflection** — every agent reports blockers and validates tips accuracy
* **Settings sync** — `cuite settings` merges framework hooks while keeping project permissions

## Docs

* [docs/git-subtree.md](docs/git-subtree.md) — Hybrid layout, CLI reference, settings sync, FAQ
* [template.md](template.md) — Full setup and usage guide
* [teammates.md](teammates.md) — Runtime workflow with diagrams
* [agents/experts/agent-teams-blueprint.md](agents/experts/agent-teams-blueprint.md) — The meta-specification
