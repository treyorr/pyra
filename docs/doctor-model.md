# Doctor Command Model

This document defines the contract for `pyra doctor`.

`pyra doctor` is a read-only diagnostics command that explains project and
environment health without changing lock or environment state.

## Purpose

`pyra doctor` helps answer:

- is the pinned interpreter usable for this project?
- is `pylock.toml` present and fresh for current inputs?
- does the centralized environment match the selected lock state?

## Read-Only Guarantee

`pyra doctor` must not mutate:

- `pyproject.toml`
- `pylock.toml`
- centralized environment package state
- environment metadata

It can read all of those inputs to produce diagnostics.

## Current Diagnostics

`pyra doctor` currently reports actionable findings for:

- interpreter mismatch:
  pinned interpreter is missing, incompatible with `requires-python`, or
  inconsistent with environment metadata
- missing lock:
  `pylock.toml` does not exist
- stale lock:
  lock freshness no longer matches current project inputs, interpreter, target,
  index, or strategy
- environment drift:
  centralized environment metadata/path is missing or unreadable, or installed
  packages differ from the selected lock state

`doctor` can report multiple findings in one run.

## Output Contract

`pyra doctor` follows the shared command contract:

- default human-readable output prints warning findings with actionable next steps
- global `--json` returns the shared command envelope with the same findings in
  machine-readable form

When findings exist, the command is warning-class output (status `warn`) with
exit code `0`. Unexpected command failures still use the normal error envelope
and non-zero exit categories.
