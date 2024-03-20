---
description: 'Learn how to use the command line parser to parse command line arguments in argsh.'
---

# Command Line Parser

In this document, you’ll learn how to use the command line parser to parse command line arguments in Argsh.

## Overview

The implementation of the command line parser in argsh is inspired by the [cobra](https://github.com/spf13/cobra) library for Go and used as a reference for the implementation. 

- [x] Easy subcommand-based CLIs: app server, app fetch, etc.
- [x] Fully POSIX-compliant flags (including short & long versions)
- [x] Nested subcommands
- [x] Global, local and cascading flags
- [ ] Intelligent suggestions (app srver... did you mean app server?)
- [x] Automatic help generation for commands and flags
- [x] Grouping help for subcommands
- [x] Automatic help flag recognition of -h, --help, etc.
- [ ] Automatically generated shell autocomplete for your application (bash, zsh, fish, powershell)
- [ ] Automatically generated man pages for your application
- [x] Command aliases so you can change things without breaking them
- [ ] The flexibility to define your own help, usage, etc.

## Usage

Argsh provides a simple way to define commands and flags. It consists of two parts, a root command `:usage` and arguments `:args`.

:::warn
argsh is still in development and prune to change.
:::

### Setup

To use the command line parser, you need to include the `argsh` library in your script or install it. Read more about how to set up argsh in the [getting started guide](/getting-started).

:::note
If you are using argsh in the shebang line, your script will be executed within a function scope. So wouldn't need to implement a `main` function and can use `local` variables directly.
:::

### Root command

The root command is defined by the [:usage](../../libraries/args.mdx#:usage) function. It takes a array defining available commands and flags.

```bash
local -a usage=(
  'command1' "Description of command1"
  'command2' "Description of command2"
)
:usage "Brief description of your script" "${@}"

echo "Being here means that command1 or command2 was called"
case "${1}" in
  command1)
    echo "Command 1 was called"
    ;;
  command2)
    echo "Command 2 was called"
    ;;
  *)
    echo "Unknown command"
    ;;
esac
```

As `:usage` only succeeds if a valid command is called, you can use it to define/name the command as one of your script's functions.

```bash
local -a usage=(
  'command1' "Description of command1"
  'command2' "Description of command2"
)
:usage "Brief description of your script" "${@}"
"${1}" "${@:2}"
```

#### Aliases

You can also define aliases and a default.

```bash

local -a usage=(
  'command1'             "Description of command1"
  'command2|cmd2'        "Description of command2"
  'uhh|ahh:-command3'    "Description of command3"
)
:usage "Brief description of your script" "${@}"
# Pre-run
"${usage[@]}"
# Post-run
```

- `command1` is called with `script command1`
- `command2` is called with `script command2` or `script cmd2`
- `command3` is called with `script uhh` or `script ahh`

:::note
We are using the `usage` array as a function call. `:usage` populates the `usage` array with the command name and rest of the arguments. This is why we can use the `usage` array as a function call.
:::

#### Subcommands

You can define subcommands by defining another `usage` array in the subcommand function.

```bash
subcommand1() {
  :args "Brief description of your subcommand" "${@}"
  echo "Being here means that subcommand1 was called"
}

command1() {
  local -a usage=(
    'subcommand1' "Description of subcommand1"
    'subcommand2' "Description of subcommand2"
  )
  :usage "Brief description of your command" "${@}"
  "${usage[@]}"
}

main() {
  local -a usage=(
    'command1' "Description of command1"
    'command2' "Description of command2"
  )
  :usage "Brief description of your script" "${@}"
  "${usage[@]}"
}

[[ "${BASH_SOURCE[0]}" != "${0}" ]] || main "${@}"
```

In this example, `subcommand1` is called with `script command1 subcommand1`.

#### Hidden commands

You can define hidden commands by prepending `#` to the end of the command name. This means that the command is not shown in the help output.

```bash
local -a usage=(
  'command1'  "Description of command1"
  '#command2' "Description of hidden command2"
)
:usage "Brief description of your script" "${@}"
```

:::note
Hidden commands are still available and can be called.
:::

#### Global/Cascading flags

Global flags behave like [arguments](#arguments). They are defined in any of the commands and are available in all child subcommands. They are defined by the `args` array. This works as every subcommand has the scope of the parent command.

```bash
subcommand1() {
  local -a args=(
    'positional' "Description of positional"
    'flag|f'     "Description of flag"
  )
  :args "Brief description of your subcommand" "${@}"
  echo "verbose: ${verbose[*]}"
  echo "subflag: ${subflag:-}"
}

command1() {
  local subflag
  # if you know that command1 will always be called with `args` in its scope, you can leave this out
  declare -p args || local -a args
  args+=(
    'subflag|f' "Description of subflag"
  )
  local -a usage=(
    'subcommand1' "Description of subcommand1"
  )
  :usage "Brief description of your command" "${@}"
  "${usage[@]}"
}

main() {
  local -a verbose args=(
    'verbose|v' "Description of verbose"
  )
  local -a usage=(
    'command1' "Description of command1"
  )
  :usage "Brief description of your script" "${@}"
  "${usage[@]}"
}

[[ "${BASH_SOURCE[0]}" != "${0}" ]] || main "${@}"
```

:::note
We overwritten the `args` array in `subcommand1`. You won't see the `verbose` flag in the help output of `subcommand1`.
But you can still use the `verbose` flag in `subcommand1` as it is in the scope of the parent function.
:::

#### Group commands

You can group commands by adding a `-` to the `usage` array. This will create a new group in the help output.

```bash
local -a usage=(
  'command1' "Description of command1"
  '-'        "Group 1"
  'command2' "Description of command2"
  'command3' "Description of command3"
  '-'        "Group 2"
  'command4' "Description of command4"
)
```

### Arguments

The [:args](../../libraries/args) function is used to define the arguments and flags for a command. It takes a array defining available arguments and flags.

```bash
local arg1 arg2 flag
local -a args=(
  'arg1'    "Description of arg1"
  'arg2'    "Description of arg2"
  'flag|f'  "Description of flag"
)
:args "Brief description of your command" "${@}"

echo "Being here means that the command was called correctly"
echo "arg1: ${arg1}"
echo "arg2: ${arg2}"
echo "flag: ${flag:-}"
```

Note that arguments are positional and flags are not. Flags can be called with a short or long version. Positional arguments are required and flags are optional (if not otherwise defined).

#### Positional arguments

Positional arguments are defined by their name and a description. They are required and positional (as their name suggests). You can define as many positional arguments as you like. You can also define their type (string, number, boolean, ...) and default value.

```bash
local arg1 arg2="default"
local -a args=(
  'arg1:~int' "Description of arg1"
  'arg2'      "Description of arg2"
)
:args "Brief description of your command" "${@}"
```

#### Flags

Flags are defined by their name and a description. They are optional and can be called with a short or long version. You can define as many flags as you like. You can also define their type (string, number, boolean, ...) and default value. Additionally, you can define a flag as a boolean flag, meaning that it doesn't take a value and is either true `1` or false `0`.

```bash
local flag1 flag2="default"
local -a args=(
  'flag1|f:~int' "Description of flag1"
  'flag2|f'      "Description of flag2"
)
:args "Brief description of your command" "${@}"
```

:::note
Short flags are defined with a single character and long flags are always in front of the short flag. The long flag has to correspond to a variable with the same name.
:::

##### long flags

Defined by appending a `|` to the flag name.

```bash
local flag1
local -a args=(
  'flag1|' "Description of flag1"
)
```

##### types

Defined by appending a `:~<type>` to the flag name. The following types are available:

- `string` (default)
- `int`
- `float`
- `boolean`
- `stdin` (reads from stdin if `-` is passed)

##### boolean flags

Boolean flags are defined by appending `:+` to the flag name. This means that the flag doesn't take a value and is either true `1` or false `0`.

```bash
local flag1
local -a args=(
  'flag1|f:+' "Description of flag1"
)
:args "Brief description of your command" "${@}"

(( flag1 )) || echo "flag1 was not set"
```

##### required flags

Flags are optional by default. You can define a flag as required by appending `!` to the end of the flag name.

```bash
local flag1
local -a args=(
  'flag1|f:~int!' "Description of flag1"
)
```

##### multiple flags

You can define a flag as a multiple flag just by defining the reference variable as array. This means that the flag can be called multiple times.

```bash
# can be called like `script -f 1 -f 2 -f 3`
local -a flag1
local -a args=(
  'flag1|f:~int' "Description of flag1"
)
```

You can also define a flag as a multiple flag with a 'no value' by appending `:+`.

```bash
# can be called like `script -vvv`
local -a verbose
local -a args=(
  'verbose|v' "Description of flag1"
)
```

#### Custom types

You can define custom types for your arguments. This is useful if you want to validate the input or transform it. You can define a custom type by defining a function with the name of the type and the signature `to::<type> <value> <field>`. The function should return the transformed value or raise an error (return non zero) if the value is invalid.

```bash 
to::uint() {
  local value="${1}"
  [[ "${value}" =~ ^[0-9]+$ ]] || return 1
  
  echo "${value}"
}
```

#### Group flags

You can group flags by adding a `-` to the `args` array. This will create a new group in the help output.

```bash
local -a args=(
  '-'     "Choose" # this will overwrite the default group name "Options:"
  'flag1' "Description of flag1"
  '-'     "Group 1"
  'flag2' "Description of flag2"
  'flag3' "Description of flag3"
  '-'     "Group 2"
  'flag4' "Description of flag4"
)
```