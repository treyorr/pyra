# Environment Model

Pyra uses centralized project environments.

That decision is foundational. It is not an implementation convenience. It is a
core part of Pyra's long-term mental model.

## Why Centralized Environments Exist

Pyra stores environments outside the project tree so that:

- project checkouts stay clean
- environment ownership is explicit
- multiple commands can reuse one stable location
- future runtime and execution behavior can rely on a consistent environment identity

The environment is part of Pyra-managed state, not an incidental local folder.

## Project Identity

Each project environment is keyed by a stable project identity derived from the
canonical project root path.

Current implications:

- the same project reached through different path spellings maps to one identity
- the environment location is stable for that canonical root
- metadata can refer to the project by identity rather than by temporary cwd

## Current Environment Metadata

Pyra currently stores environment metadata that records:

- project identity
- project root
- pinned Python selector
- concrete interpreter version
- interpreter path
- environment path
- creation and update timestamps

That metadata is part of the contract between project discovery, sync, and
future run behavior.

## Environment Lifecycle

Current lifecycle rules:

- `pyra init` prepares the centralized environment
- `pyra use` repins the interpreter and refreshes the environment
- `pyra sync` ensures the environment exists, then reconciles package state

Sync should prefer reusing the environment path and reconciling install state.
It should not silently replace the environment on every run.

## Exact Reconciliation

Pyra sync is exact by default.

Current meaning:

- install missing locked packages
- replace changed locked packages
- remove packages not present in the selected lock subset

Protected bootstrap tools may remain outside Pyra-managed package selection as
needed, but exactness should stay the default mental model.

## Project Installation

The current project is treated separately from third-party locked packages.

Current behavior:

- if `[build-system]` exists, install the project editable after dependency sync
- if no build system exists, skip project installation cleanly

This keeps the environment useful for both packages and runnable projects
without pretending every project is buildable.

## Long-Term Direction

The centralized environment model should eventually support:

- `pyra run`
- script and entrypoint execution
- automatic sync before execution
- a future runtime layer that can depend on a stable interpreter plus environment contract

That is why environment identity, metadata, and exact reconciliation behavior
must stay explicit now.
