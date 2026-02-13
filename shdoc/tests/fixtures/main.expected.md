This file contains the main function for running a bash script

## Index

* [argsh::builtins](#argshbuiltins)

### argsh::builtins

Manage argsh native builtins (.so).

#### Example

```bash
argsh builtins           # show current status
argsh builtins install   # download if not present
argsh builtins update    # re-download latest
```

#### Arguments

* **$1** (string): Subcommand: install, update, or empty for status

