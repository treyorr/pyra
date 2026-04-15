# Outdated Command Model

This document defines the contract for `pyra outdated`.

`pyra outdated` is a read-only reporting command. It surfaces package-level
upgrade opportunities without changing lock, manifest, or environment state.

## Purpose

`pyra outdated` answers:

- which declared packages in the current project lock have newer available
  versions
- what the currently locked version is
- what newer version is available while preserving current dependency intent

## Read-Only Guarantee

`pyra outdated` must not mutate:

- `pyproject.toml`
- `pylock.toml`
- centralized environment package state
- environment metadata

It may read all of those inputs to compute the report.

## Inputs And Comparison Model

`pyra outdated` reads:

- current project dependency intent from `pyproject.toml`
- currently locked package versions from `pylock.toml`
- currently available versions from the configured package index

The command compares package versions for declared dependency names only. It
uses the same interpreter and index context as lock/sync resolution.

`pyra outdated` requires an existing fresh lock:

- missing lock -> command fails with an actionable user error
- stale lock -> command fails with an actionable user error

This keeps comparison results aligned with current project inputs.

## Output Contract

`pyra outdated` follows the shared command contract:

- default human-readable mode prints a package-level report with
  `current -> latest` versions
- global `--json` returns the shared command envelope with the same package
  items for CI/dashboard use

When outdated packages are found, the command uses warning-class output (status
`warn`) with exit code `0`. Unexpected command failures still use the normal
error envelope and non-zero exit categories.
