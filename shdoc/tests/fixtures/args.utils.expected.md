This file contains functions for working with arguments

## Index

* [args::run](#argsrun)

### args::run

Run a function if a flag is set

#### Example

```bash
args:run "" 1 test::argsh 1 test::docs
```

#### Arguments

* **$1** (any): if empty will run all functions
* **$2** (boolean): run the following function
* **$3** (string): the function name to run

