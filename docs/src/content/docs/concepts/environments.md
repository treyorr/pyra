---
title: Environments
description: How Pyra manages centralized project environments and why they live outside the project tree.
---

Pyra uses centralized environments instead of per-project `.venv` directories. This is a foundational design decision, not an implementation convenience.

## Why centralized?

Pyra stores environments outside the project tree so that:

- **Project checkouts stay clean** — no `.venv` to gitignore, no stale environments after branch switches
- **Environment ownership is explicit** — Pyra manages the environment lifecycle, not the user
- **Stable identity** — the same project always maps to the same environment location
- **Future-proof** — runtime and execution features can depend on a consistent environment contract

The environment is Pyra-managed state, not an incidental local folder.

## Project identity

Each environment is keyed by a stable project identity derived from the canonical project root path.

- The same project reached through different path spellings maps to one identity
- The environment location is stable for that canonical root
- Metadata can reference the project by identity rather than by working directory

## Environment metadata

Pyra records metadata for each environment:

- Project identity
- Project root path
- Pinned Python selector
- Concrete interpreter version
- Interpreter path
- Environment path
- Creation and update timestamps

This metadata is the contract between project discovery, sync, and execution.

## Lifecycle

| Command | Effect |
|---------|--------|
| `pyra init` | Creates the centralized environment |
| `pyra use` | Repins the interpreter and refreshes the environment |
| `pyra sync` | Ensures the environment exists, then reconciles package state |

Sync prefers reusing the existing environment path and reconciling install state. It does not silently replace the environment on every run.

## Exact reconciliation

Sync is exact by default:

- Install missing locked packages
- Replace changed locked packages
- Remove packages not present in the selected lock subset

The environment contains precisely what the lock specifies — nothing more, nothing less.

## Project installation

The current project is treated separately from third-party packages:

- If `[build-system]` exists in `pyproject.toml`, install the project editable after dependency sync
- If no build system exists, skip project installation cleanly

This keeps the environment useful for both distributable packages and runnable projects.

## Why not `.venv`?

Traditional `.venv` directories in the project root have several problems:

- They bloat the project tree
- They create confusion when multiple tools (pip, poetry, pipenv) compete over `.venv`
- They couple environment state to the checkout, creating stale state after branch switches
- They make environment identity implicit

Pyra's centralized model makes the environment a first-class managed resource with explicit identity, owned lifecycle, and deterministic state.

## Storage location

Environments are stored under Pyra's data directory:

- macOS: `~/Library/Application Support/pyra/environments/`
- Linux: `~/.local/share/pyra/environments/`

Each environment is keyed by the canonical project identity.
