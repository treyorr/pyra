# Resolution Scope

This document describes what Pyra's resolver currently supports and what it does
not promise yet.

Pyra should grow toward a gold-standard Python package manager, but that only
works if current guarantees are precise.

## Current Resolution Target

This slice resolves for:

- one selected Pyra-managed interpreter
- one current platform
- one current target triple

Pyra does not currently produce one universal multi-platform lock.

## Current Resolution Strategy

Current strategy identifier:

- `current-platform-union-v1`

Current strategy behavior:

- resolve base dependencies
- resolve all dependency groups
- resolve all extras
- solve one combined dependency graph
- record root-set membership so installation can later select the appropriate subset

This strategy intentionally favors one coherent implementation over premature
multi-resolution complexity.

## Current Tradeoff

The main tradeoff of union resolution is:

- separately valid groups or extras may still fail together if they require
  incompatible versions when combined

This is stricter than the ideal long-term model, but it keeps the first lock and
selection system manageable.

## Package Metadata Sources

Current metadata sources:

- PyPI-style Simple Repository API responses
- distribution core metadata exposed by the index
- PEP 508 requirements
- PEP 440 version constraints

Current implementation uses PubGrub behind Pyra-specific abstractions.

## Marker Handling

Current marker handling supports the needs of the first sync slice:

- interpreter and platform marker evaluation during resolution
- root-set membership markers for `dependency_groups`
- root-set membership markers for `extras`

Pyra records enough marker information in the lock to select install subsets
without re-resolving.

## Artifact Scope

Current artifact support is intentionally narrow:

- prefer wheels when available
- fall back to source distributions when needed by the current model
- record artifact URLs and hashes in the lock

This is good enough to establish the lock/install contract while keeping the
installer boundary small.

## Non-Goals For This Slice

Pyra does not currently promise:

- universal multi-platform locks
- full forked marker partitioning across incompatible optional scopes
- exhaustive wheel tag support across every Python packaging edge case
- a pip-free native installer

Those are future improvements, not current guarantees.

## Long-Term Direction

The resolver should evolve without breaking the core pipeline:

project inputs -> resolve -> lock -> reconcile

As the model expands, the lock freshness strategy and the documented resolution
strategy identifier should change explicitly rather than silently broadening
assumptions.
