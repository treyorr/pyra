# Lock Format Notes

Pyra writes `pylock.toml` and treats it as the source of truth for installation.

This document describes which parts are standards-aligned, which parts are
Pyra-specific, and which fields are authoritative.

## Standards Alignment

Pyra writes a PEP 751-shaped lock with the following top-level structure:

- `lock-version = "1.0"`
- `created-by = "pyra"`
- `requires-python`
- `[[environments]]`
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
- Environment identifiers and markers in `[[environments]]`.
- Package artifact locations and hashes.
- Top-level lock metadata used for selection, such as `default-groups`,
  `dependency-groups`, and `extras`.

Pyra installs from the lock without re-resolving.

## Informational Data

`[[packages.dependencies]]` is informational and auditing-focused.

Pyra records dependency edges there for traceability, but current installation
logic does not walk those edges. Selection is driven by markers and the locked
package/artifact set.

## `[[environments]]`

Pyra now writes explicit environment tables instead of a bare string array.

Current fields are:

- `id`
- `marker`
- `interpreter-version`
- `target-triple`

Why this exists:

- It gives each target environment a stable identifier before package entries
  start carrying environment membership.
- It keeps the lock schema honest about the difference between one host marker
  and one named environment slice.
- It lets later multi-target work extend the environment metadata without
  redesigning the top-level lock shape again.

Single-target locks still record one environment slice. Multi-target locks can
now record one table per resolved target environment with its own interpreter
version and target triple.

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

## `environment-scoped-union-v1`

The current resolution strategy identifier means:

- Resolve for one selected interpreter.
- Resolve for one current platform.
- Resolve one union of base dependencies, all groups, and all extras.
- Record explicit environment ids and markers in the lock schema.
- Use marker-based selection from that unified lock.

This identifier is part of lock freshness so Pyra can change strategy later
without silently trusting old lock assumptions.

## `environment-scoped-matrix-v1`

This strategy identifier means:

- resolve one target environment at a time
- merge the compatible shared package graph into one lock
- record one `[[environments]]` table per resolved target

Current multi-target generation is intentionally narrow:

- per-target environment metadata is recorded explicitly
- the merged lock is accepted only when package graph shape stays identical
  across targets
- current-host install selection narrows the lock to host-compatible artifacts
- target configuration remains future work
