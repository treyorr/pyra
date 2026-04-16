---
title: Status
description: Current implementation status — what works, what's known, and how stable the surface is.
---

This page reflects the verified implementation status of Pyra as of the latest test pass.

## Working

All items below have been tested and verified:

### CLI foundation

- All subcommands accept `--help` and exit `0`
- `--version` / `-V` prints version
- `--json` produces deterministic JSON envelopes
- Exit code categories are stable (`0`, `2`, `3`, `4`, pass-through)

### Python management

- `pyra python list` — human and JSON output
- `pyra python search` — returns available versions
- `pyra python install` — downloads and installs
- `pyra python uninstall` — removes managed installs

### Project lifecycle

- `pyra init` creates `pyproject.toml` and `pylock.toml`
- `pyra use` updates pinned Python version in `pyproject.toml`
- Project directory detection works via parent lookup

### Dependency management

- `pyra add` — adds to base, groups, and extras
- `pyra remove` — removes from base, groups, and extras
- Sync pipeline runs after add/remove

### Sync

- `pyra sync` — full pipeline works
- `pyra sync --locked` — rejects when lock is missing
- `pyra sync --frozen` — rejects when lock is missing
- Sync is idempotent when lock is fresh
- Lock lifecycle transitions work
- Locked mode preserves lock file hash
- Failed operations do not mutate files

### Lock

- Lock freshness checks work
- Corrupt lock handling works
- Lock writes are atomic (write to temp, then rename)

### Execution

- `pyra run` executes inside the centralized environment
- Exit codes propagate correctly
- Script lookup order: `[project.scripts]` &rarr; console scripts &rarr; `.py` fallback

### Diagnostics

- `pyra doctor` — healthy/unhealthy reporting works
- `pyra outdated` — reports newer available versions
- `pyra update` — refreshes lock deterministically
- `pyra self update` — updates the installed CLI from GitHub Releases

### Error handling

- Errors include actionable remediation steps
- Error categories (user/system/internal) map to correct exit codes

## Known issues

| Area | Issue | Severity |
|------|-------|----------|
| `pyra add` | Network operations can take 20+ seconds | Moderate — functional but slow |
| Verbosity | `-v`/`-vv`/`-vvv` plateau after first level | Low — cosmetic |

## Stability assessment

The current command surface is functional and stable for normal project workflows. The core pipeline — init, add, sync, run — works end-to-end.

Areas that will improve but are usable today:

- Resolution speed (currently bounded by index metadata fetching)
- Wheel compatibility breadth
- Conflict error messaging
- Verbose output depth
