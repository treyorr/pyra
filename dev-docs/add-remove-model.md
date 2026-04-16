# Add And Remove Model

`pyra add` and `pyra remove` should be thin project mutation commands built on
top of `pyra sync`.

They should not invent new dependency installation paths.

## Core Rule

The permanent dependency flow is:

1. mutate declared project inputs
2. run sync
3. trust the resulting lock and environment state

That means:

- `add` edits `pyproject.toml`, then syncs
- `remove` edits `pyproject.toml`, then syncs

The commands should never bypass the lock pipeline and install directly.

## `pyra add`

`pyra add` should eventually:

- parse requested requirements
- choose the target scope
- update the correct declaration in `pyproject.toml`
- preserve formatting where practical
- run the normal sync pipeline

Expected scopes:

- base dependencies
- named dependency groups
- optional dependencies / extras where appropriate

## `pyra remove`

`pyra remove` should eventually:

- identify the target declaration scope
- remove matching requirements from `pyproject.toml`
- fail clearly if the requested dependency is not declared in the selected scope
- run the normal sync pipeline

## Scope Rules

When these commands are implemented, they should follow the same semantic rules
already documented for sync:

- base dependencies are distinct from dependency groups
- extras are distinct from dependency groups
- normalized group naming rules still apply
- future UX should not blur "dev group" and "extra" into one concept

## Why This Matters

If `add` and `remove` directly manipulate the environment, Pyra would quickly
split into multiple competing models:

- declared dependencies
- locked dependencies
- installed dependencies

That would break the long-term path to a reliable package manager and runtime
foundation.

## Long-Term Direction

The right shape is:

- `pyproject.toml` stores declared intent
- `pylock.toml` stores realized dependency state
- the centralized environment stores applied state

`add` and `remove` should only mutate declared intent and then hand off to sync.
