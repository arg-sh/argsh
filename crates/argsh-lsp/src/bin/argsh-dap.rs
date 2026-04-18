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

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

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
__ARGSH_DAP_STEP=0        # 0=run, 1=stepin, 2=next, 3=stepout
__ARGSH_DAP_DEPTH=0        # saved depth for next/stepout
__ARGSH_DAP_STOP_ENTRY=__STOP_ON_ENTRY__
__ARGSH_DAP_SUBSHELL_PARENT=0  # track parent FIFO lock state
declare -a __ARGSH_DAP_BPS=()     # breakpoints: "file:line" entries
declare -A __ARGSH_DAP_BP_COND=() # conditional breakpoints: "file:line" → condition
declare -a __ARGSH_DAP_WATCH=()   # watch expressions

__argsh_dap_trap() {
  local _line="${BASH_LINENO[0]}"
  local _func="${FUNCNAME[1]:-main}"
  local _file="${BASH_SOURCE[1]:-${0}}"
  local _depth=${#FUNCNAME[@]}
  local _should_stop=0

  # Subshell handling: commands inside $(), pipes, and & run in subshells
  # where the DEBUG trap is inherited. We can't use the FIFO (the parent
  # may be blocked on it → deadlock), but we CAN still track execution.
  # We send a lightweight OUTPUT event for subshell breakpoint hits
  # instead of blocking with a STOPPED event.
  if (( BASH_SUBSHELL > 0 )); then
    # Check breakpoints in subshell (non-blocking)
    local _bp
    for _bp in "${__ARGSH_DAP_BPS[@]}"; do
      if [[ "${_bp}" == "${_file}:${_line}" ]]; then
        # Non-blocking output event (write will succeed or fail silently)
        printf 'SUBSHELL\t%s\t%s\t%s\t%s\n' \
          "${_file}" "${_line}" "${_func}" "${BASH_COMMAND}" \
          > "${__ARGSH_DAP_FIFO}" 2>/dev/null || true
        break
      fi
    done
    return 0
  fi

  # Stop on entry (first trap hit)
  if (( __ARGSH_DAP_STOP_ENTRY )); then
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

    # Write stop event to FIFO (DAP server reads this)
    printf 'STOPPED\t%s\t%s\t%s\n%b%b' \
      "${_file}" "${_line}" "${_func}" "${_stack}" "${_watches}" \
      > "${__ARGSH_DAP_FIFO}"

    # Block until DAP server sends a resume command
    local _cmd
    while IFS= read -r _cmd; do
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
          local _bpdata="${_cmd#breakpoints:}"
          __ARGSH_DAP_BPS=()
          if [[ -n "${_bpdata}" ]]; then
            IFS=',' read -ra __ARGSH_DAP_BPS <<< "${_bpdata}"
          fi
          ;;
        condition:*)
          # Set conditional breakpoint: "condition:file:line:expression"
          local _cdata="${_cmd#condition:}"
          local _cfile _cline _cexpr
          IFS=':' read -r _cfile _cline _cexpr <<< "${_cdata}"
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
          local _svdata="${_cmd#setvar:}"
          eval "${_svdata}" 2>/dev/null || true
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
          {
            declare -p 2>/dev/null
            printf 'ENDVARS\n'
          } > "${__ARGSH_DAP_FIFO}"
          ;;
      esac
    done < "${__ARGSH_DAP_FIFO}.ctl"
  fi

  return 0
}

trap '__argsh_dap_trap' DEBUG
# ── end debug prelude ────────────────────────────────────────────────────
"#;

// ---------------------------------------------------------------------------
// DAP Session
// ---------------------------------------------------------------------------

struct DapSession {
    seq: AtomicI64,
    breakpoints: HashMap<PathBuf, HashSet<u32>>,
    child: Option<Child>,
    fifo_path: Option<PathBuf>,
    launched: AtomicBool,
    stdout_writer: Arc<Mutex<io::Stdout>>,
    // argsh analysis (#92-#97): cached document analysis for the launched script
    // and its imports, enabling smart breakpoints, args inspection, and type tooltips.
    analysis: Option<DocumentAnalysis>,
    imports: Option<resolver::ResolvedImports>,
    program_path: Option<PathBuf>,
    program_content: Option<String>,
}

impl DapSession {
    fn new() -> Self {
        Self {
            seq: AtomicI64::new(1),
            breakpoints: HashMap::new(),
            child: None,
            fifo_path: None,
            launched: AtomicBool::new(false),
            stdout_writer: Arc::new(Mutex::new(io::stdout())),
            analysis: None,
            imports: None,
            program_path: None,
            program_content: None,
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
                                || self.imports.as_ref().map_or(false, |imp| {
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
                            for (_, resolved_path) in &imp.resolved_files {
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

        // Create FIFOs for communication
        let fifo_dir = std::env::temp_dir().join(format!("argsh-dap-{}", std::process::id()));
        std::fs::create_dir_all(&fifo_dir).ok();
        let fifo_data = fifo_dir.join("data");
        let fifo_ctl = fifo_dir.join("data.ctl");

        // Create named pipes
        #[cfg(unix)]
        unsafe {
            let data_c = std::ffi::CString::new(fifo_data.to_str().unwrap()).unwrap();
            let ctl_c = std::ffi::CString::new(fifo_ctl.to_str().unwrap()).unwrap();
            libc::mkfifo(data_c.as_ptr(), 0o600);
            libc::mkfifo(ctl_c.as_ptr(), 0o600);
        }

        // Build the wrapper script with the debug prelude
        let prelude = DEBUG_PRELUDE
            .replace("__FIFO_PATH__", fifo_data.to_str().unwrap())
            .replace("__STOP_ON_ENTRY__", if stop_on_entry { "1" } else { "0" });

        let wrapper = format!(
            "#!/usr/bin/env bash\nset -euo pipefail\n{}\nsource \"{}\" \"$@\"\n",
            prelude,
            program.display()
        );

        let wrapper_path = fifo_dir.join("wrapper.sh");
        std::fs::write(&wrapper_path, &wrapper).unwrap();

        // Spawn bash with the wrapper
        let mut cmd = Command::new("bash");
        cmd.arg(wrapper_path.to_str().unwrap());
        cmd.args(&script_args);
        cmd.current_dir(&cwd);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

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

        match cmd.spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.fifo_path = Some(fifo_data.clone());
                self.launched.store(true, Ordering::SeqCst);
                self.send_response(req, true, None, None);

                // Start background thread to read FIFO events
                let fifo_data_clone = fifo_data.clone();
                let stdout_writer = self.stdout_writer.clone();
                let seq = &self.seq as *const AtomicI64;
                // Safety: seq lives as long as the session
                let seq_ref = unsafe { &*seq };

                std::thread::spawn(move || {
                    fifo_reader_loop(&fifo_data_clone, &stdout_writer, seq_ref);
                });
            }
            Err(e) => {
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

            // If launched, update breakpoints + conditions in the running script
            if self.launched.load(Ordering::SeqCst) {
                if let Some(ref fifo) = self.fifo_path {
                    let ctl_path = format!("{}.ctl", fifo.display());
                    let bp_str: String = self.breakpoints.iter()
                        .flat_map(|(file, lines)| {
                            lines.iter().map(move |line| format!("{}:{}", file.display(), line))
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let _ = std::fs::write(&ctl_path, format!("breakpoints:{}\n", bp_str));

                    // Send conditions
                    for (file, line, cond) in &conditions {
                        let _ = std::fs::write(&ctl_path,
                            format!("condition:{}:{}:{}\n", file.display(), line, cond));
                    }
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
        self.send_response(req, true, Some(serde_json::json!({
            "threads": [{
                "id": 1,
                "name": "main"
            }]
        })), None);
    }

    fn handle_continue(&self, req: &DapMessage) {
        self.write_ctl("continue\n");
        self.send_response(req, true, Some(serde_json::json!({
            "allThreadsContinued": true,
        })), None);
    }

    fn handle_next(&self, req: &DapMessage) {
        self.write_ctl("next\n");
        self.send_response(req, true, None, None);
    }

    fn handle_step_in(&self, req: &DapMessage) {
        self.write_ctl("stepin\n");
        self.send_response(req, true, None, None);
    }

    fn handle_step_out(&self, req: &DapMessage) {
        self.write_ctl("stepout\n");
        self.send_response(req, true, None, None);
    }

    fn handle_stack_trace(&self, req: &DapMessage) {
        // The stack trace was sent with the last STOPPED event and cached
        // For now, return a single frame — the FIFO reader enriches this
        self.send_response(req, true, Some(serde_json::json!({
            "stackFrames": [],
            "totalFrames": 0,
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
        // Add Args Inspector scope if we have analysis data
        if self.analysis.is_some() {
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
                // Locals — via FIFO protocol (TODO: implement runtime var fetching)
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
    fn handle_set_variable(&self, req: &DapMessage) {
        let args = req.arguments.as_ref();
        let name = args.and_then(|a| a.get("name")).and_then(|n| n.as_str()).unwrap_or("");
        let value = args.and_then(|a| a.get("value")).and_then(|v| v.as_str()).unwrap_or("");

        if !name.is_empty() {
            // Send setvar command to the bash process via FIFO
            self.write_ctl(&format!("setvar:{}={}\n", name, value));
            self.send_response(req, true, Some(serde_json::json!({
                "value": value,
            })), None);
        } else {
            self.send_response(req, false, None, Some("Missing variable name".into()));
        }
    }

    /// (#92) Handle function breakpoints — resolve subcommand names to line breakpoints.
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

    fn write_ctl(&self, cmd: &str) {
        if let Some(ref fifo) = self.fifo_path {
            let ctl_path = format!("{}.ctl", fifo.display());
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&ctl_path) {
                let _ = f.write_all(cmd.as_bytes());
                let _ = f.flush();
            }
        }
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

fn fifo_reader_loop(fifo_path: &Path, stdout_writer: &Arc<Mutex<io::Stdout>>, seq: &AtomicI64) {
    loop {
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
                let parts: Vec<&str> = line.splitn(4, '\t').collect();
                if parts.len() >= 4 {
                    let file = parts[1];
                    let line_num: i64 = parts[2].parse().unwrap_or(0);
                    let _func = parts[3];

                    let evt = DapEvent {
                        seq: seq.fetch_add(1, Ordering::SeqCst),
                        msg_type: "event",
                        event: "stopped".to_string(),
                        body: Some(serde_json::json!({
                            "reason": "breakpoint",
                            "threadId": 1,
                            "allThreadsStopped": true,
                            "description": format!("Stopped at {}:{}", file, line_num),
                        })),
                    };
                    send_dap_message(stdout_writer, &evt);
                }
            }
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
// Main
// ---------------------------------------------------------------------------

fn main() {
    // Handle --version and --help
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
                println!("This binary is invoked by VSCode's debug adapter infrastructure.");
                println!("It is not intended to be run directly.");
                println!();
                println!("Usage: argsh-dap");
                println!("       argsh-dap --version");
                return;
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

    loop {
        let msg = match read_dap_message(&mut reader) {
            Some(m) => m,
            None => break, // EOF — VSCode closed the connection
        };

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
