This file contains functions around bash

## Index

* [bash::version](#bashversion)

### bash::version

Verify the version of bash

#### Example

```bash
bash::version 4 3 0 # succeeds (returns 0)
```

#### Arguments

* **$1** (int): major version
* **$2** (int): minor version
* **$3** (int): patch version

#### Exit codes

* **0**: If the version is greater than or equal to the specified version
* **1**: If the version is less than the specified version

