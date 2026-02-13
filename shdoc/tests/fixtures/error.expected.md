This file contains functions for error handling

## Index

* [error::stacktrace](#errorstacktrace)

### error::stacktrace

Print a stacktrace

#### Example

```bash
trap "error::stacktrace" EXIT
```

#### Arguments

* **$1** (int): The exit code

#### Exit codes

* **0**: Always

