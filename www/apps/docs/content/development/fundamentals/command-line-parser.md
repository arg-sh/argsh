---
description: ''
---

# Command Line Parser

In this document, you’ll learn how to use the command line parser to parse command line arguments in Argsh.

## Overview

The implementation of the command line parser in argsh is inspired by the [cobra](https://github.com/spf13/cobra) library for Go. 

- Easy subcommand-based CLIs: app server, app fetch, etc.
- Fully POSIX-compliant flags (including short & long versions)
- Nested subcommands
- 🚧 (not fully implemented) Global, local and cascading flags
- 🚧 (not yet) Intelligent suggestions (app srver... did you mean app server?)
- Automatic help generation for commands and flags
- Grouping help for subcommands
- Automatic help flag recognition of -h, --help, etc.
- 🚧 (not yet) Automatically generated shell autocomplete for your application (bash, zsh, fish, powershell)
- 🚧 (not yet) Automatically generated man pages for your application
- 🚧 (kindof) Command aliases so you can change things without breaking them
- 🚧 (not yet) The flexibility to define your own help, usage, etc.

## Usage

Argsh provides a simple way to define commands and flags. It consists of two parts, a root command `:usage` and arguments `:args`.

::: warning
argsh is still in development and prune to change.
:::

### Setup

To use the command line parser, you need to include the `argsh` library in your script or install it. Read more about how to set up argsh in the [getting started guide](/getting-started).

::: note
If you are using argsh in the shebang line, your script will be executed within a function scope. So wouldn't need to implement a `main` function and can use `local` variables directly.
:::

### Root command

The root command is defined by the [:usage](../../libraries/args.usage.mdx) function. It takes a array defining available commands and flags.

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

### Arguments

The `:args` function is used to define the arguments and flags for a command. It takes a array defining available arguments and flags.

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

::: note
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