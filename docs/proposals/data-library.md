# Proposal: Structured Data Library for argsh

## Problem

Bash scripts working with YAML/JSON resort to verbose, repetitive `yq`/`jq` one-liners. A typical config-heavy script has 30+ individual `yq` calls, each spawning a subprocess for a single field. This is:
- **Slow**: N subprocesses for N fields
- **Repetitive**: same `yq -r '.path // ""'` pattern everywhere
- **Error-prone**: null handling (`"null"` string vs empty), quoting, type mismatches
- **Hard to read**: complex jq filters inline in bash

## Design

Unified API for structured data — no yaml/json distinction (yq handles both).

### Core: `data::get` — read from file

Overloaded: `var=.path` assigns to variable, bare `.path` outputs to stdout.
Mixable per-argument. Single `yq` call for all fields.

```bash
# All to variables (batch read — replaces 30+ yq calls)
local domain cidr namespace
data::get "${cluster_yaml}" \
  domain=.spec.cluster.domain \
  cidr=.spec.network.cidr \
  namespace='.spec.cluster.namespace // "default"'

# Single value to stdout (for piping / subshells)
echo "Version: $(data::get "${cluster_yaml}" .spec.kubernetes.version)"

# Multiple to stdout (one per line)
data::get "${cluster_yaml}" .spec.version .spec.foo .spec.bar

# Mixed: some to variables, some to stdout
local d
data::get "${cluster_yaml}" .spec.version d=.spec.cluster.domain
# stdout: v1.28
# $d = "prod.example.com"
```

Rule: has `=` → variable (nameref), no `=` → stdout. Per-argument, mixable.

### `data::set` — batch write from variables into file

Handles all bash variable types automatically:

```bash
# Scalar → string
domain="prod.example.com"

# Indexed array → YAML/JSON array
local -a cidr=(23.232.2.1 232.22.2.3)

# Associative array → YAML/JSON object
local -A labels=(["env"]="prod" ["team"]="infra")

data::set "${cluster_yaml}" \
  .spec.cluster.domain=domain \
  .spec.network.cidr=cidr \
  .metadata.labels=labels
```

Produces:

```yaml
spec:
  cluster:
    domain: "prod.example.com"
  network:
    cidr:
      - 23.232.2.1
      - 232.22.2.3
metadata:
  labels:
    env: prod
    team: infra
```

Type detection uses `is::array` and `declare -p` to distinguish scalar, indexed array (`-a`), and associative array (`-A`).

Direction: `.path=varname` — path is on the left (the target), var is on the right (the source).

Alternative syntax (reversed, matching `data::get` style):

```bash
data::set "${cluster_yaml}" \
  domain=.spec.cluster.domain \
  cidr=.spec.network.cidr
```

### `data::each` — array iteration

Works with files, stdin, and process substitution. Single yq/jq call under the hood.

```bash
# Iterate a YAML file
while data::each "${cluster_yaml}" '.spec.nodes.extraMounts[]' \
  host=.hostPath container=.containerPath readonly='.readOnly // false'
do
  echo "${host} -> ${container}"
done

# Iterate kubectl JSON output (process substitution)
while data::each <(kubectl get pods -l app=nginx -o json) \
  '.items[]' name=.metadata.name node=.spec.nodeName phase=.status.phase
do
  echo "${name} on ${node}: ${phase}"
done

# Iterate from stdin (pipe)
curl -s https://api.github.com/repos/arg-sh/argsh/releases | \
  data::each - '.[]' tag=.tag_name date=.published_at
do
  echo "${tag} released ${date}"
done
```

#### Nested arrays (matrix iteration)

Multiple `[]` in the path flattens into a single stream. Use `^` prefix to access parent fields:

```bash
# For each pod, iterate its containers — access parent pod name via ^
while data::each <(kubectl get pods -o json) \
  '.items[].spec.containers[]' \
  pod='^.metadata.name' container=.name image=.image
do
  echo "${pod}/${container}: ${image}"
done

# Nested arrays in config files
while data::each "${f}" '.clusters[].nodes[]' \
  cluster='^.name' node=.hostname role=.role
do
  echo "${cluster}/${node} (${role})"
done
```

`^` means "from the parent element" (one `[]` level up). Multiple `^` for deeper nesting: `^^.field` = grandparent.

For v1, nested loops are the explicit fallback:

```bash
while data::each "${f}" '.items[]' \
  pod=.metadata.name containers='.spec.containers | @json'
do
  while data::each <(echo "${containers}") '.[]' \
    name=.name image=.image
  do
    echo "${pod}/${name}: ${image}"
  done
done
```

#### Replaces the common anti-pattern

```bash
# Before: custom-columns + awk + empty-line checks
kubectl get pods --no-headers -o custom-columns=":metadata.name,:spec.nodeName" | \
  while read -r line; do
    pod=$(echo "$line" | awk '{print $1}')
    node=$(echo "$line" | awk '{print $2}')
    ...
  done

# After: structured JSON, named fields
while data::each <(kubectl get pods -o json) '.items[]' \
  pod=.metadata.name node=.spec.nodeName
do
  ...
done
```

### Append via `data::set` — `[]` suffix

No separate `data::append` needed. `data::set` with `[]` in the path appends:

```bash
# Append an object to an array (associative array → JSON object)
local -A new_entry=(["name"]="${name}" ["ip"]="${ip}" ["type"]="mirror")
data::set "${json_file}" '.registries[]=new_entry'

# Append a scalar
local new_ip="10.0.0.2"
data::set "${f}" '.spec.dns.servers[]=new_ip'

# Append multiple values from indexed array
local -a extra_ips=("10.0.0.3" "10.0.0.4")
data::set "${f}" '.spec.dns.servers[]=extra_ips'
```

Convention: `.path` → overwrite, `.path[]` → append.

### `data::merge` — deep merge files

```bash
data::merge base.yaml overlay.yaml > merged.yaml
```

### Length, keys, and expressions — just use yq syntax

No special prefixes. The path is a yq expression — whatever yq supports works:

```bash
# Length
count=$(data::get "${f}" '.spec.nodes.extraMounts | length')
data::get "${f}" count='.spec.nodes.extraMounts | length'

# Keys
data::get "${f}" '.spec | keys'
data::get "${f}" fields='.spec | keys'

# Any yq expression
data::get "${f}" ips='.spec.nodes[].ip'
data::get "${f}" first='.spec.nodes | first | .name'

# Mixed with regular fields
data::get "${f}" \
  count='.spec.nodes.extraMounts | length' \
  domain=.spec.cluster.domain \
  fields='.spec | keys'
```

No custom DSL — full yq power, zero learning curve beyond yq itself.

### `data::render` — template with data context

```bash
data::render template.yaml "${cluster_yaml}"
# envsubst-like but reads vars from the data file
```

## Naming rationale

| Function | Direction | Mnemonic |
|----------|-----------|----------|
| `data::get` | file → vars | "get data from file into my variables" |
| `data::set` | vars → file | "set data in file from my variables" |
| `data::val` | file → stdout | "get a single value" |
| `data::each` | file → loop | "iterate each element" |
| `data::set .path[]` | values → file (append) | "append to array in file" |

## Implementation notes

- **Pure bash wrappers** over `yq` (which handles both YAML and JSON)
- **Null → empty**: all getters return `""` for null/missing (no `"null"` strings)
- **Batch reads**: `data::get` generates a single `yq` expression that outputs all fields, parsed via `read`
- **No new dependencies**: `yq` is already required by argsh
- **`:args` integration**: `data::get` uses the same nameref pattern as `:args` for variable binding
- **Builtin candidate**: hot-path functions (`data::get`, `data::set`) could be Rust builtins for performance

## Implementation: bash vs Rust builtin

| | Pure bash + yq | Rust builtin |
|---|---|---|
| .so size increase | +0K (current 531K) | +~400K (~900K total) |
| External deps | yq (~10MB) must be installed | None — self-contained |
| Speed | subprocess per `data::get` call | In-process, ~100x faster |
| Portability | yq version differences | Consistent across systems |
| Effort | Low (bash wrappers) | Medium (serde_json + serde_yaml) |

Measured: a minimal cdylib with `serde_json` + `serde_yaml` (LTO, strip, opt-size) is **448K**.
For comparison: `yq` binary is ~10MB, `jq` is ~1.5MB.

## Priority

| Function | Impact | Effort | Priority |
|----------|--------|--------|----------|
| `data::get` | Eliminates 30+ yq calls per script | Medium | P0 |
| `data::set` | Write vars back to structured files | Medium | P1 |
| `data::each` | Eliminates N×M loop pattern | Medium | P1 |
| `data::set .[]` append | Append to arrays via `[]` suffix | — | Part of `data::set` |
| `| length`, `| keys` | Length/keys via yq expressions | — | Part of `data::get` |
| `data::merge` | Deep merge (yq native) | Low | P2 |
| `data::render` | Template rendering | Medium | P2 |
| Rust builtin | Performance for heavy use | High | P3 |

## Real-world before/after

### Before (lok8s config.sh — 8 yq calls)
```bash
LOK8S_CP_COUNT=$(yq -r '.spec.nodes.controlPlane // 1' "${cluster_yaml}")
LOK8S_WORKER_COUNT=$(yq -r '.spec.nodes.workers // 0' "${cluster_yaml}")
LOK8S_HOST_PORTS=$(yq -r '.spec.nodes.hostPorts' "${cluster_yaml}")
LOK8S_EXTRA_MOUNTS_COUNT=$(yq -r '.spec.nodes.extraMounts | length' "${cluster_yaml}")
K8S_VERSION=$(yq -r '.spec.kubernetes.version' "${cluster_yaml}")
CLUSTER_NAME=$(yq -r '.metadata.name' "${cluster_yaml}")
CLUSTER_DOMAIN=$(yq -r '.spec.cluster.domain' "${cluster_yaml}")
CLUSTER_NAMESPACE=$(yq -r '.spec.cluster.namespace // "default"' "${cluster_yaml}")
```

### After (1 yq call)
```bash
local LOK8S_CP_COUNT LOK8S_WORKER_COUNT LOK8S_HOST_PORTS LOK8S_EXTRA_MOUNTS_COUNT
local K8S_VERSION CLUSTER_NAME CLUSTER_DOMAIN CLUSTER_NAMESPACE
data::get "${cluster_yaml}" \
  LOK8S_CP_COUNT='.spec.nodes.controlPlane // 1' \
  LOK8S_WORKER_COUNT='.spec.nodes.workers // 0' \
  LOK8S_HOST_PORTS='.spec.nodes.hostPorts // ""' \
  LOK8S_EXTRA_MOUNTS_COUNT='.spec.nodes.extraMounts | length' \
  K8S_VERSION=.spec.kubernetes.version \
  CLUSTER_NAME=.metadata.name \
  CLUSTER_DOMAIN=.spec.cluster.domain \
  CLUSTER_NAMESPACE='.spec.cluster.namespace // "default"'
```

### Before (lok8s render.sh — N×3 yq calls in loop)
```bash
for (( m = 0; m < LOK8S_EXTRA_MOUNTS_COUNT; m++ )); do
  em_host=$(yq -r ".spec.nodes.extraMounts[${m}].hostPath" "${cluster_yaml}")
  em_container=$(yq -r ".spec.nodes.extraMounts[${m}].containerPath" "${cluster_yaml}")
  em_readonly=$(yq -r ".spec.nodes.extraMounts[${m}].readOnly // false" "${cluster_yaml}")
  # ...
done
```

### After (1 yq call total)
```bash
data::each "${cluster_yaml}" '.spec.nodes.extraMounts[]' \
  em_host=.hostPath em_container=.containerPath em_readonly='.readOnly // false'
do
  # $em_host, $em_container, $em_readonly available here
done
```

### Before (write back)
```bash
yq -i ".spec.cluster.domain = \"${domain}\"" "${cluster_yaml}"
yq -i ".spec.network.cidr = \"${cidr}\"" "${cluster_yaml}"
```

### After (1 yq call)
```bash
data::set "${cluster_yaml}" \
  .spec.cluster.domain=domain \
  .spec.network.cidr=cidr
```

## Distribution

Could be bundled in argsh core, or distributed as a plugin via OCI registries.
See separate proposal: `docs/proposals/plugin-system.md`

## Open questions

1. Stdin support for `data::get`: `-` or `/dev/stdin`?
2. Implementation: pure bash + yq first, Rust builtin later? Or Rust from the start?

## Related

- Issue #115: Alpine Docker image
- Issue #117: Tracking issue
- Libraries: `libraries/data.sh` (new)
- Builtin: `builtin/src/data.rs` (future P3)
