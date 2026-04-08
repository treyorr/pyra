# Lock Format Notes

Pyra writes `pylock.toml` and treats it as the source of truth for installation.

This document describes which parts are standards-aligned, which parts are
Pyra-specific, and which fields are authoritative.

## Standards Alignment

Pyra writes a PEP 751-shaped lock with the following top-level structure:

- `lock-version = "1.0"`
- `created-by = "pyra"`
- `requires-python`
- `environments`
- `extras`
- `dependency-groups`
- `default-groups`
- `[[packages]]`

This slice is intentionally conservative. Pyra does not currently claim full
cross-tool or full-universal lock compatibility.

## Authoritative Data

The authoritative installation inputs are:

- The selected package entries in `[[packages]]`.
- Package markers.
- Package artifact locations and hashes.
- Top-level lock metadata used for selection, such as `default-groups`,
  `dependency-groups`, and `extras`.

Pyra installs from the lock without re-resolving.

## Informational Data

`[[packages.dependencies]]` is informational and auditing-focused.

Pyra records dependency edges there for traceability, but current installation
logic does not walk those edges. Selection is driven by markers and the locked
package/artifact set.

## `tool.pyra`

`[tool.pyra]` is Pyra-specific metadata used for lock freshness checks.

Current fields are:

- `input-fingerprint`
- `interpreter-version`
- `target-triple`
- `index-url`
- `resolution-strategy`

These fields do not describe installable packages. They describe the conditions
under which Pyra considers the lock reusable.

## `pyra-default`

Base project dependencies are represented internally as the synthetic dependency
group `pyra-default`.

Why this exists:

- It gives base dependencies one explicit root set in the resolver.
- It lets lock selection treat base dependencies and named groups through one
  shared mechanism.
- It keeps `default-groups` explicit.

`pyra-default` is an internal modeling detail and should not be exposed as a
user-facing dependency group.

## `current-platform-union-v1`

The current resolution strategy identifier means:

- Resolve for one selected interpreter.
- Resolve for one current platform.
- Resolve one union of base dependencies, all groups, and all extras.
- Use marker-based selection from that unified lock.

This identifier is part of lock freshness so Pyra can change strategy later
without silently trusting old lock assumptions.
