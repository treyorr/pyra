# Run Model

`pyra run` is not implemented yet, but its behavior should be constrained now so
it builds on the existing dependency and environment model rather than creating a
second execution system.

## Core Rule

`pyra run` should trust the synchronized centralized environment.

Its permanent mental model should be:

1. discover project
2. ensure the environment is synchronized
3. execute within that environment

That means `run` should depend on sync semantics, not duplicate them.

## Expected Guarantees

When implemented, `pyra run` should:

- use the project's selected Pyra-managed interpreter
- use the centralized environment for that project identity
- ensure dependency state matches the lock before execution
- present a simple, predictable command model

## Lookup Order

Current intended lookup order remains:

1. `[project.scripts]`
2. console scripts from installed packages
3. `.py` file fallback

That order should be documented and preserved explicitly.

## Relationship To Sync

`pyra run` should not:

- resolve dependencies itself
- install ad hoc packages
- create a separate temporary environment model

It should reuse:

- project discovery
- interpreter selection
- lock freshness rules
- centralized environment reconciliation

## Why This Matters For Runtime Work

Pyra is intended to grow beyond a project manager into a runtime-capable tool.

That future only stays coherent if execution builds on stable contracts:

- one interpreter model
- one environment model
- one dependency lock model

If `run` deviates from sync now, a future runtime layer would inherit a split
system instead of a solid foundation.

## Long-Term Direction

The ideal end state is:

- package management and execution share the same synchronized environment
- execution is automatic and predictable
- runtime features build on that same contract instead of replacing it
