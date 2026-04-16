---
title: Getting Started
description: Install Pyra, set up a Python version, and create your first project.
---

This guide walks through the core workflow: installing a Python version, creating a project, syncing dependencies, and running code.

## Install Pyra

Pyra ships as a single binary.

```bash
curl -fsSL https://tlo3.com/pyra-install.sh | sh
```

The install script downloads the correct GitHub Release archive, verifies the
published checksum, and installs `pyra` onto your `PATH`.

If you need a specific release, set `PYRA_VERSION`:

```bash
curl -fsSL https://tlo3.com/pyra-install.sh | env PYRA_VERSION=0.1.0 sh
```

Verify the installation:

```bash
pyra --version
```

Update Pyra later:

```bash
pyra self update
```

## Install a Python version

Pyra manages Python installations from [`python-build-standalone`](https://github.com/indygreg/python-build-standalone). Installed Pythons are stored centrally and shared across projects.

Search for available versions:

```bash
pyra python search 3.13
```

Install one:

```bash
pyra python install 3.13
```

List installed versions:

```bash
pyra python list
```

## Initialize a project

Create a new Python project in the current directory:

```bash
pyra init --python 3.13
```

This creates:

- `pyproject.toml` — project metadata and dependency declarations
- `pylock.toml` — resolved dependency state (initially empty)

The `--python` flag pins a managed Python version in `[tool.pyra].python`.

## Pin a Python version

If you already have a `pyproject.toml`, pin a managed Python version:

```bash
pyra use 3.13
```

This writes the version to `[tool.pyra].python` and refreshes the centralized environment to use the selected interpreter.

A pinned Python version is required before `pyra sync` will run.

## Add dependencies

Add packages to your project:

```bash
pyra add requests
pyra add pytest --group dev
```

Each `add` edits `pyproject.toml` and runs the full sync pipeline — resolve, lock, reconcile.

## Sync the environment

Sync reconciles the centralized environment from `pyproject.toml` and `pylock.toml`:

```bash
pyra sync
```

The sync pipeline:

1. Reads dependency inputs from `pyproject.toml`
2. Checks whether `pylock.toml` is fresh
3. Resolves and rewrites the lock if stale or missing
4. Reconciles the environment exactly from the lock

Sync is idempotent. Running it twice with no changes is a no-op.

### Reproducibility modes

```bash
pyra sync --locked
pyra sync --frozen
```

Both modes fail if the lock is missing or stale, which is useful for CI where you want to catch drift.

## Run code

Execute scripts and commands through the synchronized environment:

```bash
pyra run main.py
pyra run serve
```

`pyra run` ensures the environment is synchronized before execution. Child exit codes are propagated.

## Check project health

Run diagnostics without changing any state:

```bash
pyra doctor
```

Doctor reports actionable findings about interpreter state, lock freshness, and environment drift.

## Next steps

- [Commands Reference](/reference/commands/) — full command documentation
- [Sync Model](/concepts/sync-model/) — how the dependency pipeline works
- [Dependency Selection](/concepts/dependency-selection/) — groups, extras, and selection flags
- [Python Management](/concepts/python-management/) — version management in detail
