This file contains functions for working with arguments

## Index

* [:args](#args)

### :args

This function will parse the command line arguments and set the variables
for the user to use. It will also print the usage information if the user
passes the `-h` or `--help` flag.

#### Example

```bash
local arg flag flag1 flag2 flag3 flag4 flag5 flag6="default"
local -a args
args=(
  'arg'             "positional argument"       # required
  'flag|'           "flag with value"
  'flag1|f'         "flag with value and short"
  'flag2|l:+'       "flag without value"
  'flag3|a:!'       "required flag"             # required
  'flag4|b:~float'  "flag with type"
  'flag5|c:~int!'   "required flag with type"   # required
  'flag6|d'         "flag with default"
)
:args "Title" "${@}"
```

#### Arguments

* **$1** (string): The title of the usage
* **...** (array): User arguments

#### Variables set

* **args** (array): [get] The arguments to parse

#### Exit codes

* **0**: If user arguments are correct
* **2**: If user arguments are incorrect

