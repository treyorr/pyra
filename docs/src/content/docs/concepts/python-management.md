---
title: Python Management
description: How Pyra manages Python installations, version matching, and project pinning.
---

Pyra manages Python installations independently from system Python. Installed versions come from [`python-build-standalone`](https://github.com/indygreg/python-build-standalone) and are stored in a centralized location shared across all projects.

## Managed Python storage

Pyra stores Python installations in its data directory under a platform-appropriate path:

- macOS: `~/Library/Application Support/pyra/pythons/`
- Linux: `~/.local/share/pyra/pythons/`

Each installed version gets its own directory. Multiple versions can coexist.

## Version requests

Version selectors are partial and resolve to the best matching installed or available release:

| Input | Matches |
|-------|---------|
| `3` | Latest `3.x.y` |
| `3.13` | Latest `3.13.y` |
| `3.13.2` | Exact `3.13.2` |

This applies to `pyra python install`, `pyra python search`, and `pyra use`.

## Installing Python versions

```bash
# Search for available versions
pyra python search 3.13

# Install
pyra python install 3.13

# List installed
pyra python list
```

`pyra python install` downloads the archive for the current host platform, extracts it, and registers it in Pyra's Python store.

## Project pinning

Projects pin a managed Python version in `pyproject.toml`:

```toml
[tool.pyra]
python = "3.13"
```

This pin tells Pyra which managed interpreter to use for resolution, locking, and environment creation.

### Setting the pin

```bash
# During project init
pyra init --python 3.13

# Or on an existing project
pyra use 3.13
```

`pyra use` resolves the version request against installed Pythons, updates `[tool.pyra].python`, and refreshes the centralized environment.

### Pin requirements

- `pyra sync` requires a pinned Python version. Sync fails before any resolution if `[tool.pyra].python` is missing.
- When `[project].requires-python` is present, the pinned interpreter must satisfy that range or sync fails.

## `pyra python use`

`pyra use` is a top-level command (not under `pyra python`) that:

1. Resolves the version request against installed Pythons
2. Writes the resolved version to `[tool.pyra].python`
3. Refreshes the centralized environment to use the new interpreter

It requires a discoverable `pyproject.toml`, which Pyra finds by searching from the current directory upward.

## Uninstalling

```bash
pyra python uninstall 3.13.2
```

Removes the managed installation from Pyra's Python store. Does not affect projects that have already synced with that version — the lock and environment remain until the next sync.
