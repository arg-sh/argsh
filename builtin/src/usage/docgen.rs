//! :usage::docgen builtin -- documentation generation in various formats.
//!
//! Mirrors: libraries/args.sh

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::shared;
use crate::shell;
use std::ffi::{c_char, c_int};
use std::io::Write;
use super::{
    extract_subcommands, extract_flags, extract_flags_for_llm,
    json_escape, sanitize_tool_name, write_tool_properties,
    FlagInfo,
};

// -- :usage::docgen builtin registration --------------------------------------

static USAGE_DOCGEN_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Generate documentation (man, md, rst, yaml, llm).".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":usage::docgen_struct"]
pub static mut USAGE_DOCGEN_STRUCT: BashBuiltin = BashBuiltin {
    name: c":usage::docgen".as_ptr(),
    function: usage_docgen_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c":usage::docgen <format> [-- title usage_pairs...]".as_ptr(),
    long_doc: USAGE_DOCGEN_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":usage::docgen_builtin_load"]
pub extern "C" fn usage_docgen_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":usage::docgen_builtin_unload"]
pub extern "C" fn usage_docgen_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback

extern "C" fn usage_docgen_builtin_fn(word_list: *const WordList) -> c_int {
    let code = std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        usage_docgen_main(&args)
    })
    .unwrap_or(1); // coverage:off - catch_unwind: panics don't occur in practice

    std::process::exit(if code == shared::HELP_EXIT || code == 0 { 0 } else { code }) // coverage:off
}

// -- :usage::docgen implementation --------------------------------------------

/// Main entry point for :usage::docgen builtin.
/// Called via "${usage[@]}" — generates documentation in various formats.
/// Args: [format] [-- title original_usage_pairs...]
pub fn usage_docgen_main(args: &[String]) -> i32 {
    let sep = args.iter().position(|s| s == "--");
    let (user_args, meta) = match sep {
        Some(pos) => (&args[..pos], &args[pos + 1..]),
        None => (args, [].as_slice()), // coverage:off - defensive_check: deferred dispatch always provides "--"
    };

    if user_args.is_empty() || user_args[0] == "-h" || user_args[0] == "--help" {
        let commandname = shell::get_commandname();
        let cmd_str = if commandname.len() > 1 {
            commandname[..commandname.len() - 1].join(" ")
        } else {
            shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
        };
        println!("Generate documentation in various formats.\n");
        println!("Usage: {} docgen <format>\n", cmd_str);
        println!("Available formats:");
        println!("  man     Man page (troff format)");
        println!("  md      Markdown");
        println!("  rst     reStructuredText");
        println!("  yaml    YAML");
        println!("  llm     LLM tool schema (claude, openai, gemini, kimi)");
        return shared::HELP_EXIT;
    }

    let format = &user_args[0];
    let title = meta.first().map(|s| s.as_str()).unwrap_or("");
    let usage_pairs = if meta.len() > 1 { &meta[1..] } else { &[] as &[String] }; // coverage:off - defensive_check: deferred dispatch always provides title + usage_pairs
    let args_arr = shell::read_array("args");

    // Full command path for display (e.g. "myapp sub" instead of just "sub").
    // Excludes the trailing "docgen" entry from COMMANDNAME.
    let commandname = shell::get_commandname();
    let cmd_name = if commandname.len() > 1 {
        commandname[..commandname.len() - 1].join(" ")
    } else {
        shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
    };

    let out = std::io::stdout();
    let mut out = out.lock();

    match format.as_str() {
        "man" => generate_man_page(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "md" => generate_markdown(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "rst" => generate_rst(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "yaml" => generate_yaml(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "llm" => {
            let provider = user_args.get(1).map(|s| s.as_str());
            match provider {
                Some("claude") | Some("anthropic") => {
                    generate_llm_claude(&mut out, &cmd_name, title, usage_pairs, &args_arr);
                }
                Some("openai") | Some("gemini") | Some("kimi") => {
                    generate_llm_openai(&mut out, &cmd_name, title, usage_pairs, &args_arr);
                }
                Some(unknown) => {
                    return shared::error_usage("", &format!(
                        "unknown LLM provider: {}. Use claude, openai, gemini, or kimi", unknown
                    ));
                }
                None => {
                    return shared::error_usage("", "llm format requires a provider: claude, openai, gemini, or kimi");
                }
            }
        }
        _ => {
            return shared::error_usage("", &format!(
                "unknown format: {}. Use man, md, rst, yaml, or llm", format
            ));
        }
    }
    shared::HELP_EXIT
}

// -- Man page generation ------------------------------------------------------

/// Generate man page in troff format.
fn generate_man_page<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let upper_name = cmd_name.to_uppercase();
    let first_line = title.lines().next().unwrap_or(title).trim();

    // Header
    let _ = writeln!(out, ".TH \"{}\" 1", upper_name);

    // NAME
    let _ = writeln!(out, ".SH NAME");
    let _ = writeln!(out, "{} \\- {}", cmd_name, man_escape(first_line));

    // SYNOPSIS
    let _ = writeln!(out, ".SH SYNOPSIS");
    let _ = writeln!(out, ".B {}", cmd_name);
    if !cmds.is_empty() {
        let _ = writeln!(out, ".RI [ command ]");
    }
    let _ = writeln!(out, ".RI [ options ]");

    // DESCRIPTION
    let _ = writeln!(out, ".SH DESCRIPTION");
    for line in title.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            let _ = writeln!(out, ".PP");
        } else {
            let _ = writeln!(out, "{}", man_escape(trimmed));
        }
    }

    // COMMANDS
    if !cmds.is_empty() {
        let _ = writeln!(out, ".SH COMMANDS");
        for cmd in &cmds {
            let _ = writeln!(out, ".TP");
            let _ = writeln!(out, ".B {}", cmd.name);
            let _ = writeln!(out, "{}", man_escape(&cmd.desc));
        }
    }

    // OPTIONS
    if !flags.is_empty() {
        let _ = writeln!(out, ".SH OPTIONS");
        for flag in &flags {
            let _ = writeln!(out, ".TP");
            if let Some(ref short) = flag.short {
                if flag.is_boolean {
                    let _ = writeln!(out, ".BR \\-{} \", \" \\-\\-{}", short, flag.name);
                } else {
                    let _ = writeln!(out, ".BR \\-{} \", \" \\-\\-{} \" \" \\fI{}\\fR",
                        short, flag.name, flag.type_name);
                }
            } else if flag.is_boolean {
                let _ = writeln!(out, ".BR \\-\\-{}", flag.name);
            } else {
                let _ = writeln!(out, ".BR \\-\\-{} \" \" \\fI{}\\fR", flag.name, flag.type_name);
            }
            let _ = writeln!(out, "{}", man_escape(&flag.desc));
        }
    }
}

/// Escape special troff characters.
/// Also neutralizes lines starting with '.' or '\'' which roff interprets as macros.
fn man_escape(s: &str) -> String {
    s.lines()
        .map(|line| {
            let escaped = line.replace('\\', "\\\\").replace('-', "\\-");
            let trimmed = escaped.trim_start();
            if trimmed.starts_with('.') || trimmed.starts_with('\'') {
                let ws_len = escaped.len() - trimmed.len();
                let (prefix, rest) = escaped.split_at(ws_len);
                format!("{}\\&{}", prefix, rest)
            } else {
                escaped
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// -- Markdown generation ------------------------------------------------------

/// Generate documentation as Markdown.
fn generate_markdown<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    let _ = writeln!(out, "# {}\n", cmd_name);
    let _ = writeln!(out, "{}\n", first_line);

    // Synopsis
    let _ = writeln!(out, "## Synopsis\n");
    let _ = write!(out, "```\n{}", cmd_name);
    if !cmds.is_empty() {
        let _ = write!(out, " [command]");
    }
    let _ = writeln!(out, " [options]\n```\n");

    // Description (skip first line since it's already shown as summary above)
    let remaining: Vec<&str> = title.lines().skip(1).collect();
    if !remaining.is_empty() {
        let _ = writeln!(out, "## Description\n");
        for line in &remaining {
            let _ = writeln!(out, "{}", line.trim());
        }
        let _ = writeln!(out);
    }

    // Commands
    if !cmds.is_empty() {
        let _ = writeln!(out, "## Commands\n");
        let _ = writeln!(out, "| Command | Description |");
        let _ = writeln!(out, "|---------|-------------|");
        for cmd in &cmds {
            let _ = writeln!(out, "| `{}` | {} |", cmd.name, cmd.desc);
        }
        let _ = writeln!(out);
    }

    // Options
    if !flags.is_empty() {
        let _ = writeln!(out, "## Options\n");
        let _ = writeln!(out, "| Flag | Description |");
        let _ = writeln!(out, "|------|-------------|");
        for flag in &flags {
            let mut flag_str = format!("`--{}`", flag.name);
            if let Some(ref short) = flag.short {
                flag_str = format!("`-{}`, {}", short, flag_str);
            }
            if !flag.is_boolean {
                flag_str.push_str(&format!(" *{}*", flag.type_name));
            }
            let _ = writeln!(out, "| {} | {} |", flag_str, flag.desc);
        }
        let _ = writeln!(out);
    }
}

// -- reStructuredText generation ----------------------------------------------

/// Generate documentation as reStructuredText.
fn generate_rst<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    // Title
    let underline: String = "=".repeat(cmd_name.len());
    let _ = writeln!(out, "{}", cmd_name);
    let _ = writeln!(out, "{}\n", underline);
    let _ = writeln!(out, "{}\n", first_line);

    // Synopsis
    let _ = writeln!(out, "Synopsis");
    let _ = writeln!(out, "--------\n");
    let _ = writeln!(out, ".. code-block:: bash\n");
    let _ = write!(out, "   {}", cmd_name);
    if !cmds.is_empty() {
        let _ = write!(out, " [command]");
    }
    let _ = writeln!(out, " [options]\n");

    // Description (skip first line since it's already shown as summary above)
    let remaining: Vec<&str> = title.lines().skip(1).collect();
    if !remaining.is_empty() {
        let _ = writeln!(out, "Description");
        let _ = writeln!(out, "-----------\n");
        for line in &remaining {
            let _ = writeln!(out, "{}", line.trim());
        }
        let _ = writeln!(out);
    }

    // Commands
    if !cmds.is_empty() {
        let _ = writeln!(out, "Commands");
        let _ = writeln!(out, "--------\n");
        for cmd in &cmds {
            let _ = writeln!(out, "**{}**", cmd.name);
            let _ = writeln!(out, "   {}\n", cmd.desc);
        }
    }

    // Options
    if !flags.is_empty() {
        let _ = writeln!(out, "Options");
        let _ = writeln!(out, "-------\n");
        for flag in &flags {
            let mut flag_str = format!("--{}", flag.name);
            if let Some(ref short) = flag.short {
                flag_str = format!("-{}, {}", short, flag_str);
            }
            if !flag.is_boolean {
                flag_str.push_str(&format!(" *{}*", flag.type_name));
            }
            let _ = writeln!(out, "**{}**", flag_str);
            let _ = writeln!(out, "   {}\n", flag.desc);
        }
    }
}

// -- YAML generation ----------------------------------------------------------

/// Escape a string for YAML double-quoted output.
fn yaml_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Generate documentation as YAML.
fn generate_yaml<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    let _ = writeln!(out, "name: \"{}\"", yaml_escape(cmd_name));
    let _ = writeln!(out, "description: \"{}\"", yaml_escape(first_line));

    let synopsis = if !cmds.is_empty() {
        format!("{} [command] [options]", cmd_name)
    } else {
        format!("{} [options]", cmd_name)
    };
    let _ = writeln!(out, "synopsis: \"{}\"", yaml_escape(&synopsis));

    // Commands
    if !cmds.is_empty() {
        let _ = writeln!(out, "commands:");
        for cmd in &cmds {
            let _ = writeln!(out, "  - name: \"{}\"", yaml_escape(&cmd.name));
            let _ = writeln!(out, "    description: \"{}\"", yaml_escape(&cmd.desc));
        }
    }

    // Options
    if !flags.is_empty() {
        let _ = writeln!(out, "options:");
        for flag in &flags {
            let _ = writeln!(out, "  - name: \"{}\"", yaml_escape(&flag.name));
            if let Some(ref short) = flag.short {
                let _ = writeln!(out, "    short: \"{}\"", yaml_escape(short));
            }
            let _ = writeln!(out, "    description: \"{}\"", yaml_escape(&flag.desc));
            if flag.is_boolean {
                let _ = writeln!(out, "    type: boolean");
            } else {
                let _ = writeln!(out, "    type: \"{}\"", yaml_escape(&flag.type_name));
            }
        }
    }
}

// -- LLM tool schema generation -----------------------------------------------

/// Generate LLM tool schema in Anthropic Claude format.
fn generate_llm_claude<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags_for_llm(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    let _ = writeln!(out, "[");

    if cmds.is_empty() {
        write_claude_tool(out, &sanitize_tool_name(cmd_name), first_line, &flags, true);
    } else {
        for (i, cmd) in cmds.iter().enumerate() {
            let tool_name = sanitize_tool_name(&format!("{}_{}", cmd_name, cmd.name));
            let desc = if cmd.desc.is_empty() { first_line } else { &cmd.desc };
            write_claude_tool(out, &tool_name, desc, &flags, i == cmds.len() - 1);
        }
    }

    let _ = writeln!(out, "]");
}

fn write_claude_tool<W: Write>(out: &mut W, name: &str, description: &str, flags: &[FlagInfo], is_last: bool) {
    let _ = writeln!(out, "  {{");
    let _ = writeln!(out, "    \"name\": \"{}\",", json_escape(name));
    let _ = writeln!(out, "    \"description\": \"{}\",", json_escape(description));
    let _ = writeln!(out, "    \"input_schema\": {{");
    let _ = writeln!(out, "      \"type\": \"object\",");
    write_tool_properties(out, flags, "      ");
    let _ = writeln!(out, "    }}");
    let trailing = if is_last { "" } else { "," };
    let _ = writeln!(out, "  }}{}", trailing);
}

/// Generate LLM tool schema in OpenAI function calling format.
/// Also used for Gemini and Kimi (OpenAI-compatible).
fn generate_llm_openai<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags_for_llm(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    let _ = writeln!(out, "[");

    if cmds.is_empty() {
        write_openai_tool(out, &sanitize_tool_name(cmd_name), first_line, &flags, true);
    } else {
        for (i, cmd) in cmds.iter().enumerate() {
            let tool_name = sanitize_tool_name(&format!("{}_{}", cmd_name, cmd.name));
            let desc = if cmd.desc.is_empty() { first_line } else { &cmd.desc };
            write_openai_tool(out, &tool_name, desc, &flags, i == cmds.len() - 1);
        }
    }

    let _ = writeln!(out, "]");
}

fn write_openai_tool<W: Write>(out: &mut W, name: &str, description: &str, flags: &[FlagInfo], is_last: bool) {
    let _ = writeln!(out, "  {{");
    let _ = writeln!(out, "    \"type\": \"function\",");
    let _ = writeln!(out, "    \"function\": {{");
    let _ = writeln!(out, "      \"name\": \"{}\",", json_escape(name));
    let _ = writeln!(out, "      \"description\": \"{}\",", json_escape(description));
    let _ = writeln!(out, "      \"parameters\": {{");
    let _ = writeln!(out, "        \"type\": \"object\",");
    write_tool_properties(out, flags, "        ");
    let _ = writeln!(out, "      }}");
    let _ = writeln!(out, "    }}");
    let trailing = if is_last { "" } else { "," };
    let _ = writeln!(out, "  }}{}", trailing);
}
