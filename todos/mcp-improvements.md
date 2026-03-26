# argsh MCP Server — Research Findings & Recommendations

> Research date: 2026-03-26
> Scope: `builtin/src/usage/mcp.rs`, MCP specification 2025-11-25

---

## 1. Current State of the argsh MCP Server

### What it does today

The argsh MCP server (`builtin/src/usage/mcp.rs`) converts any argsh-structured Bash
script into an MCP server accessible over stdio. It is invoked via the built-in
subcommand `mcp` that is automatically available on every argsh script:

```bash
./my-script mcp
```

The server is implemented as a Rust cdylib loaded into Bash as a native builtin
(`:usage::mcp`). The complete JSON-RPC loop runs inside the Bash process without
forking a separate server — it reads from stdin and writes to stdout line-by-line.

### Protocol compliance

Declared protocol version: `2025-11-25` (current).

Implemented JSON-RPC methods:

| Method | Handled |
|--------|---------|
| `initialize` | Yes — returns `serverInfo` + `capabilities: { tools: {} }` |
| `notifications/initialized` | Yes — no-op |
| `ping` | Yes — returns `{}` |
| `tools/list` | Yes — derives tool list from argsh `usage` array |
| `tools/call` | Yes — spawns script as subprocess with CLI args |
| Any other method | `Method not found` error (-32601) |

### Tool schema generation

Tools are derived from the argsh script's `usage` and `args` arrays:

- **Subcommands** (`local -a usage=(...)`) become individual tools named
  `{script}_{subcommand}`, e.g. `mcp_test_serve`, `mcp_test_build`.
- **Flags** (`local -a args=(...)`) from the *top-level* `args` array become the
  `inputSchema.properties` for every tool.
- **Types** are mapped: `int` → `integer`, `float` → `number`, booleans → `boolean`,
  everything else → `string`.
- **Required flags** are included in the `inputSchema.required` array.
- **Tool execution**: invokes the script as a subprocess (`Command::new(script_path)`)
  with flags translated back to CLI arguments (e.g. `{"verbose": true}` → `--verbose`).

### Known structural limitations

1. **Nested/sub-subcommands are not discovered.** `extract_subcommands()` only reads
   the top-level `usage` array from the current shell scope (`mcp.rs:117`). For a script
   with `cmd tilt up`, only `tilt` is seen — producing a `cmd_tilt` tool but **no**
   `cmd_tilt_up` or `tilt_up`. Nested subcommands live inside functions (`tilt()`) that
   haven't been called, so their `usage` arrays are invisible to the MCP server.

   **Example:** Given this script structure:
   ```bash
   main() {
     local -a usage=('tilt' "Tilt commands")
     :usage "My CLI" "${@}"; "${usage[@]}"
   }
   main::tilt() {
     local -a usage=('up' "Tilt up"  'down' "Tilt down")
     :usage "Tilt commands" "${@}"; "${usage[@]}"
   }
   tilt::up() {
     local -a args=('verbose' "Verbose output")
     :args "Tilt up"
   }
   ```
   MCP exposes: `cmd_tilt` (only)
   Should expose: `cmd_tilt_up`, `cmd_tilt_down` (leaf commands the LLM can actually call)

2. **All tools share the same flag schema.** The `flags` extracted at startup are
   from the top-level `args` array (`mcp.rs:118`). Per-subcommand flags (the `args`
   array inside each subcommand function) are not reflected — the tool schema shows the
   parent's flags for every subcommand.

   **Example:** If `serve` has `--port` and `build` has `--output`, both tools get both
   flags in their `inputSchema` — which misleads the LLM into passing `--port` to `build`.

3. **No capability declaration for anything except `tools: {}`.**
   Capabilities for `resources`, `prompts`, and `logging` are absent.

3. **No `title` field on tools.** The spec defines a human-readable `title` separate
   from `description`. Currently only `description` is emitted.

4. **No tool `annotations`.** The spec supports `readOnlyHint`, `destructiveHint`,
   `idempotentHint`, and `openWorldHint` to help clients decide whether to prompt the
   user before execution. None are emitted.

5. **No `outputSchema`.** Tools always return unstructured `text` content. The spec
   allows defining an `outputSchema` so clients can validate structured JSON responses.

6. **No `listChanged` in the tools capability.** Since the tool list is static (derived
   once at startup), this is correct — but it is worth making explicit for client
   compatibility.

7. **Tool execution returns only stdout/stderr as a single text block.** There is no
   support for returning structured content (JSON), resource links, or embedded resources
   even though the spec supports them in tool results.

8. **No `additionalProperties: false` on empty `inputSchema`.** For tools with no
   flags, the current code emits `"properties":{},"required":[]` without
   `"additionalProperties": false`, which is technically valid but the spec recommends
   the explicit form for no-parameter tools.

9. **`--help` output references `.mcp.json` but shows only stdio transport.** The
   generated config snippet always uses `stdio` mode. HTTP/SSE transport is not
   mentioned.

---

## 2. MCP Protocol Capabilities (2025-11-25 Spec)

The MCP spec defines server-side and client-side primitives.

### Server-side primitives

#### 2.1 Tools (currently implemented — partially)

The core primitive argsh already supports. Full spec support includes:

- `tools/list` with cursor-based **pagination**
- `tools/call` with structured output (`structuredContent` + `outputSchema`)
- Tool **annotations** (`readOnlyHint`, `destructiveHint`, `idempotentHint`,
  `openWorldHint`)
- Tool **titles** (human-readable display name, separate from `name` and `description`)
- Tool **icons** (for display in client UIs)
- Tool **execution.taskSupport** for async/deferred tasks
- `notifications/tools/list_changed` notification

#### 2.2 Resources (not implemented)

Resources expose read-only data to the AI client — files, databases, configuration.
Unlike tools, resources are application-controlled context attachments, not model-invoked
actions.

Methods:
- `resources/list` — enumerate available resources (with pagination)
- `resources/read` — fetch resource content (text or binary blob)
- `resources/templates/list` — URI-templated parameterized resources
- `resources/subscribe` / `resources/unsubscribe` — live update subscriptions
- `notifications/resources/list_changed` — list changed
- `notifications/resources/updated` — specific resource changed

Capability declaration:
```json
{ "capabilities": { "resources": { "subscribe": true, "listChanged": true } } }
```

Resources include optional **annotations** (`audience`, `priority`, `lastModified`).

#### 2.3 Prompts (not implemented)

Prompts are user-triggered reusable instruction templates. They appear as slash commands
in clients like Claude Desktop or Cursor.

Methods:
- `prompts/list` — list available prompt templates (with pagination)
- `prompts/get` — get a prompt with interpolated arguments
- `notifications/prompts/list_changed`

A prompt has named arguments (with `required` flag) and returns structured messages
(role: `user`/`assistant`, content: text, image, audio, or embedded resource).

#### 2.4 Logging (not implemented)

The server can send log messages to the client:

- Client sends `logging/setLevel` to set minimum severity
- Server sends `notifications/message` with fields: `level`, `logger`, `data`
- Levels: `debug`, `info`, `notice`, `warning`, `error`, `critical`, `alert`, `emergency`

This is especially useful during tool execution — a long-running tool can stream
progress messages back without polluting stdout.

#### 2.5 Completion (not implemented)

Servers can offer argument auto-completion for prompts and resource template parameters:

- `completion/complete` — given a prompt/resource ref + partial argument value, return
  candidate completions

### Client-side primitives (offered to the server by the client)

These are capabilities the MCP *client* offers back to the *server*:

- **Sampling**: server requests LLM completions from the client (server-initiated AI
  reasoning)
- **Roots**: client declares filesystem boundaries the server may access
- **Elicitation**: server requests additional input from the user mid-session

argsh runs as a *server*, so these are features to be aware of (and potentially use)
rather than implement.

---

## 3. Concrete Recommendations

The recommendations below are ordered by impact-to-effort ratio (highest first).

### Rec 1 — Nested subcommand discovery (Critical Impact, Medium Effort)

**Problem:** `extract_subcommands()` only reads the top-level `usage` array from the
current shell scope. For scripts with nested commands like `cmd tilt up`, only the
first level (`tilt`) is discovered — producing `cmd_tilt` as a tool but never
`cmd_tilt_up`. The LLM sees a `cmd_tilt` tool that it can call, but `tilt` is just
a dispatcher — calling it without a sub-subcommand shows help text or errors.

**Root cause:** The MCP server reads the `usage` array in-process at startup
(`mcp.rs:117`). Nested subcommands are declared inside functions (e.g. `tilt()`) that
haven't been called yet, so their `usage` arrays don't exist in the current scope.

**Cross-cutting impact:** This is not MCP-specific. Every docgen format (man, md, rst,
yaml, LLM) uses the same `extract_subcommands(usage_pairs)` + `extract_flags(args_arr)`
pattern (`docgen.rs:142-143`, `:234-235`, `:299-300`, `:376-377`, `:427-428`, `:467-468`).
**All generated documentation suffers from the same limitation** — man pages, markdown,
YAML, and LLM tool schemas only show first-level subcommands with shared top-level flags.
Fixing this once in a shared tree-walking function benefits all output formats.

**Solution — In-process function tree walking** (recommended):

All functions are **already loaded** in the Bash process when the MCP server starts.
The `shell::get_all_function_names()` FFI binding returns every visible function name.
The key insight is that function resolution must **replay the same logic** that `:usage`
dispatch uses at runtime (`mod.rs:210-303`), not just guess function names.

**Function resolution rules** (from `:usage` dispatch):

Usage array entries have two forms:
- `'name|alias'` → auto-resolve via namespace fallback
- `'name|alias:-custom::func'` → explicit mapping to `custom::func`

Auto-resolution order (when no `:-` mapping):
1. `{caller}::{name}` — full caller prefix (e.g. `main::tilt`)
2. `{last_segment}::{name}` — last `::` segment of caller (e.g. `tilt::up`)
3. `argsh::{name}` — framework namespace
4. `{name}` — bare function name

The tree-walking algorithm:

1. Start with the top-level `usage` array (already available in scope)
2. For each entry, resolve the **actual function name** using the resolution rules above
   (check `:-` explicit mapping first, then try the 4-step namespace fallback using
   `shell::function_exists()`)
3. Get the function body via `shell::run_bash("declare -f {func}")` and parse:
   - If body contains `local -a usage=(...)` → it's a **dispatcher**, extract the
     usage pairs and recurse (step 2) with this function as the new caller context
   - If body contains `local -a args=(...)` → extract flags for this function's
     `inputSchema`
   - If it has `usage` but no `args` → dispatcher only, its parent's flags apply
   - If it has `args` but no `usage` → it's a **leaf** command (a callable tool)
4. Build the full tool tree: leaf tools get names like `cmd_tilt_up`, `cmd_tilt_down`,
   each with their own `inputSchema` derived from their function's `args` array

**Example with explicit mapping:**

```bash
main() {
  local -a usage=(
    'up|u'              "Start cluster"      # resolves to main::up
    'down:-other::down' "Stop cluster"       # resolves to other::down (explicit)
  )
  :usage "Title" "${@}"; "${usage[@]}"
}
main::up() {
  local -a args=('verbose|v:+' "Verbose output")
  :args "Start cluster" "${@}"
}
other::down() {
  local -a args=('force|f:+' "Force stop")
  :args "Stop cluster" "${@}"
}
```

Tree walking produces:
- `cmd_up` → leaf, `inputSchema: { verbose: boolean }`
- `cmd_down` → leaf, `inputSchema: { force: boolean }`

Key advantages over subprocess spawning:
- **No side effects** — no user code is executed, just function introspection
- **Fast** — in-process FFI calls, no fork/exec overhead
- **Correct** — reuses the exact same resolution logic as runtime dispatch
- **Already available** — `get_all_function_names()`, `function_exists()`, and
  `run_bash("declare -f ...")` are all proven FFI bindings in `shell.rs`
- **Fixes all formats at once** — the tree-walking function can be shared by MCP,
  docgen (man/md/rst/yaml), and LLM schema generation

The `declare -f funcname` approach outputs the function body as text, which can be
parsed with a regex to extract `local -a usage=(...)` and `local -a args=(...)`
declarations. This avoids executing any user code.

**Edge cases to handle:**
- `#hidden` prefix: hidden commands should be excluded from MCP tools but still
  traversed for nested subcommands (a hidden dispatcher may have visible leaves)
- `-` group separators: skip (not commands)
- Aliases (`up|u`): use the first name for tool naming, ignore aliases
- Deferred builtins (`completion`, `docgen`, `mcp`): exclude from tool list

**Tool execution impact:** `build_cli_args()` already prepends the subcommand name —
for nested tools, it must prepend the full path: `["tilt", "up", "--flag", "val"]`.

**Impact:** Without this, LLMs cannot invoke leaf commands in nested CLIs — which is
the majority of real-world argsh scripts.

### Rec 2 — Per-subcommand flag schemas (High Impact, Medium Effort)

**Problem:** Every tool in `tools/list` currently inherits the top-level `args` array
(`mcp.rs:118`). This means if `serve` accepts `--port` but `build` does not, the LLM
is told both tools accept `--port`.

**Solution:** The in-process function tree walking from Rec 1 naturally solves this.
When parsing `declare -f serve` to check for nested `usage` arrays, simultaneously
extract the `args` array declarations. Each leaf function's `local -a args=(...)`
becomes its per-tool `inputSchema`. No subprocess spawning needed — it's all
`declare -f` text parsing within the same Bash process.

**Impact:** Dramatically more accurate tool schemas; fewer LLM hallucinations when
calling tools.

### Rec 2 — Tool annotations (Medium Impact, Low Effort)

**Problem:** Clients (e.g. Claude Desktop) use `annotations` to decide whether to show
a confirmation dialog before tool execution. Without them, every tool is treated as
potentially destructive.

**Solution:** Add argsh DSL support for tool-level hints in the `usage` array. For
example:

```bash
local -a usage=(
  'serve:readonly'   "Start the read-only status server"
  'build:destructive' "Build and overwrite output directory"
)
```

Map these to MCP annotations:

| argsh hint | MCP annotation |
|-----------|----------------|
| `:readonly` | `readOnlyHint: true` |
| `:destructive` | `destructiveHint: true` |
| `:idempotent` | `idempotentHint: true` |

Emit them in `format_tool`:
```json
"annotations": { "readOnlyHint": true }
```

**Impact:** Better user safety in AI client UIs; enables "run without confirmation" for
read-only tools.

### Rec 3 — Title field on tools (Low Impact, Near-Zero Effort)

**Problem:** The spec defines a `title` field for human-readable display names separate
from `name` (which is machine-constrained to `[a-zA-Z0-9_\-.]`).

**Solution:** Emit the subcommand's description as `title` and the first sentence as
`description`:

```json
{
  "name": "mcp_test_serve",
  "title": "Start the server",
  "description": "Start the server"
}
```

Change `format_tool` to add `"title":"{desc}"` after `"name"`.

**Impact:** Better display in graphical MCP clients.

### Rec 4 — Structured output with outputSchema (Medium Impact, Medium Effort)

**Problem:** Tool results are always plain text. When a subcommand outputs JSON, the LLM
must parse it from a text string, which is error-prone.

**Solution:** Support an opt-in `outputSchema` in the usage array:

```bash
local -a usage=(
  'status:json' "Get service status as JSON"
)
```

When a tool returns with `:json`, parse stdout as JSON and emit both:
- `content: [{type: "text", text: "<raw json>"}]` (backwards compat)
- `structuredContent: <parsed object>`

And in the tool definition:
```json
"outputSchema": { "type": "object" }
```

**Impact:** Enables LLMs to reliably extract structured data from tool results.

### Rec 5 — Resources: expose script help and docgen output (Medium Impact, Medium Effort)

**Problem:** Currently the LLM can only invoke subcommands. It cannot read the script's
documentation or configuration to understand context before calling tools.

**Solution:** Implement `resources/list` and `resources/read` to expose:

| URI | Content | MIME type |
|-----|---------|-----------|
| `script:///help` | `./script --help` output | `text/plain` |
| `script:///docgen/md` | `./script docgen md` output | `text/markdown` |
| `script:///docgen/yaml` | `./script docgen yaml` output | `application/yaml` |
| `script:///version` | `ARGSH_VERSION` env var | `text/plain` |

Add capability declaration:
```json
{ "capabilities": { "tools": {}, "resources": {} } }
```

Handle `resources/list` and `resources/read` in the JSON-RPC dispatch loop.

**Impact:** LLMs get rich context before calling tools — better first-call accuracy,
fewer "describe yourself" tool calls.

### Rec 6 — Prompts: common invocation templates (Medium Impact, Medium Effort)

**Problem:** Users connect the argsh MCP server but don't know what prompts work well
with it. There are no slash commands / prompt templates.

**Solution:** Implement `prompts/list` and `prompts/get` to expose standard templates:

| Prompt name | Description | Arguments |
|-------------|-------------|-----------|
| `run_subcommand` | "Run a specific subcommand" | `subcommand` (required), `args` (optional) |
| `get_help` | "Show full help for a subcommand" | `subcommand` (optional) |
| `explain_script` | "Explain what this script does" | none |

The `prompts/get` handler renders these templates with the script title/name interpolated.

Add capability:
```json
{ "capabilities": { "tools": {}, "resources": {}, "prompts": {} } }
```

**Impact:** Slash commands in Claude Desktop / Cursor — users can type `/run_subcommand`
and get a structured invocation form. Improves discoverability.

### Rec 7 — Server-sent logging during tool execution (Low Impact, Low Effort)

**Problem:** Long-running tools (e.g. a `build` subcommand) produce no feedback during
execution — the MCP client just waits silently.

**Solution:** Stream stderr from the subprocess as MCP log notifications. After
spawning, read stderr line-by-line and emit:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/message",
  "params": {
    "level": "info",
    "logger": "mcp_test",
    "data": "Building project..."
  }
}
```

This requires switching from `Command::output()` (blocking) to `Command::spawn()` with
separate stdout/stderr handles and interleaved reading.

Declare the logging capability:
```json
{ "capabilities": { "tools": {}, "logging": {} } }
```

**Impact:** Visible progress in AI clients that display MCP log messages. Better UX for
long-running operations.

### Rec 8 — `additionalProperties: false` for empty schemas (Near-Zero Effort)

**Problem:** The spec recommends `{"type":"object","additionalProperties":false}` for
tools with no parameters. Current code emits `{"type":"object","properties":{},"required":[]}`.

**Solution:** In `format_tool`, if `flags.is_empty()`, emit:
```json
"inputSchema": { "type": "object", "additionalProperties": false }
```

**Impact:** Strict schema validation in clients; signals intent clearly.

### Rec 9 — `listChanged: false` explicit declaration (Near-Zero Effort)

**Problem:** The current `capabilities` response emits `"tools":{}`, which is valid but
does not explicitly declare that the tool list is static.

**Solution:** Change the initialize response to:
```json
{ "tools": { "listChanged": false } }
```

**Impact:** Client clarity; some clients may optimize polling behavior based on this.

### Rec 10 — HTTP/SSE transport documentation (Low Effort)

**Problem:** The `--help` output only shows stdio transport. Some AI clients (especially
web-based) need HTTP/SSE.

**Solution:** Add a note in the `--help` output:
```
For HTTP transport, wrap with a stdio-to-HTTP bridge:
  npx @modelcontextprotocol/stdio-to-http-bridge ./my-script mcp
```

**Impact:** Better adoption in non-CLI AI client contexts.

---

## 4. Comparison with Other MCP Servers

| Feature              | argsh (current)     | filesystem MCP | github MCP | stripe MCP |
| -------------------- | ------------------- | -------------- | ---------- | ---------- |
| Tools                | Yes                 | Yes            | Yes        | Yes        |
| Nested subcommands   | No (first level only) | N/A          | N/A        | N/A        |
| Per-tool schemas     | No (shared)         | N/A            | Yes        | Yes        |
| Resources            | No                  | Yes (files)    | Yes (repos)| No         |
| Prompts              | No                  | No             | No         | No         |
| Logging              | No                  | No             | No         | No         |
| Tool annotations     | No                  | Yes            | Partial    | No         |
| outputSchema         | No                  | No             | Partial    | Partial    |
| Pagination           | No                  | Yes            | Yes        | Yes        |

The argsh MCP server is competitive in the tools dimension but lags in the contextual
dimensions (resources, prompts) that make MCP servers feel "rich" in AI client UIs.

The most commonly praised MCP servers (filesystem, GitHub) distinguish themselves by:
1. Returning structured data with well-defined output schemas
2. Exposing resources that give the LLM context before it needs to call a tool
3. Using tool annotations so clients know which tools are safe to auto-invoke

---

## 5. Implementation Priority Matrix

| Recommendation                      | Impact   | Effort | Priority |
| ----------------------------------- | -------- | ------ | -------- |
| Rec 1 — Nested subcommand discovery | Critical | Medium | P0       |
| Rec 2 — Per-subcommand flag schemas | High     | Medium | P0       |
| Rec 3 — Tool annotations            | Medium   | Low    | P1       |
| Rec 4 — Title field                 | Low      | Trivial| P1       |
| Rec 8 — additionalProperties fix    | Low      | Trivial| P1       |
| Rec 9 — listChanged explicit        | Low      | Trivial| P1       |
| Rec 5 — Resources                   | Medium   | Medium | P2       |
| Rec 4 — outputSchema                | Medium   | Medium | P2       |
| Rec 6 — Prompts                     | Medium   | Medium | P3       |
| Rec 7 — Logging                     | Low      | Low    | P3       |
| Rec 10 — HTTP docs                  | Low      | Low    | P3       |

### Suggested P0 sprint (core functionality — makes MCP actually useful)

1. Recursive subcommand discovery via subprocess introspection (Rec 1) — **critical**,
   without this nested CLIs are unusable via MCP
2. Per-subcommand flag schemas (Rec 2) — naturally solved alongside Rec 1

These require changes to the argsh runtime (a new introspection mode) and to `mcp.rs`.

### Suggested P1 sprint (single PR, low risk, additive)

1. Add `title` to tool definitions (Rec 4)
2. Add `"listChanged": false` to tools capability (Rec 9)
3. Use `additionalProperties: false` for empty schemas (Rec 8)
4. Add tool annotations via usage array hint syntax (Rec 3)

These are entirely additive and backward-compatible with all existing MCP clients.

### Suggested P2 sprint

1. Resources for help/docgen output (Rec 5) — high discoverability value
2. Structured outputSchema opt-in (Rec 4)

### Suggested P3 sprint

1. Prompts for slash commands (Rec 6)
2. Structured outputSchema opt-in (Rec 4)
3. Streaming logs during tool execution (Rec 7)

---

## 6. Acceptance Criteria

### P0 — Nested subcommands + per-subcommand schemas

- Given `cmd tilt up`, `tools/list` returns `cmd_tilt_up` (not just `cmd_tilt`)
- Leaf tools have their own `inputSchema` based on their function's `args` array
- `tools/call` with `cmd_tilt_up` passes `["tilt", "up", ...]` to the subprocess
- Non-leaf commands (dispatchers) are excluded from the tool list
- Test fixture with nested subcommands (2+ levels) passes
- Existing flat scripts (no nesting) continue to work unchanged

### P1 — Quick wins

- `tools/list` response includes `"title"` field for every tool
- `initialize` response declares `"tools":{"listChanged":false}`
- Tools with no flags emit `"inputSchema":{"type":"object","additionalProperties":false}`
- Usage entries with `:readonly`, `:destructive`, or `:idempotent` suffixes produce
  correct `"annotations"` in the tool definition
- `mcp_test.sh` fixture covers at least one annotated tool
- Existing tests in `builtin/` still pass (`cargo test`)

---

## References

- [MCP 2025-11-25 Specification](https://modelcontextprotocol.io/specification/2025-11-25)
- [MCP Tools spec](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- [MCP Resources spec](https://modelcontextprotocol.io/specification/2025-11-25/server/resources)
- [MCP Prompts spec](https://modelcontextprotocol.io/specification/2025-11-25/server/prompts)
