# DAP Debugger — Subshell Debugging & Reverse Debugging

## Subshell Debugging

### Problem

Bash creates subshells for:
- `$( command )` — command substitution
- `command | command` — each pipe segment
- `( command )` — explicit subshell
- `{ command; } &` — background execution
- Process substitution `<( command )`

Subshells **inherit** the DEBUG trap, but they share the parent's FIFO handles. If both parent and subshell try to write to the same FIFO simultaneously, or if the subshell blocks waiting for a resume command while the parent is also blocked, we **deadlock**.

### Current approach (v1)

The current prelude detects subshells via `BASH_SUBSHELL > 0` and handles them differently:
- **No blocking**: subshells never block on the FIFO (no `read` from `.ctl`)
- **Non-blocking output**: breakpoint hits in subshells emit a `SUBSHELL` event to the data FIFO (write may silently fail if the FIFO is already occupied by the parent)
- **No stepping**: step commands don't work inside subshells — the subshell runs to completion

### Better approach (v2) — per-subshell FIFOs

Create a unique FIFO pair per subshell level:

```bash
__argsh_dap_trap() {
  local _sublevel="${BASH_SUBSHELL}"
  local _fifo="${__ARGSH_DAP_FIFO}.sub${_sublevel}"
  local _ctl="${_fifo}.ctl"

  # First trap hit in a new subshell: create its FIFO pair
  if [[ ! -p "${_fifo}" ]] && (( _sublevel > 0 )); then
    mkfifo "${_fifo}" "${_ctl}" 2>/dev/null || return 0
    # Notify DAP server about the new subshell
    printf 'SUBSHELL_START\t%d\t%s\t%s\n' \
      "${_sublevel}" "${_fifo}" "${_ctl}" \
      > "${__ARGSH_DAP_FIFO}"
  fi

  # Use the subshell-specific FIFO for communication
  # ... rest of trap handler uses $_fifo and $_ctl ...
}
```

The DAP server would:
1. Spawn a new reader thread per subshell FIFO
2. Map subshell level to a separate DAP thread ID
3. Show subshell execution as a separate thread in VSCode's call stack

### Challenges

1. **FIFO cleanup**: subshells may exit abruptly; need a cleanup trap
2. **Thread ordering**: DAP events from subshells may interleave with parent events
3. **Short-lived subshells**: `$(echo foo)` finishes in microseconds — by the time the DAP server opens the FIFO reader, the subshell may be gone
4. **Pipe deadlocks**: in `cmd1 | cmd2`, both segments are subshells AND connected by a pipe. Blocking either one on a FIFO blocks the other via the pipe.

### Recommendation

For v1 (current): non-blocking subshell events are sufficient. Most debugging happens in the main shell.

For v2 (future): implement per-subshell FIFOs only for `{ ...; } &` (background jobs), which are long-lived and useful to debug. Skip short-lived subshells (`$()`, pipes).

---

## Reverse Debugging (Step Back)

### What it means

Reverse debugging lets you step **backwards** through execution — undo the last command and return to the previous state. GDB supports this for C programs via record-and-replay. For bash, this is much harder because shell state is inherently side-effectful (files created, processes spawned, etc.).

### Approach: checkpoint-based reverse execution

Instead of true reverse execution, implement **checkpoint/restore**:

1. **Recording phase**: at each DEBUG trap hit, save the current shell state:
   - All variable values (`declare -p`)
   - Current line, function, file
   - Working directory (`pwd`)
   - File descriptors (impractical to save)

2. **Storage**: maintain a circular buffer of N recent states (e.g., last 100 commands). This is the "execution history".

3. **Step back**: when the user requests "step back":
   - Send the history entry N-1 to VSCode as the current state
   - Variables panel shows the saved values
   - The stack trace shows the saved position
   - The script is **NOT actually re-executed backwards** — we just show the saved snapshot

4. **Resume from checkpoint**: to continue forward from a past state:
   - Restore all variables via `eval "declare ..."` for each saved declaration
   - `cd` to the saved directory
   - Set a breakpoint at the saved line and continue
   - Side effects (files, network, processes) are **NOT undone**

### Implementation in the prelude

```bash
declare -a __ARGSH_DAP_HISTORY=()
declare -i __ARGSH_DAP_HISTORY_MAX=100
declare -i __ARGSH_DAP_HISTORY_IDX=0

# In the trap handler, before checking breakpoints:
if (( ${#__ARGSH_DAP_HISTORY[@]} >= __ARGSH_DAP_HISTORY_MAX )); then
  # Circular buffer: overwrite oldest entry
  __ARGSH_DAP_HISTORY_IDX=$(( __ARGSH_DAP_HISTORY_IDX % __ARGSH_DAP_HISTORY_MAX ))
fi
__ARGSH_DAP_HISTORY[__ARGSH_DAP_HISTORY_IDX]="$(declare -p 2>/dev/null | base64 -w0)"
(( __ARGSH_DAP_HISTORY_IDX++ ))
```

### Performance concern

`declare -p` on every command is **expensive** — it serializes all variables. Mitigation:
- Only record when stepping (not in free-run mode)
- Limit to local variables in the current scope
- Use a fixed-size circular buffer
- Skip recording for simple commands (assignments, echo)

### DAP protocol

DAP has `supportsStepBack` capability and `stepBack`/`reverseContinue` requests. When the user clicks "Step Back":

1. DAP server receives `stepBack` request
2. Server pops the last history entry
3. Server sends `StoppedEvent(reason: "step")` with the saved position
4. `stackTrace` returns the saved stack
5. `variables` returns the saved variable values

The script doesn't actually move — only the displayed state changes. The user sees the past. If they press "Continue" or "Step Over", the script resumes from its **actual** current position (not the displayed one), which may be confusing.

### Alternative: replay-based

A more robust approach:
1. Record the script's stdin, environment, and arguments at launch
2. On "step back": restart the script from the beginning with the same inputs
3. Set a breakpoint at (current_step - 1) and run at full speed to reach it
4. This gives true reverse execution but is slow for long-running scripts

### Recommendation

- **v1**: Don't implement reverse debugging. The complexity vs. value ratio is poor for shell scripts.
- **v2**: If demanded, implement the checkpoint-based approach with recording only during step mode. Accept the limitation that side effects aren't undone.
- **v3**: Replay-based approach for scripts that are deterministic and fast.

Mark `supportsStepBack: false` in DAP capabilities (current state) and revisit based on user feedback.
