---
title: Lockfile
description: How pylock.toml works — standards alignment, authoritative data, freshness metadata, and resolution strategy.
---

Pyra writes `pylock.toml` and treats it as the source of truth for installation. The lock captures resolved dependency state so the installer never needs to re-resolve.

## Standards alignment

Pyra writes a PEP 751-shaped lock with the following top-level structure:

```toml
lock-version = "1.0"
created-by = "pyra"
requires-python = ">=3.13"

[[environments]]
id = "cpython-3.13.2-aarch64-apple-darwin"
marker = "..."
interpreter-version = "3.13.2"
target-triple = "aarch64-apple-darwin"

extras = []
dependency-groups = ["pyra-default", "dev"]
default-groups = ["pyra-default", "dev"]

[[packages]]
name = "requests"
version = "2.32.3"
# ...

[tool.pyra]
input-fingerprint = "..."
interpreter-version = "3.13.2"
target-triple = "aarch64-apple-darwin"
index-url = "https://pypi.org/simple/"
resolution-strategy = "environment-scoped-union-v1"
```

Pyra does not currently claim full cross-tool or universal lock compatibility.

## Authoritative vs. informational data

### Authoritative

These fields drive installation decisions:

- `[[packages]]` — selected package entries
- Package markers
- `[[environments]]` — environment identifiers and markers
- Package artifact locations and hashes
- `default-groups`, `dependency-groups`, `extras`

Pyra installs from the lock without re-resolving.

### Informational

`[[packages.dependencies]]` is informational. Pyra records dependency edges for traceability and auditing, but installation does not walk those edges. Selection is driven by markers and the locked package/artifact set.

## `[[environments]]`

Each lock records explicit environment tables:

| Field | Description |
|-------|-------------|
| `id` | Stable environment identifier |
| `marker` | PEP 508 environment marker |
| `interpreter-version` | Concrete Python version |
| `target-triple` | Platform triple |

Single-target locks have one environment entry. Multi-target locks can record one table per resolved target.

## `[tool.pyra]`

Pyra-specific metadata used for lock freshness checks. These fields do not describe installable packages — they describe when Pyra considers the lock reusable.

| Field | Description |
|-------|-------------|
| `input-fingerprint` | Hash of normalized dependency inputs |
| `interpreter-version` | Python version used for resolution |
| `target-triple` | Platform used for resolution |
| `index-url` | Package index used |
| `resolution-strategy` | Strategy identifier (versioned) |

## `pyra-default`

Base project dependencies are represented through the synthetic dependency group `pyra-default`. This group:

- Gives base dependencies an explicit root set in the resolver
- Lets lock selection treat base deps and named groups through one mechanism
- Keeps `default-groups` explicit

`pyra-default` is an internal modeling detail. It appears in lock files but is not a user-facing dependency group.

## Resolution strategies

### `environment-scoped-union-v1`

Current default strategy:

- Resolve for one selected interpreter
- Resolve for one current platform
- Resolve one union of base dependencies, all groups, and all extras
- Record explicit environment IDs and markers
- Use marker-based selection from the unified lock

### `environment-scoped-matrix-v1`

Multi-target strategy:

- Resolve one target environment at a time
- Merge compatible shared package graphs into one lock
- Record one `[[environments]]` table per resolved target
- Accept merged lock only when package graph shape stays identical across targets
- Narrow to current-host artifacts during installation

The strategy identifier is part of lock freshness. Pyra can change strategy without silently trusting old lock assumptions.
