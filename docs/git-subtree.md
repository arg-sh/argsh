# Hybrid Subtree + Symlink Integration

Cuite uses a **hybrid layout**: git subtree at `.claude/cuite/` for the framework, with symlinks bridging to `.claude/` where Claude Code expects files.

- **Clean push/pull** — only framework files travel; your domains stay local
- **Full git history** — framework changes visible in `git log`
- **No submodules** — no `.gitmodules`, no recursive cloning

---

## Quick Start

```bash
# Bootstrap (one-time)
git clone https://github.com/fentas/cuite.git /tmp/cuite
/tmp/cuite/bin/cuite init
rm -rf /tmp/cuite

# Or step by step
git remote add cuite https://github.com/fentas/cuite.git
git subtree add --prefix .claude/cuite cuite main --squash
.claude/cuite/bin/cuite link
```

Then customize:

```bash
$EDITOR CLAUDE.md                    # project structure table
$EDITOR .claude/domain-map.conf      # path-to-domain mappings
$EDITOR .claude/settings.json        # WebFetch whitelist domains
```

---

## Layout

```
project/
  CLAUDE.md                            ← real (customized from template)
  .claude/
    cuite/                             ← git subtree (pushable framework)
      bin/cuite                        ← management CLI
      hooks/                           ← hook scripts (actual code)
      commands/                        ← command scripts
      agents/                          ← framework agents + templates
      settings.json                    ← template (reference for sync)
      ...
    hooks -> cuite/hooks               ← symlink
    commands -> cuite/commands          ← symlink
    template.md -> cuite/template.md   ← symlink
    teammates.md -> cuite/teammates.md ← symlink
    agents/
      build-agent.md -> cuite          ← symlinks (framework agents)
      review-agent.md -> cuite
      scout-agent.md -> cuite
      templates -> cuite               ← symlink
      agent-registry.json              ← real (project-specific)
      experts/
        agent-teams-blueprint.md -> cuite  ← symlink
        my-domain/                     ← real (project-specific)
          expertise.yaml
          tips.md
          my-domain-build-agent.md
    settings.json                      ← real (project-specific)
    settings.local.json                ← real (per-user, gitignored)
    domain-map.conf                    ← real (project-specific)
    .cache/                            ← gitignored
```

### Why this works

| Concern | Solution |
|---------|----------|
| Push only sends framework | Subtree prefix is `.claude/cuite/` — project files are outside |
| Claude Code finds hooks | `.claude/hooks` symlinks to `.claude/cuite/hooks` |
| Project settings stay yours | Real file at `.claude/settings.json` |
| Domain experts are yours | Real dirs under `.claude/agents/experts/` |
| Other devs get the layout | Git tracks symlinks as link targets — `clone` restores them |
| Hooks resolve paths correctly | All hooks use `$PWD` (project root, guaranteed by Claude Code) |

---

## CLI Reference

```bash
.claude/cuite/bin/cuite <command>

# or alias it
alias cuite='.claude/cuite/bin/cuite'
```

| Command | Description |
|---------|-------------|
| `init` | Full setup: remote + subtree + symlinks + starter files |
| `link` | Create/refresh all symlinks |
| `pull` | Pull framework updates + refresh symlinks + check settings |
| `push` | Push framework changes back to cuite repo |
| `settings [-q]` | Compare/sync hooks between project and framework |
| `status` | Show remote, symlinks, domains, settings status |
| `log [N]` | Show last N (default 20) cuite-related commits |
| `diff` | Show uncommitted changes in cuite subtree |

### Environment

| Variable | Default | Description |
|----------|---------|-------------|
| `CUITE_REMOTE` | `cuite` | Git remote name |
| `CUITE_BRANCH` | `main` | Branch to track |
| `CUITE_REPO` | `https://github.com/fentas/cuite.git` | Repository URL |

---

## Pulling Updates

```bash
cuite pull
```

Runs `git subtree pull --squash`, refreshes symlinks, and checks settings for hook drift.

### What happens during pull

| File type | Behavior |
|-----------|----------|
| **Framework** (hooks, commands, core agents) | Updated in `.claude/cuite/` |
| **Symlinks** | Refreshed automatically |
| **Your files** (settings.json, domain-map.conf) | Untouched (outside subtree) |
| **Your domains** (expertise.yaml, tips.md) | Untouched (outside subtree) |
| **Framework template** (settings.json in cuite/) | Updated for reference |

### Conflicts

Conflicts only happen inside `.claude/cuite/` (if you edited framework files directly):

```bash
git status
git checkout --theirs .claude/cuite/    # accept upstream (usual choice)
git add .claude/cuite/
git commit -m "chore: merge cuite framework updates"
```

---

## Pushing Framework Changes

```bash
cuite push
```

Only files under `.claude/cuite/` are pushed. Project-specific files are **never** included.

### Contributing improvements

If you improved a framework file while working in your project:

```bash
# Edit directly in the subtree, commit, push
$EDITOR .claude/cuite/hooks/scope-enforcement.sh
git add .claude/cuite/hooks/scope-enforcement.sh
git commit -m "fix: scope-enforcement edge case"
cuite push
```

All other projects get the fix via `cuite pull`.

---

## Settings Sync

`.claude/settings.json` has two sections:

- **`permissions.allow`** — project-specific (your WebFetch whitelist)
- **`hooks`** — framework wiring (should track upstream)

When the framework updates hooks:

```bash
cuite settings
```

Shows a diff and offers to update hooks while preserving your permissions.

Use `cuite settings -q` for a non-interactive check (used automatically by `cuite pull`).

---

## File Categories

### Framework (inside `.claude/cuite/`, updated by pull)

```
hooks/*                            # Security enforcement, expertise injection
commands/*                         # /do, /do-teams, /improve
agents/build-agent.md              # Core build agent
agents/scout-agent.md              # Core scout agent
agents/review-agent.md             # Core review agent
agents/templates/*                 # Agent templates
agents/experts/agent-teams-blueprint.md
bin/cuite                          # Management CLI
bin/argsh.min.sh                   # Bash framework runtime
settings.json                      # Template (reference for sync)
CLAUDE.md                          # Template (copied to project root)
template.md                        # Setup guide
teammates.md                       # Workflow reference
```

### Project-specific (real files in `.claude/`, never pushed)

```
settings.json                      # Your WebFetch whitelist + hooks wiring
settings.local.json                # Per-user (gitignored)
domain-map.conf                    # Your path-to-domain mappings
agents/agent-registry.json         # Your agent index
agents/experts/{your-domains}/     # Your domain expertise, tips, agents
.cache/                            # Session data (gitignored)
```

### Symlinks (tracked by git, refreshed by `cuite link`)

```
hooks -> cuite/hooks
commands -> cuite/commands
template.md -> cuite/template.md
teammates.md -> cuite/teammates.md
agents/templates -> ../cuite/agents/templates
agents/build-agent.md -> ../cuite/agents/build-agent.md
agents/review-agent.md -> ../cuite/agents/review-agent.md
agents/scout-agent.md -> ../cuite/agents/scout-agent.md
agents/experts/agent-teams-blueprint.md -> ../../cuite/agents/experts/agent-teams-blueprint.md
```

---

## Makefile Helper (Optional)

```makefile
CUITE := .claude/cuite/bin/cuite

cuite-pull:
	$(CUITE) pull

cuite-init:
	$(CUITE) init

cuite-status:
	$(CUITE) status

cuite-settings:
	$(CUITE) settings
```

---

## FAQ

**Q: Will `subtree pull` overwrite my domain experts?**
No. Your domains live at `.claude/agents/experts/my-domain/`, outside the subtree prefix `.claude/cuite/`.

**Q: Will `subtree push` include my project files?**
No. Only `.claude/cuite/` is pushed. Settings, domain-map, and experts are outside.

**Q: Do symlinks survive `git clone`?**
Yes. Git stores symlinks as their target path. Cloning restores them correctly.

**Q: What if symlinks break after a pull?**
Run `cuite link` to refresh all symlinks.

**Q: How do I see what changed?**

```bash
cuite log               # recent framework commits
cuite diff              # uncommitted changes
git diff HEAD~1 -- .claude/cuite/   # last pull delta
```

**Q: Should I commit `.claude/settings.local.json`?**
No. `cuite init` adds it to `.gitignore` automatically.

**Q: How do hooks resolve paths with symlinks?**
All hooks use `$PWD` (project root, guaranteed by Claude Code) instead of `$0`-based path resolution. Works regardless of symlink traversal.
