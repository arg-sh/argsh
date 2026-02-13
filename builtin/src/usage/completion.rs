//! :usage::completion builtin -- shell completion script generation.

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::shared;
use crate::shell;
use std::ffi::{c_char, c_int};
use std::io::Write;
use super::{extract_subcommands, extract_flags};

// -- :usage::completion builtin registration ----------------------------------

static USAGE_COMPLETION_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Generate shell completion scripts (bash, zsh, fish).".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":usage::completion_struct"]
pub static mut USAGE_COMPLETION_STRUCT: BashBuiltin = BashBuiltin {
    name: c":usage::completion".as_ptr(),
    function: usage_completion_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c":usage::completion <shell> [-- title usage_pairs...]".as_ptr(),
    long_doc: USAGE_COMPLETION_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":usage::completion_builtin_load"]
pub extern "C" fn usage_completion_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":usage::completion_builtin_unload"]
pub extern "C" fn usage_completion_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback

extern "C" fn usage_completion_builtin_fn(word_list: *const WordList) -> c_int {
    let code = std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        usage_completion_main(&args)
    })
    .unwrap_or(1); // coverage:off - catch_unwind: panics don't occur in practice

    std::process::exit(if code == shared::HELP_EXIT || code == 0 { 0 } else { code }) // coverage:off
}

// -- :usage::completion implementation ----------------------------------------

/// Main entry point for :usage::completion builtin.
/// Called via "${usage[@]}" — generates shell completion scripts.
/// Args: [shell_type] [-- title original_usage_pairs...]
pub fn usage_completion_main(args: &[String]) -> i32 {
    let sep = args.iter().position(|s| s == "--");
    let (user_args, meta) = match sep {
        Some(pos) => (&args[..pos], &args[pos + 1..]),
        None => (args, [].as_slice()),
    };

    if user_args.is_empty() || user_args[0] == "-h" || user_args[0] == "--help" {
        let commandname = shell::get_commandname();
        let cmd_str = if commandname.len() > 1 {
            commandname[..commandname.len() - 1].join(" ")
        } else {
            shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
        };
        println!("Generate shell completion scripts.\n");
        println!("Usage: {} completion <shell>\n", cmd_str);
        println!("Available shells:");
        println!("  bash    Bash completion script");
        println!("  zsh     Zsh completion script");
        println!("  fish    Fish completion script");
        return shared::HELP_EXIT;
    }

    let shell_type = &user_args[0];
    let title = meta.first().map(|s| s.as_str()).unwrap_or("");
    let usage_pairs = if meta.len() > 1 { &meta[1..] } else { &[] as &[String] };
    let args_arr = shell::read_array("args");

    // Base command name (COMMANDNAME minus "completion" at the end)
    let commandname = shell::get_commandname();
    let cmd_name = if commandname.len() > 1 {
        commandname[commandname.len() - 2].clone()
    } else {
        shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
    };

    let out = std::io::stdout();
    let mut out = out.lock();

    match shell_type.as_str() {
        "bash" => generate_bash_completion(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "zsh" => generate_zsh_completion(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "fish" => generate_fish_completion(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        _ => {
            return shared::error_usage("", &format!(
                "unknown shell: {}. Use bash, zsh, or fish", shell_type
            ));
        }
    }
    shared::HELP_EXIT
}

// -- Completion generators ----------------------------------------------------

/// Generate bash completion script.
fn generate_bash_completion<W: Write>(
    out: &mut W,
    cmd_name: &str,
    _title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let func_name = format!("_{}", cmd_name.replace('-', "_"));

    let _ = writeln!(out, "# bash completion for {}", cmd_name);
    let _ = writeln!(out, "{}() {{", func_name);
    let _ = writeln!(out, "    local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"");
    let _ = writeln!(out);

    // Flags
    let flag_words: Vec<String> = flags.iter().flat_map(|f| {
        let mut words = vec![format!("--{}", f.name)];
        if let Some(ref s) = f.short {
            words.push(format!("-{}", s));
        }
        words
    }).collect();

    let _ = writeln!(out, "    if [[ \"${{cur}}\" == -* ]]; then");
    let _ = writeln!(out, "        COMPREPLY=($(compgen -W \"{}\" -- \"${{cur}}\"))", flag_words.join(" "));
    let _ = writeln!(out, "    else");

    // Subcommands
    let cmd_words: Vec<&str> = cmds.iter().map(|c| c.name.as_str()).collect();
    let _ = writeln!(out, "        COMPREPLY=($(compgen -W \"{}\" -- \"${{cur}}\"))", cmd_words.join(" "));
    let _ = writeln!(out, "    fi");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "complete -o default -F {} {}", func_name, cmd_name);
}

/// Generate zsh completion script.
fn generate_zsh_completion<W: Write>(
    out: &mut W,
    cmd_name: &str,
    _title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let func_name = format!("_{}", cmd_name.replace('-', "_"));

    let _ = writeln!(out, "#compdef {}", cmd_name);
    let _ = writeln!(out);
    let _ = writeln!(out, "{}() {{", func_name);

    if !cmds.is_empty() {
        let _ = writeln!(out, "    local -a commands=(");
        for cmd in &cmds {
            let esc_desc = cmd.desc.replace('\'', "'\\''");
            let _ = writeln!(out, "        '{}:{}'", cmd.name, esc_desc);
        }
        let _ = writeln!(out, "    )");
        let _ = writeln!(out);
    }

    let _ = write!(out, "    _arguments -s");

    for flag in &flags {
        let long = &flag.name;
        let esc_desc = flag.desc.replace('\'', "'\\''").replace('[', "\\[").replace(']', "\\]");
        if let Some(ref short) = flag.short {
            if flag.is_boolean {
                let _ = write!(out, " \\\n        '(-{} --{})'{{\"-{}\",\"--{}\"}}'[{}]'",
                    short, long, short, long, esc_desc);
            } else {
                let _ = write!(out, " \\\n        '(-{} --{})'{{\"-{}\",\"--{}\"}}'[{}]:{}:'",
                    short, long, short, long, esc_desc, flag.type_name);
            }
        } else if flag.is_boolean {
            let _ = write!(out, " \\\n        '--{}[{}]'", long, esc_desc);
        } else {
            let _ = write!(out, " \\\n        '--{}[{}]:{}:'", long, esc_desc, flag.type_name);
        }
    }

    if !cmds.is_empty() {
        let _ = writeln!(out, " \\\n        '*::command:->commands'");
        let _ = writeln!(out);
        let _ = writeln!(out, "    case \"$state\" in");
        let _ = writeln!(out, "        commands)");
        let _ = writeln!(out, "            _describe 'command' commands");
        let _ = writeln!(out, "            ;;");
        let _ = writeln!(out, "    esac");
    } else {
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "}}");
    let _ = writeln!(out);
    let _ = writeln!(out, "{} \"$@\"", func_name);
}

/// Generate fish completion script.
fn generate_fish_completion<W: Write>(
    out: &mut W,
    cmd_name: &str,
    _title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);

    let _ = writeln!(out, "# fish completion for {}", cmd_name);

    // Subcommands
    for cmd in &cmds {
        let esc_desc = cmd.desc.replace('\'', "\\'");
        let _ = writeln!(out, "complete -c {} -n '__fish_use_subcommand' -a '{}' -d '{}'",
            cmd_name, cmd.name, esc_desc);
    }

    // Flags
    for flag in &flags {
        let esc_desc = flag.desc.replace('\'', "\\'");
        let mut parts = format!("complete -c {} -l '{}'", cmd_name, flag.name);
        if let Some(ref short) = flag.short {
            parts.push_str(&format!(" -s '{}'", short));
        }
        if !flag.is_boolean {
            parts.push_str(" -r");
        }
        parts.push_str(&format!(" -d '{}'", esc_desc));
        let _ = writeln!(out, "{}", parts);
    }
}
