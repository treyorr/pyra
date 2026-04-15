# Product Direction

This document exists to keep the project legible as Pyra grows.

Pyra is not just a package installer and not just a runtime experiment. It is
being built in layers, and each layer must stay coherent with the ones below
it.

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
- `sync`, `add`, `remove`, `run`, and locked/frozen sync

What is still incomplete is not the basic shape. It is the hardening of that
shape into a safer daily driver and the addition of the next package-manager
commands that people expect from a uv-class tool.

## What Pyra Must Become First

Before Pyra expands into broader workflow or runtime territory, it must become
a package manager that people can trust for day-to-day project work.

That means:

- `doctor` must explain broken project or environment states clearly
- `lock` must be explicit rather than only a sync side effect
- `outdated` must report upgrade opportunities without mutating state
- `update` must refresh resolved state without changing declared intent
- command contracts must be stable enough for CI and automation

The rest of the command surface should be demand-gated.

That means Pyra should defer broader command families until users clearly ask
for them and the lean core is stable in CI.

This is the next layer of the execution roadmap.

The core idea is:

- Pyra first becomes a strong package and project manager
- then Pyra becomes a strong developer tool
- then Pyra becomes a workflow and runtime platform

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

The near-term destination is a production-grade package manager with strong
diagnostics and artifact workflows.

That means Pyra should be able to do the important day-to-day work that tools
like uv, PDM, and Poetry are expected to handle:

- diagnose project and environment problems
- manage lock lifecycle explicitly and validate lock health in CI
- inspect dependency freshness without mutation
- refresh dependency resolution safely
- manage dependencies cleanly through declared intent, lock, and sync
- run project commands through the centralized synchronized environment

Additional command families should be promoted in small tranches after clear
user demand, not shipped all at once.

This is the point where Pyra becomes a credible default tool for normal Python
project work.

## Delivery Model Going Forward

Pyra should continue using milestone-based execution roadmaps with stable task
IDs, explicit acceptance criteria, and explicit test requirements.

That is not process overhead. It is how Pyra protects architecture while moving
quickly with coding agents.

The expected sequence for new initiatives is:

1. write an idea brief
2. generate an execution roadmap
3. execute one task at a time
4. verify contract and integration behavior
5. run release and open-source readiness gates before announcement

Demand-driven expansion remains the rule. New command families should be
promoted only when real user demand is visible and the current command
contracts remain stable in CI.

## Mid-Term Destination

Once the core package-manager behavior is solid, Pyra should become a broader
developer tool.

That includes:

- fast installs through caching and parallelism
- better conflict and error UX
- more stable resolver behavior on real-world package graphs
- stable task execution and test-running workflows built on the same model

This is the point where Pyra becomes usable across more real teams, CI flows,
and cross-platform projects.

## Long-Term Destination

Once package management is solid and execution is grounded in the same
environment model, Pyra can grow into the broader runtime direction.

Long-term feature areas include:

### Tasks And Workflows

Tasks defined in `pyproject.toml`, inspired by tools like taskipy and mise.

These should not be bolt-on shell wrappers. They should be:

- project-aware
- interpreter-aware
- environment-aware
- easy to run through the same execution model as `pyra run`

### Test Runner

A test runner inspired by pytest and its most useful plugin ecosystem, with a
strong default UX around:

- concurrency
- clean output
- sensible defaults
- tight integration with the synchronized project environment

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
2. build a trustworthy execution and diagnostics model on top of it
3. build developer workflows on top of that
4. build runtime features on top of that

Everything should stack cleanly on the same foundation.
