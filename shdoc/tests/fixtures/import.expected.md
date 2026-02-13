This file contains functions for importing libraries

## Index

* [import](#import)
* [import::clear](#importclear)

### import

Import a library, relative to the current script
If '@' is prepended to the library name, it will be imported from the base path
If '~' is prepended to the library name, it will be imported from the script entry point

#### Example

```bash
import fmt
```

#### Arguments

* **$1** (string): Library name

### import::clear

Clear the import cache

