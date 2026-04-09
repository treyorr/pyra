# Product Direction

This document exists to keep the project legible as Pyra grows.

Pyra is not just a package installer and not just a runtime experiment.
It is being built in layers, and each layer must stay coherent with the ones
below it.

## What Pyra Is Today

Today, Pyra is primarily:

- a standards-first Python project manager
- a Python package manager built around `pyproject.toml`
- a lock-and-sync system with centralized environments
- a managed Python installation tool

Current model:

1. `pyproject.toml` declares intent
2. `pylock.toml` records resolved state
3. the centralized environment applies that state

Current foundation already exists in code:

- CLI parsing and command dispatch
- app context and Pyra-managed paths
- managed Python installation flows
- project discovery and centralized environment identity
- project input loading from `pyproject.toml`
- resolver integration
- lock writing and lock reuse logic
- exact reconciliation planning
- current `sync` pipeline

What is still incomplete is not the basic shape. It is the hardening of that
shape into something production-grade.

## What Pyra Must Become First

Before Pyra expands into bigger runtime territory, it must become a real
package manager that people can trust.

That means:

- `sync` must be correct
- lock reuse must be truthful
- installs must be hash-verified
- interpreter constraints must be enforced
- `add` and `remove` must mutate declared intent and then reuse sync
- `run` must execute from the synchronized centralized environment

This is phase 1 and phase 2 work in the execution roadmap.

The core idea is:

- Pyra first becomes a strong package and project manager
- then Pyra becomes a strong developer tool
- then Pyra becomes a runtime foundation

## What Pyra Is Not Trying To Be

Pyra is not trying to become a pile of unrelated commands.

It should not split into:

- one system for package management
- another system for task running
- another system for notebooks
- another system for runtime features

Those must all build on the same contracts:

- one interpreter model
- one lock model
- one environment model
- one sync model

If a future feature cannot build on those contracts, it should not be added in
an ad hoc way.

## Near-Term Destination

The near-term destination is a production-grade package manager core.

That means Pyra should be able to do the important day-to-day work that tools
like uv, PDM, and Poetry are expected to handle:

- manage a project Python
- resolve dependencies
- write and reuse a lock file
- install from the lock deterministically
- add and remove dependencies cleanly
- run project commands through the synchronized environment

This is the point where Pyra becomes a credible default tool for normal Python
project work.

## Mid-Term Destination

Once the core package-manager behavior is solid, Pyra should become a broader
developer tool.

That includes:

- fast installs through caching and parallelism
- better conflict and error UX
- more stable resolver behavior on real-world package graphs
- platform-aware and then multi-environment locks

This is the point where Pyra becomes usable across more real teams, CI flows,
and cross-platform projects.

## Long-Term Destination

Once package management is solid and execution is grounded in the same
environment model, Pyra can grow into the broader runtime direction.

Long-term feature areas include:

### Test Runner

A test runner inspired by pytest and its most useful plugin ecosystem, with a
strong default UX around:

- concurrency
- clean output
- sensible defaults
- tight integration with the synchronized project environment

### Tasks And Workflows

Tasks defined in `pyproject.toml`, inspired by tools like taskipy and mise.

These should not be bolt-on shell wrappers. They should be:

- project-aware
- interpreter-aware
- environment-aware
- easy to run through the same execution model as `pyra run`

### Notebooks

Notebook workflows inspired by marimo.

This should eventually build on the same managed interpreter, synchronized
environment, and runtime contract rather than creating a separate notebook-only
environment model.

### Expanded Standard Library

Over time, Pyra may grow a higher-level batteries-included ecosystem around
common tasks such as:

- HTTP client and server utilities
- database access
- common project utilities

This should happen only after the package manager and runtime foundations are
strong, and it should be guided by real community usage rather than early
speculation.

## The Through-Line

The easiest way to lose sight of Pyra is to think of it as a list of future
features.

The clearer way to think about it is:

1. build a trustworthy package manager core
2. build a trustworthy execution model on top of it
3. build developer workflows on top of that
4. build runtime features on top of that

Everything should stack cleanly on the same foundation.
