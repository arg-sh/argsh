//! argsh-dap — Debug Adapter Protocol server for argsh scripts.
//!
//! Uses bash's built-in DEBUG trap for breakpoints and stepping — no bashdb
//! dependency required. Communicates with the debug target via named pipes
//! (FIFOs) for synchronization.
//!
//! Protocol: DAP over stdin/stdout (Content-Length framed JSON, same as LSP).
//!
//! Usage (invoked by VSCode, not directly):
//!   argsh-dap
//!   argsh-dap --version
//!   argsh-dap --help
//!
//! Platform: requires Unix (named pipes). On non-Unix platforms the binary
//! compiles but exits with an error — same limitation as argsh-lsp.

// Issue #14: cross-platform build — the build script (build.rs) shares the same
// Unix-only limitation as argsh-lsp. This is acceptable for now since argsh
// targets bash, which is inherently Unix.

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// argsh analysis — shared with LSP and argsh-lint.
// Used for smart breakpoints (#92), args inspector (#93), import-aware
// source mapping (#95), and variable type tooltips (#97).
use argsh_lsp::resolver;
use argsh_syntax::document::{analyze, DocumentAnalysis, FunctionInfo};
#[allow(unused_imports)]
use argsh_syntax::field::FieldDef;

// ---------------------------------------------------------------------------
// DAP types (hand-rolled — the `dap` crate is alpha and adds unnecessary
// complexity. DAP is simple JSON-over-stdio, same framing as LSP.)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DapMessage {
    seq: i64,
    #[serde(rename = "type")]
    msg_type: String,
    command: Option<String>,
    arguments: Option<Value>,
}

#[derive(Debug, Serialize)]
struct DapResponse {
    seq: i64,
    #[serde(rename = "type")]
    msg_type: &'static str,
    request_seq: i64,
    success: bool,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct DapEvent {
    seq: i64,
    #[serde(rename = "type")]
    msg_type: &'static str,
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<Value>,
}

// ---------------------------------------------------------------------------
// Debug prelude — injected into the bash script to enable DEBUG trap
// ---------------------------------------------------------------------------

const DEBUG_PRELUDE: &str = r#"
# ── argsh debug prelude ──────────────────────────────────────────────────
# Injected by argsh-dap. Enables step-through debugging via bash's DEBUG trap.
# Communicates with the DAP server via named pipes (FIFOs).

__ARGSH_DAP_FIFO="__FIFO_PATH__"
__ARGSH_DAP_WRAPPER="__WRAPPER_PATH__"
__ARGSH_DAP_STEP=0        # 0=run, 1=stepin, 2=next, 3=stepout
__ARGSH_DAP_DEPTH=0        # saved depth for next/stepout
__ARGSH_DAP_STOP_ENTRY=__STOP_ON_ENTRY__
__ARGSH_DAP_LOCK=""        # flock file descriptor (set during init)
__ARGSH_DAP_CTL_FD=""      # persistent fd for control FIFO (issue #9)
declare -a __ARGSH_DAP_BPS=()     # breakpoints: "file:line" entries
declare -A __ARGSH_DAP_BP_COND=() # conditional breakpoints: "file:line" → condition
declare -a __ARGSH_DAP_WATCH=()   # watch expressions

# Unit separator used as delimiter in the condition protocol (issue #2).
# Avoids breakage when file paths contain colons (e.g. Windows-style or
# unusual Unix paths).
__ARGSH_DAP_US=$'\x1f'

# Initialize the lock file for flock-based FIFO serialization.
# Subshells inherit the fd, so both parent and child can acquire the lock.
exec {__ARGSH_DAP_LOCK}>"${__ARGSH_DAP_FIFO}.lock"

# Subshell cleanup: each subshell creates a per-PID control FIFO.
# Clean it up on exit.
__argsh_dap_cleanup() {
  local _ctl="${__ARGSH_DAP_FIFO}.ctl.$$"
  [[ ! -p "${_ctl}" ]] || rm -f "${_ctl}"
  # Close persistent control fd if open
  if [[ -n "${__ARGSH_DAP_CTL_FD}" ]]; then
    eval "exec ${__ARGSH_DAP_CTL_FD}<&-" 2>/dev/null || true
    __ARGSH_DAP_CTL_FD=""
  fi
}
trap '__argsh_dap_cleanup' EXIT

__argsh_dap_trap() {
  # In a function-based DEBUG trap, BASH_SOURCE[0] is where the trap
  # function is defined (the wrapper), not where the triggering command
  # is. BASH_SOURCE[1] + BASH_LINENO[0] give the correct caller context.
  local _file="${BASH_SOURCE[1]:-${0}}"
  local _line="${BASH_LINENO[0]}"
  local _func="${FUNCNAME[1]:-main}"
  local _depth=${#FUNCNAME[@]}
  local _should_stop=0
  local _is_subshell=0
  local _ctl_fifo

  # Skip trap events from the wrapper script itself (the prelude and
  # argsh runtime loader). Only fire for the user's sourced script and
  # any files it imports/sources.
  [[ "${_file}" != "${__ARGSH_DAP_WRAPPER}" ]] || return 0

  # Determine which control FIFO to use:
  # - Main shell (BASH_SUBSHELL==0): uses the primary .ctl FIFO
  # - Subshell (BASH_SUBSHELL>0): uses a per-PID .ctl.$$ FIFO
  #   This avoids deadlock: the parent blocks on its .ctl, the subshell
  #   blocks on .ctl.$$, and the DAP server writes to the correct one.
  if (( BASH_SUBSHELL > 0 )); then
    _is_subshell=1
    _ctl_fifo="${__ARGSH_DAP_FIFO}.ctl.$$"
    # Create per-PID control FIFO on first use in this subshell
    if [[ ! -p "${_ctl_fifo}" ]]; then
      mkfifo "${_ctl_fifo}" 2>/dev/null || return 0
    fi
  else
    _ctl_fifo="${__ARGSH_DAP_FIFO}.ctl"
  fi

  # Stop on entry (first trap hit, main shell only)
  if (( __ARGSH_DAP_STOP_ENTRY && ! _is_subshell )); then
    __ARGSH_DAP_STOP_ENTRY=0
    _should_stop=1
  fi

  # Check step mode
  case ${__ARGSH_DAP_STEP} in
    1) _should_stop=1 ;;  # stepin: always stop
    2) (( _depth <= __ARGSH_DAP_DEPTH )) && _should_stop=1 ;;  # next
    3) (( _depth < __ARGSH_DAP_DEPTH )) && _should_stop=1 ;;   # stepout
  esac

  # Check breakpoints (with conditional support)
  if (( ! _should_stop )); then
    local _bp _key
    for _bp in "${__ARGSH_DAP_BPS[@]}"; do
      if [[ "${_bp}" == "${_file}:${_line}" ]]; then
        _key="${_file}:${_line}"
        if [[ -n "${__ARGSH_DAP_BP_COND[${_key}]+x}" ]]; then
          # Conditional breakpoint: evaluate the condition
          local _cond="${__ARGSH_DAP_BP_COND[${_key}]}"
          if eval "${_cond}" 2>/dev/null; then
            _should_stop=1
          fi
        else
          _should_stop=1
        fi
        break
      fi
    done
  fi

  if (( _should_stop )); then
    # Build stack trace
    local _stack="" _i
    for (( _i=1; _i < ${#FUNCNAME[@]}; _i++ )); do
      _stack+="${BASH_SOURCE[_i]:-?}\t${BASH_LINENO[_i-1]}\t${FUNCNAME[_i]:-?}\n"
    done

    # Evaluate watch expressions
    local _watches=""
    local _wexpr _wval
    for _wexpr in "${__ARGSH_DAP_WATCH[@]}"; do
      _wval="$(eval "echo ${_wexpr}" 2>/dev/null || echo "<error>")"
      _watches+="WATCH\t${_wexpr}\t${_wval}\n"
    done

    # Capture subshell level BEFORE the flock subshell — the flock
    # runs in ( ... ) which increments BASH_SUBSHELL by 1.
    local _subshell_level="${_is_subshell}"

    # Write stop event to FIFO under flock to prevent interleaving.
    (
      flock "${__ARGSH_DAP_LOCK}"
      printf 'STOPPED\t%s\t%s\t%s\t%d\t%d\n%b%b' \
        "${_file}" "${_line}" "${_func}" "$$" "${_subshell_level}" \
        "${_stack}" "${_watches}" \
        > "${__ARGSH_DAP_FIFO}"
    )

    # Block until DAP server sends a resume command on OUR control FIFO.
    # Issue #9: Use a persistent file descriptor to keep the FIFO open
    # across reads. Redirecting from the FIFO path directly causes EOF
    # after each non-resume command, breaking the read loop.
    if [[ -z "${__ARGSH_DAP_CTL_FD}" ]] || ! { true >&"${__ARGSH_DAP_CTL_FD}"; } 2>/dev/null; then
      exec {__ARGSH_DAP_CTL_FD}<"${_ctl_fifo}"
    fi

    local _cmd
    while IFS= read -r _cmd <&"${__ARGSH_DAP_CTL_FD}"; do
      case "${_cmd}" in
        continue)
          __ARGSH_DAP_STEP=0
          break
          ;;
        stepin)
          __ARGSH_DAP_STEP=1
          break
          ;;
        next)
          __ARGSH_DAP_STEP=2
          __ARGSH_DAP_DEPTH=${_depth}
          break
          ;;
        stepout)
          __ARGSH_DAP_STEP=3
          __ARGSH_DAP_DEPTH=${_depth}
          break
          ;;
        breakpoints:*)
          # Update breakpoints: "breakpoints:file:1,file:5,file:10"
          # NOTE: intentionally no 'break' here — the trap stays blocked
          # waiting for a resume command (continue/stepin/next/stepout).
          # Breakpoint updates are applied while stopped.
          local _bpdata="${_cmd#breakpoints:}"
          __ARGSH_DAP_BPS=()
          if [[ -n "${_bpdata}" ]]; then
            IFS=',' read -ra __ARGSH_DAP_BPS <<< "${_bpdata}"
          fi
          ;;
        condition${__ARGSH_DAP_US}*)
          # Set conditional breakpoint: "condition\x1ffile\x1fline\x1fexpression"
          # Issue #2: uses unit separator (\x1f) instead of colon to avoid
          # breaking on colons in file paths.
          local _cdata="${_cmd#condition${__ARGSH_DAP_US}}"
          local _cfile _cline _cexpr
          IFS="${__ARGSH_DAP_US}" read -r _cfile _cline _cexpr <<< "${_cdata}"
          __ARGSH_DAP_BP_COND["${_cfile}:${_cline}"]="${_cexpr}"
          ;;
        watch:*)
          # Add watch expression: "watch:expression"
          local _wdata="${_cmd#watch:}"
          __ARGSH_DAP_WATCH+=("${_wdata}")
          ;;
        unwatch:*)
          # Remove watch expression: "unwatch:expression"
          local _uwdata="${_cmd#unwatch:}"
          local _new_watches=()
          for _w in "${__ARGSH_DAP_WATCH[@]}"; do
            [[ "${_w}" != "${_uwdata}" ]] && _new_watches+=("${_w}")
          done
          __ARGSH_DAP_WATCH=("${_new_watches[@]}")
          ;;
        setvar:*)
          # Modify variable at runtime: "setvar:name=value"
          # Issue #1/#5/#14: Use printf -v for safe assignment instead of eval.
          # Parse name=value with parameter expansion to avoid injection.
          local _svdata="${_cmd#setvar:}"
          local _name="${_svdata%%=*}"
          local _value="${_svdata#*=}"
          # Validate variable name: must be a valid bash identifier
          if [[ "${_name}" =~ ^[a-zA-Z_][a-zA-Z0-9_]*$ ]]; then
            printf -v "${_name}" '%s' "${_value}"
          fi
          ;;
        eval:*)
          # Evaluate expression and return result: "eval:expression"
          local _edata="${_cmd#eval:}"
          local _eresult
          _eresult="$(eval "${_edata}" 2>&1)" || true
          printf 'EVAL\t%s\n' "${_eresult}" > "${__ARGSH_DAP_FIFO}"
          ;;
        vars)
          # Dump variables to FIFO for inspection
          # NOTE (issue #3): Locals scope shows variables only when the script
          # is stopped (requires FIFO round-trip). Variables are read via
          # `declare -p` which reflects the current scope at the trap callsite.
          {
            declare -p 2>/dev/null
            printf 'ENDVARS\n'
          } > "${__ARGSH_DAP_FIFO}"
          ;;
      esac
    done
  fi

  return 0
}

trap '__argsh_dap_trap' DEBUG
# ── end debug prelude ────────────────────────────────────────────────────
"#;

// ---------------------------------------------------------------------------
// Trace prelude — injected into bash for headless `--trace` mode.
// Similar to the DEBUG trap but writes events to the FIFO without blocking
// for resume commands. Every trap hit writes a TRACE event and returns
// immediately, allowing the script to run to completion uninterrupted.
// ---------------------------------------------------------------------------

const TRACE_PRELUDE: &str = r#"
# ── argsh trace prelude ─────────────────────────────────────────────────
# Injected by argsh-dap --trace. Collects execution events via FIFO.
# Unlike the debug prelude, this does NOT block — it fires and forgets.

__ARGSH_TRACE_FIFO="__FIFO_PATH__"
__ARGSH_TRACE_WRAPPER="__WRAPPER_PATH__"
__ARGSH_TRACE_LOCK=""
__ARGSH_TRACE_DEPTH=0
__ARGSH_TRACE_PREV_FUNC=""
__ARGSH_TRACE_FUNC_STACK=()

exec {__ARGSH_TRACE_LOCK}>"${__ARGSH_TRACE_FIFO}.lock"

__argsh_trace_trap() {
  local _file="${BASH_SOURCE[1]:-${0}}"
  local _line="${BASH_LINENO[0]}"
  local _func="${FUNCNAME[1]:-main}"
  local _depth=$(( ${#FUNCNAME[@]} - 1 ))
  local _cmd="${BASH_COMMAND}"

  # Skip events from the wrapper script itself
  [[ "${_file}" != "${__ARGSH_TRACE_WRAPPER}" ]] || return 0

  # Detect function entry/exit by depth changes
  local _event_type="step"
  if (( _depth > __ARGSH_TRACE_DEPTH )); then
    _event_type="enter"
    __ARGSH_TRACE_FUNC_STACK+=("${_func}")
  elif (( _depth < __ARGSH_TRACE_DEPTH )); then
    _event_type="exit"
    # Pop from our stack
    if (( ${#__ARGSH_TRACE_FUNC_STACK[@]} > 0 )); then
      _func="${__ARGSH_TRACE_FUNC_STACK[-1]}"
      unset '__ARGSH_TRACE_FUNC_STACK[-1]'
    fi
  fi
  __ARGSH_TRACE_DEPTH=${_depth}

  # Detect :args and :usage calls
  local _special=""
  case "${_cmd}" in
    :args*|": args"*) _special="args" ;;
    :usage*|": usage"*) _special="usage" ;;
  esac

  # Write event to FIFO under flock to prevent interleaving
  (
    flock "${__ARGSH_TRACE_LOCK}"
    printf 'TRACE\t%s\t%s\t%s\t%s\t%d\t%s\t%s\n' \
      "${_event_type}" "${_file}" "${_line}" "${_func}" "${_depth}" \
      "${_cmd}" "${_special}" \
      > "${__ARGSH_TRACE_FIFO}"
  )

  return 0
}

trap '__argsh_trace_trap' DEBUG

# Dump variables on exit for final state capture
__argsh_trace_exit() {
  (
    flock "${__ARGSH_TRACE_LOCK}"
    printf 'TRACE_EXIT\t%d\n' "$?" > "${__ARGSH_TRACE_FIFO}"
  )
}
trap '__argsh_trace_exit' EXIT
# ── end trace prelude ───────────────────────────────────────────────────
"#;

// ---------------------------------------------------------------------------
// Trace mode types — used by `--trace` to collect and render events
// ---------------------------------------------------------------------------

/// A single trace event collected from the FIFO during `--trace` mode.
#[derive(Debug, Clone)]
struct TraceEvent {
    /// Event type: "enter", "exit", or "step".
    event_type: String,
    /// Source file path.
    file: String,
    /// Line number in the source file.
    line: u32,
    /// Function name.
    func: String,
    /// Call depth (0 = top-level).
    depth: u32,
    /// The bash command being executed.
    command: String,
    /// Special marker: "args", "usage", or empty.
    special: String,
    /// Timestamp relative to trace start (milliseconds).
    elapsed_ms: u64,
}

/// Information about a function call extracted from trace events.
#[derive(Debug, Clone)]
struct TracedFunction {
    name: String,
    file: String,
    entry_line: u32,
    depth: u32,
    entry_ms: u64,
    exit_ms: Option<u64>,
    steps: Vec<TraceEvent>,
    has_args_call: bool,
    has_usage_call: bool,
}

// ---------------------------------------------------------------------------
// DAP Session
// ---------------------------------------------------------------------------

struct DapSession {
    seq: Arc<AtomicI64>,
    breakpoints: HashMap<PathBuf, HashSet<u32>>,
    child: Option<Child>,
    fifo_path: Option<PathBuf>,
    launched: Arc<AtomicBool>,
    stdout_writer: Arc<Mutex<io::Stdout>>,
    // argsh analysis (#92-#97): cached document analysis for the launched script
    // and its imports, enabling smart breakpoints, args inspection, and type tooltips.
    analysis: Option<DocumentAnalysis>,
    imports: Option<resolver::ResolvedImports>,
    program_path: Option<PathBuf>,
    program_content: Option<String>,
    // Last stack trace from a STOPPED event, used by handle_stack_trace.
    last_stack_frames: Arc<Mutex<Vec<Value>>>,
    // Issue #4/#10: mapping from DAP threadId to bash PID, so continue/step
    // commands are routed to the correct per-PID control FIFO.
    // threadId 1 = main shell (no PID suffix), others = subshells.
    thread_pids: Arc<Mutex<HashMap<i64, u64>>>,
    // Issue #11: set of currently active thread IDs (main + subshells).
    active_threads: Arc<Mutex<HashMap<i64, String>>>,
}

impl DapSession {
    fn new() -> Self {
        let mut active_threads = HashMap::new();
        active_threads.insert(1, "main".to_string());
        Self {
            seq: Arc::new(AtomicI64::new(1)),
            breakpoints: HashMap::new(),
            child: None,
            fifo_path: None,
            launched: Arc::new(AtomicBool::new(false)),
            stdout_writer: Arc::new(Mutex::new(io::stdout())),
            analysis: None,
            imports: None,
            program_path: None,
            program_content: None,
            last_stack_frames: Arc::new(Mutex::new(Vec::new())),
            thread_pids: Arc::new(Mutex::new(HashMap::new())),
            active_threads: Arc::new(Mutex::new(active_threads)),
        }
    }

    /// Analyze the program source and resolve imports.
    /// Called during launch to enable argsh-specific debug features.
    fn analyze_program(&mut self, program: &Path) {
        if let Ok(content) = std::fs::read_to_string(program) {
            let analysis = analyze(&content);
            let imports = resolver::resolve_imports(
                &analysis,
                program,
                resolver::DEFAULT_MAX_DEPTH,
            );
            self.program_content = Some(content);
            self.analysis = Some(analysis);
            self.imports = Some(imports);
            self.program_path = Some(program.to_path_buf());
        }
    }

    /// (#92) Resolve a subcommand name to a file:line breakpoint.
    /// Walks the :usage dispatch tree to find the target function.
    fn resolve_command_breakpoint(&self, command_name: &str) -> Option<(PathBuf, u32)> {
        let analysis = self.analysis.as_ref()?;
        let program = self.program_path.as_ref()?;

        // Search all functions for :usage entries matching the command name
        for func in &analysis.functions {
            for entry in &func.usage_entries {
                let name = entry.name.split('|').next().unwrap_or(&entry.name);
                // Strip annotations (e.g. "deploy@destructive" → "deploy")
                let clean = name.split('@').next().unwrap_or(name);
                if clean == command_name {
                    // The entry itself is on a line — but we want the target function.
                    // Try to find the target function in the analysis.
                    let target_name = if let Some(ref explicit) = entry.explicit_func {
                        explicit.clone()
                    } else {
                        // Namespace resolution: caller::cmd, last_segment::cmd, argsh::cmd, cmd
                        let caller = &func.name;
                        let candidates = [
                            format!("{}::{}", caller, clean),
                            {
                                let last = caller.rsplit("::").next().unwrap_or(caller);
                                format!("{}::{}", last, clean)
                            },
                            format!("argsh::{}", clean),
                            clean.to_string(),
                        ];
                        candidates.into_iter().find(|c| {
                            analysis.functions.iter().any(|f| f.name == *c)
                                || self.imports.as_ref().is_some_and(|imp| {
                                    imp.functions.iter().any(|f| f.name == *c)
                                })
                        }).unwrap_or_else(|| clean.to_string())
                    };

                    // Find the target function's location
                    if let Some(target) = analysis.functions.iter().find(|f| f.name == target_name) {
                        return Some((program.clone(), target.line as u32 + 1));
                    }
                    // Check imports (#95)
                    if let Some(imp) = self.imports.as_ref() {
                        if let Some(target) = imp.functions.iter().find(|f| f.name == target_name) {
                            // Find which file this function is in
                            for (_, _, resolved_path) in &imp.resolved_files {
                                if let Ok(content) = std::fs::read_to_string(resolved_path) {
                                    let file_analysis = analyze(&content);
                                    if file_analysis.functions.iter().any(|f| f.name == target_name) {
                                        return Some((resolved_path.clone(), target.line as u32 + 1));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// (#93) Build args inspector variables for a function.
    /// Returns structured variable entries showing the args array definition
    /// with field types, required/optional status, and default values.
    fn args_inspector_variables(&self, func_name: &str) -> Vec<Value> {
        let analysis = self.analysis.as_ref();
        let mut vars = Vec::new();

        let find_func = |name: &str| -> Option<&FunctionInfo> {
            analysis?.functions.iter().find(|f| f.name == name)
                .or_else(|| self.imports.as_ref()?.functions.iter().find(|f| f.name == name))
        };

        if let Some(func) = find_func(func_name) {
            for entry in &func.args_entries {
                let field_str = &entry.spec;
                let desc = &entry.description;
                let is_flag = field_str.contains('|');

                let type_str = match &entry.parsed {
                    Ok(f) => {
                        let mut t = String::new();
                        if is_flag { t.push_str("flag"); } else { t.push_str("positional"); }
                        if f.is_boolean { t.push_str(" :+"); }
                        if f.required { t.push_str(" :!"); }
                        if !f.type_name.is_empty() { t.push_str(&format!(" :~{}", f.type_name)); }
                        t
                    }
                    Err(_) => "unknown".to_string(),
                };

                vars.push(serde_json::json!({
                    "name": field_str,
                    "value": desc,
                    "type": type_str,
                    "variablesReference": 0,
                    "presentationHint": {
                        "kind": "property",
                        "attributes": ["readOnly"],
                    }
                }));
            }
        }

        vars
    }

    /// (#97) Get the argsh type annotation for a variable name.
    /// Returns the field definition if the variable is an args field.
    fn variable_type_annotation(&self, var_name: &str, func_name: &str) -> Option<String> {
        let analysis = self.analysis.as_ref()?;

        let find_func = |name: &str| -> Option<&FunctionInfo> {
            analysis.functions.iter().find(|f| f.name == name)
                .or_else(|| self.imports.as_ref()?.functions.iter().find(|f| f.name == name))
        };

        let func = find_func(func_name)?;
        for entry in &func.args_entries {
            let field_name = entry.spec.split('|').next().unwrap_or(&entry.spec);
            // Strip modifiers to get the variable name
            let clean_name = field_name.split(':').next().unwrap_or(field_name);
            if clean_name == var_name {
                return Some(entry.spec.clone());
            }
        }
        None
    }

    /// (#94) Generate launch configurations for all subcommand paths.
    fn generate_launch_configs(&self) -> Vec<Value> {
        let analysis = match self.analysis.as_ref() {
            Some(a) => a,
            None => return vec![],
        };
        let program = match self.program_path.as_ref() {
            Some(p) => p.to_string_lossy().to_string(),
            None => return vec![],
        };

        let mut configs = Vec::new();

        // Walk the usage tree to build subcommand paths
        fn collect_paths(
            funcs: &[FunctionInfo],
            func: &FunctionInfo,
            prefix: &[String],
            configs: &mut Vec<Value>,
            program: &str,
        ) {
            for entry in &func.usage_entries {
                let name = entry.name.split('|').next().unwrap_or(&entry.name);
                let clean = name.split('@').next().unwrap_or(name);
                let mut path = prefix.to_vec();
                path.push(clean.to_string());

                configs.push(serde_json::json!({
                    "type": "argsh",
                    "request": "launch",
                    "name": format!("Debug: {}", path.join(" ")),
                    "program": program,
                    "args": path,
                    "stopOnEntry": false,
                }));

                // Find the target function and recurse for nested subcommands
                let target_name = if let Some(ref explicit) = entry.explicit_func {
                    explicit.clone()
                } else {
                    format!("{}::{}", func.name, clean)
                };
                if let Some(target) = funcs.iter().find(|f| f.name == target_name) {
                    collect_paths(funcs, target, &path, configs, program);
                }
            }
        }

        // Start from functions that have :usage (typically main)
        for func in &analysis.functions {
            if func.calls_usage && !func.usage_entries.is_empty() {
                let prefix = vec![];
                collect_paths(&analysis.functions, func, &prefix, &mut configs, &program);
            }
        }

        configs
    }

    fn next_seq(&self) -> i64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    fn send_response(&self, req: &DapMessage, success: bool, body: Option<Value>, message: Option<String>) {
        let resp = DapResponse {
            seq: self.next_seq(),
            msg_type: "response",
            request_seq: req.seq,
            success,
            command: req.command.clone().unwrap_or_default(),
            body,
            message,
        };
        send_dap_message(&self.stdout_writer, &resp);
    }

    fn send_event(&self, event: &str, body: Option<Value>) {
        let evt = DapEvent {
            seq: self.next_seq(),
            msg_type: "event",
            event: event.to_string(),
            body,
        };
        send_dap_message(&self.stdout_writer, &evt);
    }

    fn handle_initialize(&self, req: &DapMessage) {
        let capabilities = serde_json::json!({
            "supportsConfigurationDoneRequest": true,
            "supportsFunctionBreakpoints": true,   // #92: smart breakpoints by command name
            "supportsConditionalBreakpoints": true, // conditional breakpoints
            "supportsEvaluateForHovers": true,      // #97: variable type tooltips
            "supportsStepBack": false,
            "supportsSetVariable": true,            // modify variables at runtime
            "supportsRestartFrame": false,
            "supportsGotoTargetsRequest": false,
            "supportsStepInTargetsRequest": false,
            "supportsCompletionsRequest": false,
            "supportsTerminateRequest": true,
            "exceptionBreakpointFilters": [],
        });
        self.send_response(req, true, Some(capabilities), None);
        self.send_event("initialized", None);
    }

    fn handle_launch(&mut self, req: &DapMessage) {
        let args = match &req.arguments {
            Some(a) => a,
            None => {
                self.send_response(req, false, None, Some("Missing launch arguments".into()));
                return;
            }
        };

        let program = match args.get("program").and_then(|v| v.as_str()) {
            Some(p) => PathBuf::from(p),
            None => {
                self.send_response(req, false, None, Some("Missing 'program' in launch config".into()));
                return;
            }
        };

        let script_args: Vec<String> = args.get("args")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let cwd = args.get("cwd")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| program.parent().unwrap_or(Path::new(".")).to_path_buf());

        let stop_on_entry = args.get("stopOnEntry")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Create FIFOs in a secure temporary directory (unpredictable path).
        let fifo_tmpdir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => {
                self.send_response(req, false, None,
                    Some(format!("Failed to create temp directory for FIFOs: {}", e)));
                return;
            }
        };
        let fifo_dir = fifo_tmpdir.keep();
        let fifo_data = fifo_dir.join("data");
        let fifo_ctl = fifo_dir.join("data.ctl");

        // Create named pipes + lock file for flock serialization
        #[cfg(unix)]
        {
            let data_str = match fifo_data.to_str() {
                Some(s) => s,
                None => {
                    self.send_response(req, false, None,
                        Some("FIFO path contains invalid UTF-8".into()));
                    return;
                }
            };
            let ctl_str = match fifo_ctl.to_str() {
                Some(s) => s,
                None => {
                    self.send_response(req, false, None,
                        Some("FIFO path contains invalid UTF-8".into()));
                    return;
                }
            };
            let data_c = std::ffi::CString::new(data_str).unwrap();
            let ctl_c = std::ffi::CString::new(ctl_str).unwrap();
            // SAFETY: CString pointers are valid and null-terminated.
            let rc_data = unsafe { libc::mkfifo(data_c.as_ptr(), 0o600) };
            let rc_ctl = unsafe { libc::mkfifo(ctl_c.as_ptr(), 0o600) };
            if rc_data != 0 || rc_ctl != 0 {
                let err = std::io::Error::last_os_error();
                self.send_response(req, false, None,
                    Some(format!("Failed to create FIFOs: {}", err)));
                return;
            }
        }
        // Lock file for flock-based FIFO write serialization
        let lock_path = fifo_dir.join("data.lock");
        std::fs::write(&lock_path, "").ok();

        let wrapper_path = fifo_dir.join("wrapper.sh");

        // Inject any breakpoints that were set before launch into the prelude.
        // The prelude's __ARGSH_DAP_BPS array is populated at script start so
        // breakpoints work immediately without a ctl FIFO round-trip.
        // Shell-escape file paths in breakpoint entries to handle spaces/quotes.
        let initial_bps: String = self.breakpoints.iter()
            .flat_map(|(file, lines)| {
                let escaped = file.display().to_string().replace('\'', "'\\''");
                lines.iter().map(move |line| format!("'{}:{}'", escaped, line))
            })
            .collect::<Vec<_>>()
            .join(" ");

        // Build the wrapper script with the debug prelude
        let prelude = DEBUG_PRELUDE
            .replace("__FIFO_PATH__", fifo_data.to_str().unwrap())
            .replace("__WRAPPER_PATH__", wrapper_path.to_str().unwrap())
            .replace("__STOP_ON_ENTRY__", if stop_on_entry { "1" } else { "0" })
            .replace("declare -a __ARGSH_DAP_BPS=()     # breakpoints: \"file:line\" entries",
                     &format!("declare -a __ARGSH_DAP_BPS=({})     # breakpoints: \"file:line\" entries", initial_bps));

        // Don't inject set flags (e.g. set -euo pipefail) — let the user's
        // script set its own runtime semantics. The wrapper only injects the
        // debug prelude and then sources the target script.
        // Detect if the script needs the argsh runtime by checking its shebang.
        let needs_argsh = std::fs::read_to_string(&program)
            .ok()
            .and_then(|s| s.lines().next().map(String::from))
            .map(|s| s.contains("argsh"))
            .unwrap_or(false);

        // Build the wrapper script. If the target uses argsh, source the argsh
        // runtime first so :args, :usage, import, etc. are available. We try
        // `argsh.min.sh` (bundled minified runtime) and fall back to `argsh`
        // on PATH. The debug prelude is injected before the user's script.
        let argsh_loader = if needs_argsh {
            // Source argsh runtime: try argsh.min.sh next to the script,
            // then argsh on PATH, then the system argsh.min.sh.
            format!(
                concat!(
                    "# Load argsh runtime for scripts with #!/usr/bin/env argsh\n",
                    "_argsh_rt=\"$(dirname \"{script}\")/../argsh.min.sh\"\n",
                    "[[ -f \"$_argsh_rt\" ]] || _argsh_rt=\"$(command -v argsh 2>/dev/null)\"\n",
                    "[[ -n \"$_argsh_rt\" ]] || {{ echo \"argsh-dap: argsh runtime not found\" >&2; exit 1; }}\n",
                    "ARGSH_SOURCE=\"{script}\" source \"$_argsh_rt\"\n",
                ),
                script = program.display()
            )
        } else {
            String::new()
        };

        let wrapper = format!(
            "#!/usr/bin/env bash\nset -T\n{argsh}{prelude}\nsource \"{script}\" \"$@\"\n",
            argsh = argsh_loader,
            prelude = prelude,
            script = program.display()
        );

        std::fs::write(&wrapper_path, &wrapper).unwrap();
        let interpreter = "bash".to_string();

        // Spawn with the detected interpreter
        let mut cmd = Command::new(&interpreter);
        cmd.arg(wrapper_path.to_str().unwrap());
        cmd.args(&script_args);
        cmd.current_dir(&cwd);
        // Inherit stdio so the debugged script can read stdin and its
        // stdout/stderr are visible in the debug console. Using Stdio::piped()
        // would block the script if it writes more than the pipe buffer.
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        // Forward env vars from launch config
        if let Some(env) = args.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env {
                if let Some(val) = v.as_str() {
                    cmd.env(k, val);
                }
            }
        }

        // (#92-#97) Analyze the script source for argsh-specific features
        self.analyze_program(&program);

        // Start the FIFO reader BEFORE spawning bash — otherwise bash's
        // DEBUG trap tries to write to the data FIFO before a reader is
        // ready, causing a race condition / hang.
        self.fifo_path = Some(fifo_data.clone());
        self.launched.store(true, Ordering::SeqCst);

        let fifo_data_clone = fifo_data.clone();
        let stdout_writer = self.stdout_writer.clone();
        let seq = Arc::clone(&self.seq);
        let launched = Arc::clone(&self.launched);
        let last_frames = Arc::clone(&self.last_stack_frames);
        let thread_pids = Arc::clone(&self.thread_pids);
        let active_threads = Arc::clone(&self.active_threads);

        std::thread::spawn(move || {
            fifo_reader_loop(
                &fifo_data_clone, &stdout_writer, &seq, &launched,
                &last_frames, &thread_pids, &active_threads,
            );
        });

        // Small delay to let the FIFO reader open the read end
        std::thread::sleep(std::time::Duration::from_millis(50));

        match cmd.spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.send_response(req, true, None, None);
            }
            Err(e) => {
                // Reset state — FIFO reader was already started but there's
                // no bash process to communicate with.
                self.launched.store(false, Ordering::SeqCst);
                self.fifo_path = None;
                self.send_response(req, false, None, Some(format!("Failed to launch: {}", e)));
            }
        }
    }

    fn handle_set_breakpoints(&mut self, req: &DapMessage) {
        let args = req.arguments.as_ref().unwrap();
        let source_path = args.get("source")
            .and_then(|s| s.get("path"))
            .and_then(|p| p.as_str())
            .map(PathBuf::from);

        let bp_array = args.get("breakpoints")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();

        let mut verified = Vec::new();
        let mut lines_set = HashSet::new();
        let mut conditions: Vec<(PathBuf, u32, String)> = Vec::new();

        for bp in &bp_array {
            let line = bp.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
            let condition = bp.get("condition").and_then(|c| c.as_str()).unwrap_or("");

            lines_set.insert(line);
            if let Some(ref path) = source_path {
                if !condition.is_empty() {
                    conditions.push((path.clone(), line, condition.to_string()));
                }
            }

            verified.push(serde_json::json!({
                "verified": true,
                "line": line,
            }));
        }

        if let Some(ref path) = source_path {
            self.breakpoints.insert(path.clone(), lines_set);

            // If launched, update breakpoints + conditions in the running script.
            // Writing to a FIFO blocks until a reader is present, so spawn a
            // thread to avoid blocking the main DAP message loop.
            if self.launched.load(Ordering::SeqCst) {
                if let Some(ref fifo) = self.fifo_path {
                    let ctl_path = format!("{}.ctl", fifo.display());
                    let bp_str: String = self.breakpoints.iter()
                        .flat_map(|(file, lines)| {
                            lines.iter().map(move |line| format!("{}:{}", file.display(), line))
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let conditions_clone = conditions.clone();
                    let ctl_path_clone = ctl_path.clone();
                    std::thread::spawn(move || {
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .write(true)
                            .open(&ctl_path_clone)
                        {
                            let _ = f.write_all(format!("breakpoints:{}\n", bp_str).as_bytes());
                            for (file, line, cond) in &conditions_clone {
                                // Issue #2: use unit separator (\x1f) instead of colon
                                // to avoid breaking on colons in file paths.
                                let _ = f.write_all(
                                    format!("condition\x1f{}\x1f{}\x1f{}\n", file.display(), line, cond)
                                        .as_bytes(),
                                );
                            }
                            let _ = f.flush();
                        }
                    });
                }
            }
        }

        self.send_response(req, true, Some(serde_json::json!({
            "breakpoints": verified,
        })), None);
    }

    fn handle_configuration_done(&self, req: &DapMessage) {
        self.send_response(req, true, None, None);
    }

    fn handle_threads(&self, req: &DapMessage) {
        // Issue #11: return all active threads (main + subshells), not just main.
        let threads_map = self.active_threads.lock().unwrap();
        let threads: Vec<Value> = threads_map.iter()
            .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
            .collect();
        self.send_response(req, true, Some(serde_json::json!({
            "threads": threads,
        })), None);
    }

    /// Issue #4/#10: Resolve a threadId from the request arguments to the
    /// corresponding bash PID. Returns None for the main thread (threadId 1)
    /// since it uses the default .ctl FIFO without a PID suffix.
    fn resolve_thread_pid(&self, req: &DapMessage) -> Option<u64> {
        let thread_id = req.arguments.as_ref()
            .and_then(|a| a.get("threadId"))
            .and_then(|t| t.as_i64())
            .unwrap_or(1);
        if thread_id == 1 {
            return None; // main thread uses default .ctl
        }
        self.thread_pids.lock().unwrap().get(&thread_id).copied()
    }

    fn handle_continue(&self, req: &DapMessage) {
        let pid = self.resolve_thread_pid(req);
        self.write_ctl_for("continue\n", pid);
        self.send_response(req, true, Some(serde_json::json!({
            "allThreadsContinued": pid.is_none(),
        })), None);
    }

    fn handle_next(&self, req: &DapMessage) {
        let pid = self.resolve_thread_pid(req);
        self.write_ctl_for("next\n", pid);
        self.send_response(req, true, None, None);
    }

    fn handle_step_in(&self, req: &DapMessage) {
        let pid = self.resolve_thread_pid(req);
        self.write_ctl_for("stepin\n", pid);
        self.send_response(req, true, None, None);
    }

    fn handle_step_out(&self, req: &DapMessage) {
        let pid = self.resolve_thread_pid(req);
        self.write_ctl_for("stepout\n", pid);
        self.send_response(req, true, None, None);
    }

    fn handle_stack_trace(&self, req: &DapMessage) {
        // Return the stack trace from the last STOPPED event, stored by
        // the FIFO reader thread.
        // Issue #7: Clone the vec to avoid moving out of the MutexGuard.
        let frames = self.last_stack_frames.lock().unwrap().clone();
        let total = frames.len();
        self.send_response(req, true, Some(serde_json::json!({
            "stackFrames": frames,
            "totalFrames": total,
        })), None);
    }

    fn handle_scopes(&self, req: &DapMessage) {
        // Scope 1: Locals, Scope 2: argsh Args Inspector (#93)
        let mut scopes = vec![
            serde_json::json!({
                "name": "Locals",
                "variablesReference": 1,
                "expensive": false,
            }),
        ];
        // Add Args Inspector scope only if the script uses :args
        let has_args = self.analysis.as_ref()
            .map(|a| a.functions.iter().any(|f| f.calls_args))
            .unwrap_or(false);
        if has_args {
            scopes.push(serde_json::json!({
                "name": "argsh Args",
                "variablesReference": 2,
                "expensive": false,
                "presentationHint": "registers",
            }));
        }
        self.send_response(req, true, Some(serde_json::json!({
            "scopes": scopes,
        })), None);
    }

    fn handle_variables(&self, req: &DapMessage) {
        let var_ref = req.arguments.as_ref()
            .and_then(|a| a.get("variablesReference"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let variables = match var_ref {
            1 => {
                // Issue #3: Locals scope shows variables when the script is stopped
                // (requires FIFO round-trip via the `vars` command). This is a known
                // limitation — the scope appears empty until a stop event triggers a
                // `declare -p` dump from the bash process.
                // TODO: implement runtime var fetching via FIFO round-trip
                vec![]
            }
            2 => {
                // (#93) Args Inspector — structured view of :args definitions
                // Uses the current function from the last stop event
                // For now, show all functions' args (will be scoped to current frame later)
                if let Some(ref analysis) = self.analysis {
                    let mut vars = Vec::new();
                    for func in &analysis.functions {
                        if !func.args_entries.is_empty() {
                            vars.extend(self.args_inspector_variables(&func.name));
                        }
                    }
                    vars
                } else {
                    vec![]
                }
            }
            _ => vec![],
        };

        self.send_response(req, true, Some(serde_json::json!({
            "variables": variables,
        })), None);
    }

    /// Handle setVariable — modify a variable at runtime via the FIFO protocol.
    /// Issue #1/#5/#14: Validates the variable name as a valid bash identifier
    /// on the Rust side before sending to the bash process, which uses printf -v
    /// for safe assignment (no eval).
    fn handle_set_variable(&self, req: &DapMessage) {
        let args = req.arguments.as_ref();
        let name = args.and_then(|a| a.get("name")).and_then(|n| n.as_str()).unwrap_or("");
        let value = args.and_then(|a| a.get("value")).and_then(|v| v.as_str()).unwrap_or("");

        if name.is_empty() {
            self.send_response(req, false, None, Some("Missing variable name".into()));
            return;
        }

        // Validate: must be a valid bash identifier (letters, digits, underscores;
        // cannot start with a digit). Reject anything else to prevent injection.
        let is_valid_ident = !name.is_empty()
            && name.bytes().next().is_some_and(|b| b == b'_' || b.is_ascii_alphabetic())
            && name.bytes().all(|b| b == b'_' || b.is_ascii_alphanumeric());

        if !is_valid_ident {
            self.send_response(req, false, None,
                Some(format!("Invalid variable name: '{}'", name)));
            return;
        }

        // Send setvar command to the bash process via FIFO.
        // The bash side uses `printf -v` for safe assignment.
        self.write_ctl(&format!("setvar:{}={}\n", name, value));
        self.send_response(req, true, Some(serde_json::json!({
            "value": value,
        })), None);
    }

    /// (#92) Handle function breakpoints — resolve subcommand names to line breakpoints.
    /// Issue #13: After resolving, push the updated breakpoint list to the
    /// running script via the FIFO (same as setBreakpoints does).
    fn handle_set_function_breakpoints(&mut self, req: &DapMessage) {
        let args = req.arguments.as_ref();
        let breakpoints: Vec<Value> = args
            .and_then(|a| a.get("breakpoints"))
            .and_then(|b| b.as_array())
            .map(|arr| {
                arr.iter().map(|bp| {
                    let name = bp.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    match self.resolve_command_breakpoint(name) {
                        Some((file, line)) => {
                            // Add to our breakpoint set
                            self.breakpoints
                                .entry(file.clone())
                                .or_default()
                                .insert(line);
                            serde_json::json!({
                                "verified": true,
                                "line": line,
                                "source": { "path": file.to_string_lossy() },
                                "message": format!("Resolved '{}' to {}:{}", name, file.display(), line),
                            })
                        }
                        None => {
                            serde_json::json!({
                                "verified": false,
                                "message": format!("Could not resolve command '{}'", name),
                            })
                        }
                    }
                }).collect()
            })
            .unwrap_or_default();

        // Issue #13: Push updated breakpoints to the running script via FIFO
        if self.launched.load(Ordering::SeqCst) {
            if let Some(ref fifo) = self.fifo_path {
                let ctl_path = format!("{}.ctl", fifo.display());
                let bp_str: String = self.breakpoints.iter()
                    .flat_map(|(file, lines)| {
                        lines.iter().map(move |line| format!("{}:{}", file.display(), line))
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                std::thread::spawn(move || {
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .write(true)
                        .open(&ctl_path)
                    {
                        let _ = f.write_all(format!("breakpoints:{}\n", bp_str).as_bytes());
                        let _ = f.flush();
                    }
                });
            }
        }

        self.send_response(req, true, Some(serde_json::json!({
            "breakpoints": breakpoints,
        })), None);
    }

    /// (#97) Handle evaluate — return argsh type annotations on hover.
    fn handle_evaluate(&self, req: &DapMessage) {
        let args = req.arguments.as_ref();
        let expression = args
            .and_then(|a| a.get("expression"))
            .and_then(|e| e.as_str())
            .unwrap_or("");
        let context = args
            .and_then(|a| a.get("context"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        if context == "hover" {
            // (#97) Try to find argsh type annotation for the hovered variable
            // For now, search all functions — will be scoped to current frame later
            if let Some(ref analysis) = self.analysis {
                for func in &analysis.functions {
                    if let Some(annotation) = self.variable_type_annotation(expression, &func.name) {
                        self.send_response(req, true, Some(serde_json::json!({
                            "result": format!("argsh: {}", annotation),
                            "variablesReference": 0,
                        })), None);
                        return;
                    }
                }
            }
        }

        // Issue #15: Watch expressions via evaluate with context="watch".
        // Sends the expression to the bash process via FIFO and returns the
        // result. This allows the Watch panel to show live variable values.
        if context == "watch" && !expression.is_empty() && self.launched.load(Ordering::SeqCst) {
            // Send eval command and wait for the result via the data FIFO.
            // Note: this is a best-effort implementation. The eval result is
            // read by the FIFO reader thread and stored; here we send the
            // command and return a placeholder. A full implementation would
            // use a condvar to wait for the FIFO reader to deliver the result.
            // TODO: implement condvar-based synchronous eval for watch expressions.
            self.write_ctl(&format!("eval:{}\n", expression));
            self.send_response(req, true, Some(serde_json::json!({
                "result": format!("(evaluating: {})", expression),
                "variablesReference": 0,
            })), None);
            return;
        }

        // (#94) Special command to generate launch configs
        if expression == "argsh.generateLaunchConfigs" {
            let configs = self.generate_launch_configs();
            let json = serde_json::to_string_pretty(&configs).unwrap_or_default();
            self.send_response(req, true, Some(serde_json::json!({
                "result": json,
                "variablesReference": 0,
            })), None);
            return;
        }

        // Default: expression not evaluable
        self.send_response(req, true, Some(serde_json::json!({
            "result": "",
            "variablesReference": 0,
        })), None);
    }

    fn handle_disconnect(&mut self, req: &DapMessage) {
        // Kill the bash process
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;

        // Clean up FIFOs
        if let Some(ref fifo) = self.fifo_path {
            let fifo_dir = fifo.parent().unwrap();
            let _ = std::fs::remove_dir_all(fifo_dir);
        }
        self.fifo_path = None;
        self.launched.store(false, Ordering::SeqCst);

        self.send_response(req, true, None, None);
        self.send_event("terminated", None);
    }

    fn handle_terminate(&mut self, req: &DapMessage) {
        self.handle_disconnect(req);
    }

    /// Write a command to the control FIFO. If pid is Some, writes to the
    /// per-PID control FIFO (for subshells); otherwise the main .ctl FIFO.
    ///
    /// Note: the open() call here is blocking (standard FIFO semantics). This is
    /// correct for the stop/resume protocol: when we write a resume command, the
    /// bash process is always blocked on `read` from the same FIFO, so the open()
    /// succeeds immediately. Non-blocking O_NONBLOCK is not needed.
    fn write_ctl_for(&self, cmd: &str, pid: Option<u64>) {
        if let Some(ref fifo) = self.fifo_path {
            let ctl_path = match pid {
                Some(p) => format!("{}.ctl.{}", fifo.display(), p),
                None => format!("{}.ctl", fifo.display()),
            };
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&ctl_path) {
                let _ = f.write_all(cmd.as_bytes());
                let _ = f.flush();
            }
        }
    }

    fn write_ctl(&self, cmd: &str) {
        self.write_ctl_for(cmd, None);
    }
}

impl Drop for DapSession {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
        }
        if let Some(ref fifo) = self.fifo_path {
            let fifo_dir = fifo.parent().unwrap();
            let _ = std::fs::remove_dir_all(fifo_dir);
        }
    }
}

// ---------------------------------------------------------------------------
// FIFO reader — background thread that reads stop events from bash
// ---------------------------------------------------------------------------

fn fifo_reader_loop(
    fifo_path: &Path,
    stdout_writer: &Arc<Mutex<io::Stdout>>,
    seq: &AtomicI64,
    launched: &AtomicBool,
    last_frames: &Mutex<Vec<Value>>,
    thread_pids: &Mutex<HashMap<i64, u64>>,
    active_threads: &Mutex<HashMap<i64, String>>,
) {
    loop {
        // Check if the session is still alive before (re-)opening the FIFO.
        // File::open on a FIFO blocks until a writer opens, so without this
        // check the thread would hang after the session ends and FIFOs are
        // removed.
        if !launched.load(Ordering::SeqCst) {
            break;
        }

        // Open FIFO for reading (blocks until writer opens)
        let file = match std::fs::File::open(fifo_path) {
            Ok(f) => f,
            Err(_) => break, // FIFO removed, session ended
        };
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            if line.starts_with("STOPPED\t") {
                // Format: STOPPED\tfile\tline\tfunc\tpid\tsubshell_level
                // Followed by stack trace lines: file\tline\tfunc
                let parts: Vec<&str> = line.splitn(6, '\t').collect();
                if parts.len() >= 4 {
                    let file = parts[1];
                    let line_num: i64 = parts[2].parse().unwrap_or(0);
                    let func = parts[3];
                    let pid: u64 = parts.get(4).and_then(|p| p.parse().ok()).unwrap_or(0);
                    let subshell: i64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

                    // Map subshell level to thread ID: main=1, subshell 1=2, etc.
                    let thread_id = 1 + subshell;

                    // Issue #4/#10: Store the threadId → PID mapping so
                    // continue/step commands can route to the correct FIFO.
                    if subshell > 0 && pid > 0 {
                        if let Ok(mut pids) = thread_pids.lock() {
                            pids.insert(thread_id, pid);
                        }
                    }

                    // Issue #11: Add this thread to the active set.
                    if let Ok(mut threads) = active_threads.lock() {
                        let name = if subshell == 0 {
                            "main".to_string()
                        } else {
                            format!("subshell {} (pid {})", subshell, pid)
                        };
                        threads.insert(thread_id, name);
                    }

                    let reason = if subshell > 0 { "breakpoint (subshell)" } else { "breakpoint" };

                    // Build stack frames from the stopped position
                    let frames = vec![serde_json::json!({
                        "id": 0,
                        "name": func,
                        "source": { "path": file },
                        "line": line_num,
                        "column": 1,
                    })];
                    // Additional stack frames from the bash prelude follow the
                    // STOPPED line as part of the same write. They will appear
                    // as subsequent lines in this iterator if present.
                    // For now, we report the top frame; deeper frames can be
                    // parsed from the follow-up lines in a future iteration.

                    // Store the stack trace for handle_stack_trace
                    if let Ok(mut f) = last_frames.lock() {
                        *f = frames.clone();
                    }

                    let evt = DapEvent {
                        seq: seq.fetch_add(1, Ordering::SeqCst),
                        msg_type: "event",
                        event: "stopped".to_string(),
                        body: Some(serde_json::json!({
                            "reason": reason,
                            "threadId": thread_id,
                            "allThreadsStopped": subshell == 0,
                            "description": format!("Stopped at {}:{} in {}", file, line_num, func),
                        })),
                    };
                    send_dap_message(stdout_writer, &evt);
                }
            }
            // TODO: Handle "EXITED\tpid" events to remove subshell threads
            // from active_threads and thread_pids when the subshell exits.
        }
    }
}

// ---------------------------------------------------------------------------
// DAP message I/O (same framing as LSP: Content-Length header + JSON body)
// ---------------------------------------------------------------------------

fn read_dap_message(reader: &mut impl BufRead) -> Option<DapMessage> {
    let mut content_length: usize = 0;

    // Read headers
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            return None; // EOF
        }
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str.parse().ok()?;
        }
    }

    if content_length == 0 {
        return None;
    }

    // Read body
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).ok()?;

    serde_json::from_slice(&body).ok()
}

fn send_dap_message<T: Serialize>(writer: &Arc<Mutex<io::Stdout>>, msg: &T) {
    let body = serde_json::to_string(msg).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", body.len());

    let mut out = writer.lock().unwrap();
    let _ = out.write_all(header.as_bytes());
    let _ = out.write_all(body.as_bytes());
    let _ = out.flush();
}

// ---------------------------------------------------------------------------
// Trace mode — headless execution with markdown output
// ---------------------------------------------------------------------------

/// Run a script with the trace prelude and write a structured markdown trace.
///
/// This mode reuses the FIFO infrastructure from the DAP debugger but runs
/// headlessly: no DAP protocol, no stdin reading. The script runs to
/// completion while trace events are collected, then the markdown file is
/// written with enriched analysis from `argsh_syntax`.
#[cfg(unix)]
fn run_trace_mode(output: &Path, script: &Path, args: &[String]) -> Result<(), String> {
    let start_time = Instant::now();

    // Validate that the script exists
    if !script.exists() {
        return Err(format!("Script not found: {}", script.display()));
    }

    // Canonicalize the script path for consistent display
    let script = script.canonicalize()
        .map_err(|e| format!("Failed to canonicalize script path: {}", e))?;

    // Create FIFOs in a secure temporary directory
    let fifo_tmpdir = tempfile::tempdir()
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;
    let fifo_dir = fifo_tmpdir.keep();
    let fifo_data = fifo_dir.join("data");

    // Create the data FIFO
    {
        let data_str = fifo_data.to_str()
            .ok_or("FIFO path contains invalid UTF-8")?;
        let data_c = std::ffi::CString::new(data_str).unwrap();
        // SAFETY: CString pointer is valid and null-terminated.
        let rc = unsafe { libc::mkfifo(data_c.as_ptr(), 0o600) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("Failed to create FIFO: {}", err));
        }
    }

    // Lock file for flock-based FIFO write serialization
    let lock_path = fifo_dir.join("data.lock");
    std::fs::write(&lock_path, "").ok();

    let wrapper_path = fifo_dir.join("wrapper.sh");

    // Build the trace prelude with paths substituted
    let prelude = TRACE_PRELUDE
        .replace("__FIFO_PATH__", fifo_data.to_str().unwrap())
        .replace("__WRAPPER_PATH__", wrapper_path.to_str().unwrap());

    // Detect if the script needs the argsh runtime
    let needs_argsh = std::fs::read_to_string(&script)
        .ok()
        .and_then(|s| s.lines().next().map(String::from))
        .map(|s| s.contains("argsh"))
        .unwrap_or(false);

    let argsh_loader = if needs_argsh {
        format!(
            concat!(
                "# Load argsh runtime\n",
                "_argsh_rt=\"$(dirname \"{script}\")/../argsh.min.sh\"\n",
                "[[ -f \"$_argsh_rt\" ]] || _argsh_rt=\"$(command -v argsh 2>/dev/null)\"\n",
                "[[ -n \"$_argsh_rt\" ]] || {{ echo \"argsh-dap: argsh runtime not found\" >&2; exit 1; }}\n",
                "ARGSH_SOURCE=\"{script}\" source \"$_argsh_rt\"\n",
            ),
            script = script.display()
        )
    } else {
        String::new()
    };

    let wrapper = format!(
        "#!/usr/bin/env bash\nset -T\n{argsh}{prelude}\nsource \"{script}\" \"$@\"\n",
        argsh = argsh_loader,
        prelude = prelude,
        script = script.display()
    );

    std::fs::write(&wrapper_path, &wrapper)
        .map_err(|e| format!("Failed to write wrapper script: {}", e))?;

    // Collect events from the FIFO in a background thread
    let events: Arc<Mutex<Vec<TraceEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let _exit_code_fifo: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
    let fifo_done = Arc::new(AtomicBool::new(false));

    let events_clone = Arc::clone(&events);
    let exit_code_clone = Arc::clone(&_exit_code_fifo);
    let fifo_done_clone = Arc::clone(&fifo_done);
    let fifo_data_clone = fifo_data.clone();
    let trace_start = Instant::now();

    let reader_thread = std::thread::spawn(move || {
        // The reader loop reopens the FIFO after each EOF (bash may close
        // and reopen the write end in subshells).
        loop {
            if fifo_done_clone.load(Ordering::SeqCst) {
                break;
            }

            let file = match std::fs::File::open(&fifo_data_clone) {
                Ok(f) => f,
                Err(_) => break,
            };
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };

                let elapsed = trace_start.elapsed().as_millis() as u64;

                if line.starts_with("TRACE\t") {
                    // Format: TRACE\ttype\tfile\tline\tfunc\tdepth\tcmd\tspecial
                    let parts: Vec<&str> = line.splitn(8, '\t').collect();
                    if parts.len() >= 7 {
                        let event = TraceEvent {
                            event_type: parts[1].to_string(),
                            file: parts[2].to_string(),
                            line: parts[3].parse().unwrap_or(0),
                            func: parts[4].to_string(),
                            depth: parts[5].parse().unwrap_or(0),
                            command: parts[6].to_string(),
                            special: parts.get(7).unwrap_or(&"").to_string(),
                            elapsed_ms: elapsed,
                        };
                        events_clone.lock().unwrap().push(event);
                    }
                } else if line.starts_with("TRACE_EXIT\t") {
                    let code: i32 = line.strip_prefix("TRACE_EXIT\t")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(-1);
                    *exit_code_clone.lock().unwrap() = Some(code);
                }
            }
        }
    });

    // Spawn the bash process
    let mut cmd = Command::new("bash");
    cmd.arg(wrapper_path.to_str().unwrap());
    cmd.args(args);
    if let Some(parent) = script.parent() {
        cmd.current_dir(parent);
    }
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn bash: {}", e))?;

    // Wait for the script to finish
    let status = child.wait()
        .map_err(|e| format!("Failed to wait for bash: {}", e))?;

    let process_exit_code = status.code().unwrap_or(-1);

    // Give the FIFO reader a moment to drain remaining events, then signal it
    std::thread::sleep(std::time::Duration::from_millis(100));
    fifo_done.store(true, Ordering::SeqCst);

    // Unblock the reader thread if it's stuck on open() by briefly opening
    // the FIFO for writing. This makes the reader's open() return, then it
    // sees fifo_done==true and exits.
    let _ = std::fs::OpenOptions::new().write(true).open(&fifo_data);

    let _ = reader_thread.join();

    let total_duration = start_time.elapsed();
    let events = events.lock().unwrap();
    // Prefer the actual process exit code (ground truth) over the FIFO-reported
    // one, which may not arrive in time for fast-exiting scripts.
    let exit_code = process_exit_code;

    // Analyze the script with argsh_syntax for enrichment
    let content = std::fs::read_to_string(&script).unwrap_or_default();
    let analysis = analyze(&content);
    let imports = resolver::resolve_imports(&analysis, &script, resolver::DEFAULT_MAX_DEPTH);

    // Build the markdown output
    let markdown = render_trace_markdown(
        &script,
        args,
        &events,
        exit_code,
        &total_duration,
        &analysis,
        &imports,
    );

    // Write the output file
    std::fs::write(output, &markdown)
        .map_err(|e| format!("Failed to write trace output: {}", e))?;

    // Clean up temp directory
    let _ = std::fs::remove_dir_all(&fifo_dir);

    eprintln!("argsh-dap: trace written to {}", output.display());
    Ok(())
}

/// Render the collected trace events into a structured markdown document.
///
/// Enriches the raw execution trace with static analysis from `argsh_syntax`:
/// - Field type annotations from `:args` definitions
/// - Command tree from `:usage` dispatch
/// - Import tree from the resolver
fn render_trace_markdown(
    script: &Path,
    args: &[String],
    events: &[TraceEvent],
    exit_code: i32,
    duration: &std::time::Duration,
    analysis: &DocumentAnalysis,
    imports: &resolver::ResolvedImports,
) -> String {
    let mut md = String::new();

    let script_name = script.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| script.display().to_string());

    let args_str = if args.is_empty() {
        String::new()
    } else {
        format!(" {}", args.join(" "))
    };

    // Header
    md.push_str(&format!("# Process Trace: {}{}\n\n", script_name, args_str));

    let now = chrono_like_timestamp();
    let duration_secs = duration.as_secs_f64();
    let duration_str = if duration_secs < 1.0 {
        format!("{:.0}ms", duration.as_millis())
    } else {
        format!("{:.1}s", duration_secs)
    };

    md.push_str(&format!(
        "> Generated: {}  \n> Script: `{}`  \n> Exit code: {} | Duration: {} | Steps: {}\n\n",
        now,
        script.display(),
        exit_code,
        duration_str,
        events.len(),
    ));

    md.push_str("---\n\n");

    // Command tree from static analysis
    let has_usage = analysis.functions.iter().any(|f| f.calls_usage);
    if has_usage {
        md.push_str("## Command Tree\n\n");
        md.push_str("```\n");
        md.push_str(&script_name);
        md.push('\n');
        render_command_tree(&mut md, analysis, &analysis.functions, "", 0);
        md.push_str("```\n\n---\n\n");
    }

    // Execution trace
    md.push_str("## Execution\n\n");

    // Build traced functions from events
    let traced_fns = build_traced_functions(events);

    for tf in &traced_fns {
        let file_short = Path::new(&tf.file)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| tf.file.clone());

        // Function entry
        let indent = "  ".repeat(tf.depth as usize);
        let timing = format_timing(tf.entry_ms);

        // Find matching FunctionInfo for enrichment
        let func_info = analysis.functions.iter().find(|f| f.name == tf.name)
            .or_else(|| imports.functions.iter().find(|f| f.name == tf.name));

        if let Some(exit_ms) = tf.exit_ms {
            let call_duration = exit_ms - tf.entry_ms;
            md.push_str(&format!(
                "{}### `{}` ({}:{}) [{}]\n\n",
                indent, tf.name, file_short, tf.entry_line,
                format_timing(call_duration)
            ));
        } else {
            md.push_str(&format!(
                "{}### `{}` ({}:{}) [{}+]\n\n",
                indent, tf.name, file_short, tf.entry_line, timing
            ));
        }

        // Steps table
        if !tf.steps.is_empty() {
            md.push_str(&format!("{}| # | Line | Command | Duration |\n", indent));
            md.push_str(&format!("{}|---|------|---------|----------|\n", indent));

            let mut step_num = 0;
            for (i, step) in tf.steps.iter().enumerate() {
                step_num += 1;
                let step_duration = if i + 1 < tf.steps.len() {
                    let next_ms = tf.steps[i + 1].elapsed_ms;
                    format_timing(next_ms - step.elapsed_ms)
                } else {
                    "<1ms".to_string()
                };

                // Truncate long commands
                let cmd_display = if step.command.len() > 60 {
                    format!("{}...", &step.command[..57])
                } else {
                    step.command.clone()
                };

                md.push_str(&format!(
                    "{}| {} | {} | `{}` | {} |\n",
                    indent, step_num, step.line, cmd_display, step_duration
                ));
            }
            md.push('\n');
        }

        // Args details if the function has :args
        if tf.has_args_call {
            if let Some(fi) = func_info {
                if !fi.args_entries.is_empty() {
                    md.push_str(&format!(
                        "{}<details>\n{}<summary>:args definition</summary>\n\n",
                        indent, indent
                    ));
                    md.push_str(&format!(
                        "{}| Field | Description | Type |\n",
                        indent
                    ));
                    md.push_str(&format!(
                        "{}|-------|-------------|------|\n",
                        indent
                    ));

                    for entry in &fi.args_entries {
                        let type_str = match &entry.parsed {
                            Ok(f) => {
                                let mut t = if f.is_positional {
                                    "positional".to_string()
                                } else {
                                    "flag".to_string()
                                };
                                if f.is_boolean { t.push_str(" :+"); }
                                if f.required { t.push_str(" :!"); }
                                if !f.type_name.is_empty() {
                                    t.push_str(&format!(" :~{}", f.type_name));
                                }
                                t
                            }
                            Err(_) => "unknown".to_string(),
                        };

                        md.push_str(&format!(
                            "{}| `{}` | {} | {} |\n",
                            indent, entry.spec, entry.description, type_str
                        ));
                    }

                    md.push_str(&format!("\n{}</details>\n\n", indent));
                }
            }
        }
    }

    md.push_str("---\n\n");

    // Import tree
    if !imports.resolved_files.is_empty() {
        md.push_str("## Import Tree\n\n");
        md.push_str("| Module | Path |\n");
        md.push_str("|--------|------|\n");

        for (_, module, resolved_path) in &imports.resolved_files {
            md.push_str(&format!(
                "| `{}` | `{}` |\n",
                module,
                resolved_path.display()
            ));
        }

        md.push_str("\n---\n\n");
    }

    // Summary
    md.push_str("## Summary\n\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("|--------|-------|\n");

    let total_steps = events.iter().filter(|e| e.event_type == "step").count();
    let fn_calls = events.iter().filter(|e| e.event_type == "enter").count();
    let args_calls = events.iter().filter(|e| e.special == "args").count();
    let usage_calls = events.iter().filter(|e| e.special == "usage").count();

    md.push_str(&format!("| Total steps | {} |\n", total_steps));
    md.push_str(&format!("| Functions called | {} |\n", fn_calls));
    md.push_str(&format!("| `:args` parsed | {} |\n", args_calls));
    md.push_str(&format!("| `:usage` dispatched | {} |\n", usage_calls));
    md.push_str(&format!("| Imports loaded | {} |\n", imports.resolved_files.len()));
    md.push_str(&format!("| Exit code | {} |\n", exit_code));
    md.push_str(&format!("| Wall time | {} |\n", duration_str));

    md
}

/// Build a list of traced function calls from raw events.
fn build_traced_functions(events: &[TraceEvent]) -> Vec<TracedFunction> {
    let mut functions: Vec<TracedFunction> = Vec::new();
    // Stack of indices into `functions` for open (un-exited) function calls
    let mut stack: Vec<usize> = Vec::new();

    for event in events {
        match event.event_type.as_str() {
            "enter" => {
                let idx = functions.len();
                functions.push(TracedFunction {
                    name: event.func.clone(),
                    file: event.file.clone(),
                    entry_line: event.line,
                    depth: event.depth,
                    entry_ms: event.elapsed_ms,
                    exit_ms: None,
                    steps: Vec::new(),
                    has_args_call: false,
                    has_usage_call: false,
                });
                stack.push(idx);
            }
            "exit" => {
                if let Some(idx) = stack.pop() {
                    functions[idx].exit_ms = Some(event.elapsed_ms);
                }
            }
            "step" => {
                // Add step to the most recently entered function
                if let Some(&idx) = stack.last() {
                    if event.special == "args" {
                        functions[idx].has_args_call = true;
                    }
                    if event.special == "usage" {
                        functions[idx].has_usage_call = true;
                    }
                    functions[idx].steps.push(event.clone());
                }
            }
            _ => {}
        }
    }

    functions
}

/// Render the command tree from :usage analysis (recursive).
fn render_command_tree(
    md: &mut String,
    analysis: &DocumentAnalysis,
    funcs: &[FunctionInfo],
    prefix: &str,
    depth: usize,
) {
    // Find functions at this level that have :usage entries
    for func in funcs {
        if depth == 0 && !func.calls_usage {
            continue;
        }
        if depth > 0 {
            // Only render if we were explicitly asked to render this func
            continue;
        }

        for (i, entry) in func.usage_entries.iter().enumerate() {
            let is_last = i == func.usage_entries.len() - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let name = entry.name.split('|').next().unwrap_or(&entry.name);
            let clean = name.split('@').next().unwrap_or(name);

            md.push_str(&format!("{}{}{}\n", prefix, connector, clean));

            // Find the target function and recurse
            let target_name = if let Some(ref explicit) = entry.explicit_func {
                explicit.clone()
            } else {
                format!("{}::{}", func.name, clean)
            };
            if let Some(target) = analysis.functions.iter().find(|f| f.name == target_name) {
                if target.calls_usage {
                    let child_prefix = if is_last {
                        format!("{}    ", prefix)
                    } else {
                        format!("{}│   ", prefix)
                    };
                    for (j, sub_entry) in target.usage_entries.iter().enumerate() {
                        let sub_last = j == target.usage_entries.len() - 1;
                        let sub_conn = if sub_last { "└── " } else { "├── " };
                        let sub_name = sub_entry.name.split('|').next().unwrap_or(&sub_entry.name);
                        let sub_clean = sub_name.split('@').next().unwrap_or(sub_name);
                        md.push_str(&format!("{}{}{}\n", child_prefix, sub_conn, sub_clean));
                    }
                }
            }
        }
    }
}

/// Format milliseconds into a human-readable duration string.
fn format_timing(ms: u64) -> String {
    if ms == 0 {
        "<1ms".to_string()
    } else if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// Generate a timestamp string without pulling in the chrono crate.
fn chrono_like_timestamp() -> String {
    let output = std::process::Command::new("date")
        .arg("-Iseconds")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    output.unwrap_or_else(|| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[cfg(not(unix))]
fn main() {
    eprintln!("argsh-dap: DAP debugging requires Unix (named pipes)");
    std::process::exit(1);
}

#[cfg(unix)]
fn main() {
    // Handle --version, --help, and --trace
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-V" => {
                println!("argsh-dap {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" => {
                println!("argsh-dap — Debug Adapter Protocol server for argsh scripts");
                println!();
                println!("Usage:");
                println!("  argsh-dap                              Start DAP server (stdin/stdout)");
                println!("  argsh-dap --trace <out.md> -- <script> [args...]");
                println!("                                         Run script and write execution trace");
                println!("  argsh-dap --version                    Show version");
                return;
            }
            "--trace" => {
                // Parse: --trace <output.md> -- <script> [args...]
                if args.len() < 3 {
                    eprintln!("argsh-dap: --trace requires an output path");
                    eprintln!("Usage: argsh-dap --trace <out.md> -- <script> [args...]");
                    std::process::exit(2);
                }
                let output = PathBuf::from(&args[2]);

                // Find the "--" separator
                let sep_pos = args.iter().position(|a| a == "--");
                let sep_pos = match sep_pos {
                    Some(p) if p >= 3 => p,
                    _ => {
                        eprintln!("argsh-dap: --trace requires -- separator before script");
                        eprintln!("Usage: argsh-dap --trace <out.md> -- <script> [args...]");
                        std::process::exit(2);
                    }
                };

                if sep_pos + 1 >= args.len() {
                    eprintln!("argsh-dap: no script specified after --");
                    std::process::exit(2);
                }

                let script = PathBuf::from(&args[sep_pos + 1]);
                let script_args: Vec<String> = args[sep_pos + 2..].to_vec();

                match run_trace_mode(&output, &script, &script_args) {
                    Ok(()) => return,
                    Err(e) => {
                        eprintln!("argsh-dap: trace failed: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            _ => {
                eprintln!("argsh-dap: unknown flag: {}", args[1]);
                std::process::exit(2);
            }
        }
    }

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut session = DapSession::new();

    while let Some(msg) = read_dap_message(&mut reader) {

        if msg.msg_type != "request" {
            continue; // DAP server only handles requests
        }

        match msg.command.as_deref() {
            Some("initialize") => session.handle_initialize(&msg),
            Some("launch") => session.handle_launch(&msg),
            Some("setBreakpoints") => session.handle_set_breakpoints(&msg),
            Some("setFunctionBreakpoints") => session.handle_set_function_breakpoints(&msg),
            Some("configurationDone") => session.handle_configuration_done(&msg),
            Some("threads") => session.handle_threads(&msg),
            Some("continue") => session.handle_continue(&msg),
            Some("next") => session.handle_next(&msg),
            Some("stepIn") => session.handle_step_in(&msg),
            Some("stepOut") => session.handle_step_out(&msg),
            Some("stackTrace") => session.handle_stack_trace(&msg),
            Some("scopes") => session.handle_scopes(&msg),
            Some("variables") => session.handle_variables(&msg),
            Some("setVariable") => session.handle_set_variable(&msg),
            Some("evaluate") => session.handle_evaluate(&msg),
            Some("disconnect") => {
                session.handle_disconnect(&msg);
                break;
            }
            Some("terminate") => {
                session.handle_terminate(&msg);
                break;
            }
            Some(cmd) => {
                // Unknown command — respond with success (DAP spec says unknown
                // commands should not cause errors)
                session.send_response(&msg, true, None, None);
                eprintln!("argsh-dap: unhandled command: {}", cmd);
            }
            None => {}
        }
    }
}
