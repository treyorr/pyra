---
title: Sync Model
description: How Pyra's dependency pipeline works — from project inputs to a reconciled environment.
---

`pyra sync` is the central command in Pyra. Every dependency operation — `add`, `remove`, `update`, `run` — ultimately flows through the same sync pipeline.

## The pipeline

```text
pyproject.toml &rarr; selection &rarr; freshness check &rarr; resolve &rarr; lock &rarr; reconcile
```

Five steps, always in this order:

1. **Read project inputs** from `pyproject.toml`
2. **Decide the selected dependency set** from defaults and CLI flags
3. **Check lock freshness** against current inputs
4. **Resolve and rewrite** `pylock.toml` if stale or missing
5. **Reconcile** the centralized environment exactly from the lock

## Step 1: Project inputs

Sync reads:

- `[project].name` — required
- `[project].dependencies` — base deps
- `[project.optional-dependencies]` — extras
- `[dependency-groups]` — groups, with include-group expansion
- `[tool.pyra].python` — required pinned interpreter
- `[project].requires-python` — optional constraint check
- `[build-system]` — determines editable project installation

Sync fails early if the project does not pin a Python version. If `requires-python` is present, the pinned interpreter must satisfy it.

## Step 2: Selection

The selected dependency set determines what gets installed. Default:

- Base dependencies (always)
- `dev` group (if it exists)
- No extras

This can be overridden with `--group`, `--extra`, `--all-groups`, `--all-extras`, `--no-group`, `--no-dev`, `--only-group`, and `--only-dev` flags. See [Dependency Selection](./dependency-selection/) for full rules.

## Step 3: Lock freshness

`pylock.toml` is considered fresh when all of the following match current state:

- Normalized dependency input fingerprint
- Selected interpreter version
- Selected lock target set
- Index URL
- Resolution strategy identifier

Freshness means "lock matches current inputs." It does not mean "latest upstream." A lock stays fresh until project or resolution inputs change.

## Step 4: Resolution

When the lock is stale or missing, Pyra runs fresh resolution:

- One selected interpreter
- One target platform (current host by default)
- One union of base dependencies, all groups, and all extras
- PubGrub-based solver behind Pyra-specific abstractions
- Metadata from PyPI Simple Repository API

The result is written to `pylock.toml`.

Current strategy identifier: `environment-scoped-union-v1`

:::note
Union resolution means Pyra solves all scopes together. Independently valid groups or extras may fail if they conflict when combined. This is a [known tradeoff](../roadmap/#known-tradeoffs).
:::

## Step 5: Reconciliation

Installation is driven from `pylock.toml`, not from re-resolution. The installer:

- Selects lock entries matching the current dependency groups and extras
- Narrows multi-target artifacts to the current host
- Installs missing or changed packages
- Removes packages not in the selected lock subset
- Installs the current project editable when `[build-system]` is present

Reconciliation is exact by default: the environment contains precisely what the selected lock subset specifies.

## Reproducibility modes

### Default (`pyra sync`)

Resolves when lock is stale or missing. Writes the updated lock. Reconciles the environment.

### `--locked`

Requires an existing fresh `pylock.toml`. Never resolves. Never rewrites. Fails if the lock is missing or stale.

Use for CI to verify that committed lock files match project inputs.

### `--frozen`

Same guarantees as `--locked`. Requires an existing fresh `pylock.toml`. Never resolves or rewrites.

## Idempotency

Sync is idempotent when the lock is fresh and the environment is already reconciled. Running `pyra sync` twice with no changes is a no-op.

## Why sync is central

Every mutation command flows through sync:

- `pyra add` edits `pyproject.toml`, then syncs
- `pyra remove` edits `pyproject.toml`, then syncs
- `pyra run` ensures sync before execution
- `pyra update` writes a fresh lock, but does not bypass the lock/install split

This keeps one pipeline instead of multiple competing installation paths. The lock is always the source of truth for what gets installed.
