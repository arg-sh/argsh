This file contains functions for working with Docker

## Index

* [docker::user](#dockeruser)

### docker::user

Prepare a Docker container for running as current or specified user.
This function creates a temporary passwd and group file to run the container.
Env $PATH_BASE is mounted to the container's $HOME.

#### Example

```bash
# docker::user "$(id -u)" "$(id -g)" "$(whoami)" "/workspace" "/bin/sh"
flags=$(docker::user)
docker run ${flags} image
```

#### Arguments

* **$1** (string): user id
* **$2** (string): group id
* **$3** (string): user name
* **$4** (string): home directory
* **$5** (string): shell

#### Output on stdout

* The Docker run options for running as the specified user

