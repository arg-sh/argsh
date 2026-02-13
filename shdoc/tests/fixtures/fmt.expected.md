This file contains functions for formatting text

## Index

* [fmt::tty](#fmttty)

### fmt::tty

Format text to the width of the terminal

#### Example

```bash
fmt::tty "This is a long line that should be wrapped to the width of the terminal"
fmt::tty < file.txt
cat file.txt | fmt::tty
```

#### Arguments

* **$1** (string|stdin): text to format. If not provided, will read from stdin

#### Output on stdout

* The formatted text

