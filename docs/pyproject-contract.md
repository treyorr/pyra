# `pyproject.toml` Contract

This document defines what Pyra currently reads, writes, and owns in
`pyproject.toml`.

Pyra should behave like a standards-first Python project manager. That means it
should preserve standard packaging metadata whenever possible and keep Pyra-
specific state narrow and explicit.

## Ownership Boundaries

Pyra treats `pyproject.toml` as a mix of:

- standard Python packaging metadata
- Pyra-specific project management metadata

Current ownership split:

- `[project]` is standard metadata owned by the project.
- `[build-system]` is standard metadata owned by the project.
- `[tool.pyra]` is Pyra-managed metadata.

Pyra should not casually invent new top-level tables for behavior that already
fits existing standards.

## Current Inputs Pyra Reads

Pyra currently reads:

- `[project].name`
- `[project].dependencies`
- `[project].optional-dependencies`
- `[project].scripts`
- `[project].requires-python`
- `[build-system]` presence
- `[dependency-groups]`
- `[tool.pyra].python`

## Current Inputs Pyra Writes

Pyra currently writes:

- `[tool.pyra].python`

Current `pyra init` also creates a baseline `pyproject.toml`, but ongoing Pyra
management should stay narrow and avoid rewriting unrelated metadata.

## Required Versus Optional Fields

Current command requirements:

- `pyra use` requires a discoverable `pyproject.toml`.
- `pyra sync` requires `[tool.pyra].python`.
- `pyra sync` requires `[project].name`.
- `pyra sync` can operate without `[project].requires-python`, but the current
  product direction is that pinned interpreter and project Python constraints
  should converge over time. When `[project].requires-python` is present, the
  selected managed interpreter must satisfy it or sync fails before lock reuse
  and resolution.
- `pyra sync` supports empty dependency lists.
- `pyra sync` treats `[build-system]` as the switch for editable project
  installation.
- `pyra run` reuses the same sync requirements, then looks up targets through
  `[project].scripts`, installed console scripts, and `.py` file fallback.

## `[tool.pyra]`

`[tool.pyra]` exists to hold Pyra-specific project state that should not be
confused with packaging metadata.

Current field:

- `python`

Current meaning:

- This is the project's selected Pyra-managed interpreter request.
- It is the contract between project state and environment/lock workflows.
- It is required for `pyra sync` in the current model.

Pyra should keep this table small and intentional.

## Dependency Declarations

Pyra treats the following sources as authoritative project dependency inputs:

- `[project].dependencies`
- `[project].optional-dependencies]`
- `[dependency-groups]`

The current sync flow does not derive dependency intent from environment state
or from the lock.

## Validation Rules

Current validation includes:

- dependency requirements must be valid PEP 508 requirement strings
- dependency group names are normalized for comparison
- duplicate group names after normalization are rejected
- `{ include-group = "..." }` is supported
- include cycles are rejected
- missing included groups are rejected
- repeated requirements introduced by includes are preserved

## Long-Term Direction

This contract is meant to support a gold-standard toolchain foundation:

- package management through sync and lock
- project mutation commands that update inputs, then sync
- execution commands that trust the synchronized environment
- a future runtime layer that can rely on the same project identity,
  interpreter, environment, and lock contracts

That means `pyproject.toml` should remain the source of declared intent, while
`pylock.toml` remains the source of realized install state.
