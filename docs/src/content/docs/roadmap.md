---
title: Roadmap
description: Current state, known tradeoffs, and near-term priorities for Pyra development.
---

Pyra is in active development. This page documents where things stand, what tradeoffs exist by design, and what comes next.

## Current state

Pyra is a working Python package and project manager with:

- Full Python version management lifecycle
- Project initialization and pinning
- Dependency resolution via PubGrub
- PEP 751-shaped lock file generation
- Centralized environment reconciliation
- Dependency add/remove with automatic sync
- Script and command execution
- Health diagnostics and upgrade reporting
- Machine-readable JSON output
- `--locked` / `--frozen` reproducibility modes

The core pipeline — `pyproject.toml` &rarr; resolve &rarr; `pylock.toml` &rarr; reconcile — is functional and tested.

## Known tradeoffs

These are intentional design boundaries in the current implementation, not bugs:

### Current-platform lock only

Lock files target the current platform and interpreter. Pyra does not produce universal cross-platform locks. Multi-target generation exists (`environment-scoped-matrix-v1`) but is intentionally narrow — it requires identical package graph shapes across targets.

### Union resolution

Resolution solves one union of base dependencies, all groups, and all extras. This means independently valid but mutually incompatible groups or extras may fail when combined. This is stricter than the ideal model but keeps the resolver and lock simple.

### Installer backend

Pyra uses `pip` behind a boundary (`--no-deps` install, no resolution allowed). This is intentional — pip applies artifacts, Pyra owns resolution and desired state. A native installer is a future improvement.

### Resolver scope

The resolver currently supports:

- PyPI Simple Repository API metadata
- PEP 508 requirements and PEP 440 versions
- Wheel preference with sdist fallback

It does not yet promise:

- Full forked marker partitioning across incompatible scopes
- Exhaustive wheel tag coverage for every edge case
- Universal multi-platform resolution

### Network performance

`pyra add` works but network operations can currently take 20+ seconds in some cases, depending on index response times and metadata fetching.

### Verbosity

`-v` increases output verbosity. `-vv` and `-vvv` are accepted but currently plateau at the same level as `-v`.

## Near-term priorities

The next layer of work focuses on making Pyra a credible daily-driver package manager:

1. **Performance** — faster resolution and index metadata caching
2. **Resolver hardening** — better conflict diagnostics and edge case handling
3. **Cache improvements** — artifact caching and reuse across syncs
4. **Broader wheel support** — more complete platform tag matching
5. **Error UX polish** — clearer guidance in complex failure scenarios

## Mid-term direction

Once the core package management is solid:

- Fast installs through parallelism and caching
- Stable task execution workflows
- More robust cross-platform support
- CI/CD integration patterns

## Long-term vision

Pyra is designed to grow into a broader Python developer tool:

- Task runner integrated with the project model
- Test runner with strong defaults
- Notebook workflows
- All built on the same interpreter, environment, and lock contracts

The key constraint: every new feature must build on the existing sync pipeline and environment model. No parallel systems.

## What Pyra is not trying to be

Pyra is not trying to become a pile of unrelated commands. It should not split into separate systems for package management, task running, notebooks, and runtime features. Those must all build on the same contracts — one interpreter model, one lock model, one environment model, one sync model.
