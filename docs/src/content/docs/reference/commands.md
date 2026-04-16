---
title: Commands Reference
description: Complete reference for all Pyra commands, flags, and behavior.
---

All commands support `--help` for inline documentation and `--json` for machine-readable output.

## Global flags

| Flag | Description |
|------|-------------|
| `--json` | Emit machine-readable JSON envelopes instead of human output |
| `-v` | Increase verbosity |
| `--version` / `-V` | Print version and exit |
| `--help` / `-h` | Print help |

---

## `pyra python`

Manage Pyra-owned Python installations from `python-build-standalone`.

### `pyra python list`

List Python versions currently installed by Pyra.

```bash
pyra python list
pyra python list --json
```

Supports both human-readable and JSON output.

### `pyra python search [version]`

Search for installable Python versions for the current host platform.

```bash
pyra python search
pyra python search 3.13
```

The optional version selector filters results. Accepts partial versions like `3`, `3.13`, or `3.13.2`.

### `pyra python install <version>`

Download and install a managed Python version.

```bash
pyra python install 3.13
pyra python install 3.13.2
```

Pyra resolves the version request to the best matching release from `python-build-standalone`, downloads the archive, and installs it to Pyra's centralized Python store.

Version requests can be partial: `3.13` resolves to the latest `3.13.x` patch.

### `pyra python uninstall <version>`

Remove a managed Python installation.

```bash
pyra python uninstall 3.13.2
```

---

## `pyra self update`

Update the installed Pyra binary from GitHub Releases.

```bash
pyra self update
```

This command updates the CLI itself. It does not touch your project,
dependencies, lockfile, or environment state.

Pyra keeps this separate from `pyra update`, which still means "refresh project
lock state within existing dependency intent."

---

## `pyra init`

Initialize a new Python project in the current directory.

```bash
pyra init
pyra init --python 3.13
```

Creates:
- `pyproject.toml` with baseline project metadata
- `pylock.toml` (empty initial lock)

| Flag | Description |
|------|-------------|
| `--python <version>` | Pin a managed Python version in `[tool.pyra].python` |

If `--python` is provided, Pyra verifies the version is installed before writing it.

---

## `pyra use <version>`

Pin a managed Python version for the current project.

```bash
pyra use 3.13
```

Writes the version to `[tool.pyra].python` in `pyproject.toml` and refreshes the centralized environment to use the selected interpreter.

Requires a discoverable `pyproject.toml` (searches parent directories).

---

## `pyra add <requirement>`

Add a dependency to `pyproject.toml` and sync.

```bash
pyra add requests
pyra add "httpx>=0.27"
pyra add pytest --group dev
pyra add uvloop --extra performance
```

| Flag | Description |
|------|-------------|
| `--group <name>` | Add to a named dependency group |
| `--extra <name>` | Add to a named optional dependency set |

The requirement must be a valid PEP 508 string. After editing `pyproject.toml`, Pyra runs the full sync pipeline.

`--group` and `--extra` are mutually exclusive. Without either flag, the dependency is added to base `[project].dependencies`.

:::note
Network operations during `add` can currently take 20+ seconds depending on index response times. This is a known area for improvement.
:::

---

## `pyra remove <package>`

Remove a dependency from `pyproject.toml` and sync.

```bash
pyra remove requests
pyra remove pytest --group dev
pyra remove uvloop --extra performance
```

| Flag | Description |
|------|-------------|
| `--group <name>` | Remove from a named dependency group |
| `--extra <name>` | Remove from a named optional dependency set |

Fails clearly if the package is not declared in the selected scope. After editing `pyproject.toml`, Pyra runs the full sync pipeline.

---

## `pyra sync`

Reconcile the centralized environment from `pyproject.toml` and `pylock.toml`.

```bash
pyra sync
pyra sync --locked
pyra sync --frozen
```

### Pipeline

1. Read dependency inputs from `pyproject.toml`
2. Decide the selected dependency set
3. Check lock freshness
4. Resolve and rewrite `pylock.toml` if stale or missing
5. Reconcile the environment exactly from the lock

### Flags

| Flag | Description |
|------|-------------|
| `--locked` | Require an existing fresh lock. Never resolve or rewrite. |
| `--frozen` | Require an existing fresh lock. Never resolve or rewrite. |
| `--target <triple>` | Add a lock-generation target triple for this invocation only |
| `--group <name>` | Include a dependency group in addition to defaults |
| `--extra <name>` | Include an extra |
| `--all-groups` | Include all dependency groups |
| `--all-extras` | Include all extras |
| `--no-group <name>` | Exclude a group after inclusions |
| `--no-dev` | Exclude the `dev` group |
| `--only-group <name>` | Select only named groups, exclude base dependencies |
| `--only-dev` | Select only `dev`, exclude base dependencies |

### Default selection

- Include base dependencies
- Include `dev` group if it exists
- No extras

### Precedence

Exclusions always win over inclusions. `--only-group` and `--only-dev` exclude base dependencies entirely.

### Reconciliation

Installation is exact: install missing, replace changed, remove unselected packages. If `[build-system]` exists, the current project is installed editable after dependency sync.

---

## `pyra lock`

Generate or refresh `pylock.toml` without reconciling the environment.

```bash
pyra lock
pyra lock --target aarch64-apple-darwin
```

| Flag | Description |
|------|-------------|
| `--target <triple>` | Add a lock-generation target triple for this invocation only |

Useful when you want to update the lock without installing anything — for example, before committing or in CI validation steps.

---

## `pyra run <target> [args...]`

Execute a command through the synchronized centralized environment.

```bash
pyra run main.py
pyra run serve
pyra run main.py -- --port 8080
```

### Lookup order

1. `[project.scripts]` entries in `pyproject.toml`
2. Console scripts from installed packages
3. `.py` file fallback

### Behavior

- Ensures the environment is synchronized before execution
- Uses the project's managed interpreter
- Forwards remaining CLI arguments to the child process
- Propagates the child process exit code

`pip install` and `pip uninstall` attempts inside `pyra run` fail fast with guidance to use `pyra add`/`pyra remove` instead.

---

## `pyra doctor`

Read-only diagnostics for project and environment health.

```bash
pyra doctor
pyra doctor --json
```

Reports findings for:

- **Interpreter mismatch** — pinned interpreter missing, incompatible with `requires-python`, or inconsistent with environment
- **Missing lock** — `pylock.toml` does not exist
- **Stale lock** — lock freshness does not match current inputs
- **Environment drift** — installed packages differ from the selected lock state

Doctor never mutates `pyproject.toml`, `pylock.toml`, or the environment. See [Doctor Reference](/reference/doctor/) for details.

---

## `pyra outdated`

Report packages with newer available versions.

```bash
pyra outdated
pyra outdated --json
```

Compares locked versions against the configured package index. Requires an existing fresh lock.

Read-only — does not change the lock, manifest, or environment.

---

## `pyra update`

Refresh lock state to newer versions allowed by current specifiers.

```bash
pyra update
pyra update --dry-run
```

| Flag | Description |
|------|-------------|
| `--dry-run` | Show what would change without writing `pylock.toml` |

Always performs a fresh resolution. Does not change `pyproject.toml` — only the lock.

When no versions change, reports that the lock is already current.

---

## Exit codes

| Code | Category | Meaning |
|------|----------|---------|
| `0` | `success` | Command completed successfully |
| `2` | `user` | User/input error |
| `3` | `system` | IO or system error |
| `4` | `internal` | Internal invariant violation |
| pass-through | `external` | Child process exit code (`pyra run`) |
