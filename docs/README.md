# Pyra Docs

This directory defines the architectural contracts for Pyra.

Pyra is being built as a long-term Python package manager, project manager, and
eventual runtime foundation. The goal is not just to support today's CLI
commands. The goal is to establish a clean, durable model that later commands
and a future runtime can build on without rewriting core behavior.

Contributors and coding agents should read this directory before changing
behavioral or architectural code.

## Read Order

Start here for general orientation:

1. `docs/README.md`
2. `docs/pyproject-contract.md`
3. `docs/environment-model.md`
4. `docs/sync-model.md`

Then read the more specific docs that apply to the task:

- `docs/group-semantics.md`
- `docs/lock-format-notes.md`
- `docs/lock-command-model.md`
- `docs/doctor-model.md`
- `docs/resolution-scope.md`
- `docs/installer-boundary.md`
- `docs/add-remove-model.md`
- `docs/run-model.md`
- `docs/error-model.md`
- `docs/command-contracts.md`
- `docs/testing-strategy.md`
- `docs/product-direction.md`
- `docs/execution-roadmap.md`

## What Each Doc Answers

### Product and input contracts

- `docs/pyproject-contract.md`
  What Pyra reads and owns in `pyproject.toml`.
- `docs/group-semantics.md`
  How base dependencies, groups, and extras behave.

### Environment and sync contracts

- `docs/environment-model.md`
  What a centralized environment is and how Pyra manages it.
- `docs/sync-model.md`
  The end-to-end dependency pipeline for `pyra sync`.
- `docs/lock-format-notes.md`
  What Pyra means by `pylock.toml` today.
- `docs/lock-command-model.md`
  What `pyra lock` guarantees and how lock freshness messaging works.
- `docs/doctor-model.md`
  What `pyra doctor` checks, how findings are reported, and why it stays read-only.
- `docs/package-cache-model.md`
  How Pyra reuses verified package artifacts without changing lock authority.
- `docs/resolution-scope.md`
  What the resolver currently supports and what it intentionally does not.
- `docs/installer-boundary.md`
  What the installer owns versus what the resolver and lock own.

### Future command contracts

- `docs/add-remove-model.md`
  How future dependency mutation commands should fit the sync pipeline.
- `docs/run-model.md`
  How future execution should build on the synchronized environment model.

### Product direction

- `docs/product-direction.md`
  The high-level picture of what Pyra is now, what it must become next, and how
  longer-term runtime features should build on the package-manager foundation.

### Quality and maintainership

- `docs/error-model.md`
  How Pyra should communicate failures.
- `docs/command-contracts.md`
  Shared human/JSON command envelopes and exit-code category mapping.
- `docs/testing-strategy.md`
  How Pyra should test package management behavior safely and deterministically.
- `docs/execution-roadmap.md`
  The implementation roadmap, milestone order, and stable task IDs for
  execution planning.

## Documentation Rules

When behavior changes:

- update the relevant doc in the same change
- keep docs explicit about what is guaranteed versus temporary
- prefer short contract docs over broad aspirational prose
- document both current behavior and intended direction when the distinction matters

If code and docs disagree, fix the mismatch intentionally. Do not leave drift in
place.
