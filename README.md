<h3 align="center">
	<img src="https://bashlogo.com/img/symbol/svg/full_colored_light.svg" width="16" alt="Logo"/>
	arg.sh
</h3>

<h6 align="center">
  <a href="https://arg.sh/getting-started">Quickstart</a>
  ·
  <a href="https://arg.sh/command-line-parser">CLI Parser</a>
  ·
  <a href="https://arg.sh/libraries/overview">Libraries</a>
  ·
  <a href="https://arg.sh/styleguide">Styleguide</a>
</h6>

<p align="center">
	<a href="https://github.com/arg-sh/argsh/stargazers">
		<img alt="Stargazers" src="https://img.shields.io/github/stars/arg-sh/argsh?style=for-the-badge&logo=starship&color=C9CBFF&logoColor=D9E0EE&labelColor=302D41"></a>
	<a href="https://github.com/arg-sh/argsh/releases/latest">
		<img alt="Releases" src="https://img.shields.io/github/release/arg-sh/argsh.svg?style=for-the-badge&logo=github&color=F2CDCD&logoColor=D9E0EE&labelColor=302D41"/></a>
	<a href="https://github.com/arg-sh/argsh/blob/main/coverage/coverage.json">
		<img alt="Bash Coverage" src="https://img.shields.io/badge/dynamic/json?style=for-the-badge&logo=gnubash&color=F2CDCD&logoColor=D9E0EE&labelColor=302D41&url=https%3A%2F%2Fraw.githubusercontent.com%2Farg-sh%2Fargsh%2Fmain%2Fcoverage%2Fcoverage.json&query=%24.percent_covered&label=Bash&suffix=%25"/></a>
	<a href="https://github.com/arg-sh/argsh/blob/main/builtin/coverage.json">
		<img alt="Rust Coverage" src="https://img.shields.io/badge/dynamic/json?style=for-the-badge&logo=rust&color=F2CDCD&logoColor=D9E0EE&labelColor=302D41&url=https%3A%2F%2Fraw.githubusercontent.com%2Farg-sh%2Fargsh%2Fmain%2Fbuiltin%2Fcoverage.json&query=%24.percent_covered&label=Rust&suffix=%25"/></a>
</p>

&nbsp;

<p align="left">
Bash is a powerful tool (and widly available), but it's also a language that is easy to write in a way that is hard to read and maintain. As such Bash is used often but used as little as possible, resulting in poor quality scripts that are hard to maintain and understand.

Not only is this happaning as Bash is seen as a "glue" language, but also because there is no hardend styleguide, easy testing and good documentation around it.

The Google Shell Style Guide says it itself:

> If you are writing a script that is more than 100 lines long, or that uses non-straightforward control flow logic, you should rewrite it in a more structured language now.

You can write bad code in every other language too, but there is lots of effort to make it better. So let's make it better for bash too. Let's make Bash a more structured language.

This is what argsh is trying to do. Check out the [Quickstart](https://arg.sh/getting-started) to see how you can use it.
</p>

&nbsp;

### 📦 Install

```bash
curl -sL https://min.arg.sh > .bin/argsh
chmod +x .bin/argsh
```

Or use the interactive installer:

```bash
bash -c "$(curl -sL https://get.arg.sh)"
```

See the [Getting Started](https://arg.sh/getting-started) guide for more options.

&nbsp;

### 🧠 Design Philosophy

- **First class citizen**: Treat your scripts as first class citizens. They are important and should be treated as such.
- **Be Consistent**: Consistency is key. It makes your scripts easier to read and maintain.
- **Perfect is the enemy of good**: Don't try to make your scripts perfect. Make them good and maintainable.
- **Write for the next person**: Write your scripts for the next person that has to read and maintain them. This person might be you.

&nbsp;

### 🔧 CLI Parser

argsh turns a plain Bash array into a full CLI — flags, types, defaults, validation and help — with zero boilerplate.

```bash
#!/usr/bin/env bash
source argsh

main() {
  local name age verbose
  local -a args=(
    "name|n:!    Name of the person"
    "age|a:int   Age in years"
    "verbose|v:+ Enable verbose output"
  )
  :args "Greet someone" "${@}"

  echo "Hello ${name}, you are ${age} years old."
}

main "${@}"
```

```console
$ ./greet --name World --age 42
Hello World, you are 42 years old.

$ ./greet --help
Greet someone

Options:
   --name, -n     string  Name of the person (required)
   --age, -a      int     Age in years
   --verbose, -v          Enable verbose output
   --help, -h             Show this help message
```

The `:args` builtin handles `--flag value`, `-f value`, `--flag=value`, `--no-flag` (booleans), automatic `--help`/`-h`, unknown flag errors, and type validation (`int`, `float`, `boolean`, `file`, or custom). See the [CLI Parser docs](https://arg.sh/command-line-parser) for the full syntax.

&nbsp;

### 🗂️ Subcommand Routing

`:usage` gives you git-style subcommands with auto-generated help, fuzzy suggestions on typos, and convention-based function dispatch.

```bash
#!/usr/bin/env bash
source argsh

main::deploy() {
  local args=(
    "env|e:!  Target environment"
  )
  :args "Deploy the application" "${@}"
  echo "Deploying to ${env}..."
}

main::status() {
  :args "Show deployment status" "${@}"
  echo "All systems operational."
}

main() {
  local usage=(
    'deploy|d' "Deploy the application"
    'status|s' "Show deployment status"
  )
  :usage "Application manager" "${@}"
  "${usage[@]}"
}

main "${@}"
```

```console
$ ./app deploy --env production
Deploying to production...

$ ./app stat
Invalid command: stat. Did you mean 'status'?
```

Each subcommand maps to a function by convention (`main::deploy`, `main::status`). Nested subcommands compose naturally — just add another `:usage` inside a subcommand function.

&nbsp;

### 🤖 AI Integration

Every argsh script is AI-ready out of the box — no glue code required.

**MCP Server** — expose subcommands as tools for AI agents over [Model Context Protocol](https://modelcontextprotocol.io):

```bash
./myscript mcp            # starts JSON-RPC 2.0 stdio server
./myscript mcp --help     # prints .mcp.json config snippet
```

**LLM Tool Schemas** — generate ready-to-use tool definitions for AI APIs:

```bash
./myscript docgen llm claude   # Anthropic tool array (input_schema)
./myscript docgen llm openai   # OpenAI function calling format
./myscript docgen llm gemini   # Gemini (OpenAI-compatible)
```

**Shell Completions & Docs** — also generated from the same source:

```bash
./myscript completion bash|zsh|fish
./myscript docgen man|md|rst|yaml
```

See the [AI Integration docs](https://arg.sh/ai) for details on MCP and LLM tool schemas.

&nbsp;

### ⚡ Native Builtins (Rust)

argsh ships with optional **Bash loadable builtins** compiled from Rust. When the shared library is available, the core parsing commands (`:args`, `:usage`, type converters, etc.) run as native code inside the Bash process — zero fork overhead, zero subshell cost.

| Builtin | Purpose |
|---|---|
| `:args` | CLI argument parser with type checking |
| `:usage` | Subcommand router with intelligent suggestions |
| `:usage::help` | Deferred help display (runs after setup code) |
| `is::array`, `is::uninitialized`, `is::set`, `is::tty` | Variable introspection |
| `to::int`, `to::float`, `to::boolean`, `to::file`, `to::string` | Type converters |
| `args::field_name` | Field name extraction |
| `:usage::completion` | Autocomplete backend for `:usage completion` (bash, zsh, fish) |
| `:usage::docgen` | Documentation backend for `:usage docgen` (man, md, rst, yaml, llm) |
| `:usage::mcp` | MCP server backend for `:usage mcp` (JSON-RPC 2.0 over stdio) |

**Transparent fallback** — `args.sh` auto-detects the `.so` at load time. If found, builtins are enabled via `enable -f` and the pure-Bash function definitions are skipped. If not found, everything works as before with no change in behavior.

```bash
# Build (requires Rust toolchain)
cd builtin && cargo build --release
# Output: builtin/target/release/libargsh.so

# Copy to PATH_BIN and auto-load
cp builtin/target/release/libargsh.so .bin/argsh.so
source libraries/args.sh

# Or set explicit path
export ARGSH_BUILTIN_PATH="/path/to/argsh.so"
source libraries/args.sh
```

Search order: `ARGSH_BUILTIN_PATH` > `PATH_LIB` > `PATH_BIN` > `LD_LIBRARY_PATH` > `BASH_LOADABLES_PATH`

#### Benchmark

Subcommand dispatch (`cmd x x ... x -h`) — 50 iterations:

| Depth | Pure Bash | Builtin | Speedup |
|------:|----------:|--------:|--------:|
|    10 |   1188 ms |   21 ms |    57x  |
|    25 |   2686 ms |   53 ms |    51x  |
|    50 |   5434 ms |  155 ms |    35x  |

Argument parsing (`cmd --flag1 v1 ... --flagN vN`) — 50 iterations:

| Flags | Pure Bash | Builtin | Speedup |
|------:|----------:|--------:|--------:|
|    10 |   5405 ms |    4 ms |  1351x  |
|    25 |  13986 ms |    9 ms |  1554x  |
|    50 |  29603 ms |   20 ms |  1480x  |

Run `bash bench/usage-depth.sh` to reproduce.

&nbsp;

### 🚧 State of this Project

> This project is in a very early stage.

That being said, most of it is quite rough. But it's a start. The best time that you join the conversation and try to refine the concept.

#### Short term goals

- [ ] Design a logo
- [ ] Write a language server to lint and format bash code according to the styleguide
- [ ] VSCode extension for the language server
- [x] Convert [shdoc](https://github.com/reconquest/shdoc) to rust
- [ ] Bash debugger integration (e.g. with `bashdb`)

&nbsp;

### 📜 License

Argsh is released under the MIT license, which grants the following permissions:

- Commercial use
- Distribution
- Modification
- Private use

For more convoluted language, see the [LICENSE](https://github.com/arg-sh/argsh/blob/main/LICENSE). Let's build a better Bash experience together.

&nbsp;

### ❤️ Gratitude

Thanks to the following tools and projects developing this project is possible:

- [medusajs](https://github.com/medusajs/medusa/): From where the base of this docs, github and more is copied.
- [Google Styleguide](https://google.github.io/styleguide/shellguide.html): Google's Shell Style Guide used as base for the argsh styleguide.
- [Catppuccin](https://github.com/catppuccin/catppuccin): Base for the readme.md and general nice color palettes.

&nbsp;

### 🐾 Projects to follow

- [bash-it](https://github.com/Bash-it/bash-it): A Bash shell - autocompletion, themes, aliases, custom functions, and more.

&nbsp;

<p align="center">Copyright &copy; 2024-present <a href="https://github.com/fentas" target="_blank">Jan Guth</a>
<p align="center"><a href="https://github.com/arg-sh/argsh/blob/main/LICENSE"><img src="https://img.shields.io/static/v1.svg?style=for-the-badge&label=License&message=MIT&logoColor=d9e0ee&colorA=302d41&colorB=b7bdf8"/></a></p>
