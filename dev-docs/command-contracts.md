# Command Contracts

This document defines the shared command result and error contract introduced in
M1.1.

The goal is one stable model for:

- default human-readable terminal output
- machine-readable JSON output for automation and CI
- consistent exit-code categories

## Output Modes

Pyra now supports two output modes for major commands:

- default human-readable mode
- `--json` machine-readable mode

`--json` is a global flag and applies to every command invocation.

## Shared Result Status

Every command result is categorized as:

- `success`
- `warn`
- `fail`

Rules:

- `success`: command finished with exit code `0` and no warning blocks in output
- `warn`: command finished with exit code `0` and includes warning output
- `fail`: command finished with a non-zero exit code or returned an error

## JSON Envelope

Machine-readable output uses one envelope shape:

- `status`: `success | warn | fail`
- `exit`:
  - `code`: integer process exit code
  - `category`: `success | user | system | internal | external`
- `output`: command output tree or `null` for failures without normal output
- `error`: error payload or `null` for successful command paths

Error payload shape:

- `summary`
- `detail`
- `suggestion`

## Exit Code Categories

Error and process outcomes map to explicit categories:

- `success` -> `0`
- `user` -> `2`
- `system` -> `3`
- `internal` -> `4`
- `external` -> pass-through process code for externally executed commands

`pyra run` keeps its existing guarantee that child exit codes are returned.
