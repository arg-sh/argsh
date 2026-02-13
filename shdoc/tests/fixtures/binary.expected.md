This file contains functions for working with binaries

## Index

* [binary::exists](#binaryexists)
* [binary::github](#binarygithub)
* [binary::arch](#binaryarch)
* [binary::jq](#binaryjq)
* [binary::vale](#binaryvale)

### binary::exists

Check if a binary exists

#### Example

```bash
binary::exists "curl" # succeeds (returns 0)
binary::exists "zcurl" # fails (returns 1)
```

#### Arguments

* **$1** (string): binary name

#### Exit codes

* **0**: If the binary exists
* **1**: If the binary does not exist

#### Output on stderr

* The binary is required to run this script

### binary::github

Download a binary from github

#### Example

```bash
# https://github.com/cli/cli/releases/download/v2.45.0/gh_2.45.0_linux_amd64.tar.gz
latest="$(github::latest "cli/cli")"
binary::github "./bin/gh" "cli/cli" "${latest}/gh_${latest:1}_$(uname -s)_$(uname -m).tar.gz" "gh_${latest:-1}_$(uname -s)_$(uname -m)/bin/gh"
```

#### Arguments

* **$1** (string): path to binary
* **$2** (string): GitHub repository
* **$3** (string): file to download
* **$4** (string): [opt] tar file to extract

### binary::arch

Get the architecture of the system

#### Example

```bash
binary::arch # amd64
```

#### Output on stdout

* The architecture of the system

### binary::jq

Download the jq binary into $PATH_BIN if it does not exist

#### Example

```bash
  binary::jq
https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64
```

### binary::vale

Download the vale binary into $PATH_BIN if it does not exist

#### Example

```bash
  binary::vale
https://github.com/errata-ai/vale/releases/download/v2.28.0/vale_2.28.0_Linux_64-bit.tar.gz
```

