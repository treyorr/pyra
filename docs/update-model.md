# Update Command Model

This document defines the contract for `pyra update`.

`pyra update` refreshes lock state to newer versions allowed by the current
dependency specifiers, without mutating declared project intent.

## Purpose

`pyra update` answers:

- what lockfile changes are required to move to the latest resolvable versions
  under current dependency constraints
- what package-level differences those lock changes introduce

It is a lock lifecycle command, not a project-intent mutation command.

## Intent And Environment Boundaries

`pyra update` must not mutate:

- `pyproject.toml` dependency declarations
- centralized environment package state
- environment metadata

`pyra update` may mutate:

- `pylock.toml` (unless run in dry-run mode)

This is the command-level difference from future upgrade-style intent mutation:
`update` refreshes resolved lock state only. It does not widen, narrow, add, or
remove declared dependency specifiers.

## Resolution Model

`pyra update` uses the same interpreter, target-selection, index, and resolver
model as `pyra lock` and `pyra sync`, then resolves a refreshed lock from
current project inputs.

Unlike lock freshness reuse commands, `update` always performs a fresh
resolution for the current inputs so it can pick up newer allowed versions.

## Dry-Run Mode

`pyra update --dry-run` resolves the refreshed lock and reports a package-level
summary of what would change, but does not write `pylock.toml`.

Dry-run output should make it clear that lock changes were planned only and not
applied.

## Output Contract

`pyra update` follows the shared command contract:

- human mode reports lock refresh status and package-level change summary
- `--json` returns the same status and change payload through the shared
  envelope model

When no package versions change, `pyra update` still succeeds and reports that
the lock refresh was already current for existing intent.
