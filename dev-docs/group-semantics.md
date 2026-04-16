# Group Semantics

This document defines how `pyra sync` interprets base dependencies, dependency
groups, and extras.

## Scopes

Pyra currently recognizes three dependency scopes:

- Base dependencies from `[project].dependencies`.
- Optional dependencies from `[project.optional-dependencies]`.
- Dependency groups from `[dependency-groups]`.

Base dependencies are represented internally through the synthetic dependency
group name `pyra-default`. That synthetic group is not user-facing.

## Defaults

Default sync selection is:

- Include `pyra-default`.
- Include `dev` if the project defines a `dev` dependency group.
- Include no extras.

The lock records these defaults in `default-groups`.

## Group Names

Dependency group names follow normalized comparison rules:

- Comparison is case-insensitive.
- `-`, `_`, and `.` normalize to `-`.
- Original names are preserved where possible for display.

Pyra rejects duplicate group names after normalization.

## Include Groups

`[dependency-groups]` supports `{ include-group = "..." }`.

Current behavior:

- Includes are expanded eagerly into one requirement list per group.
- Include cycles are rejected.
- Missing include targets are rejected.
- Repeated requirements introduced by includes are preserved.

Pyra does not silently deduplicate included requirements.

## Extras

Extras come from `[project.optional-dependencies]`.

Current behavior:

- Extras are off by default.
- `--extra <name>` includes one extra.
- `--all-extras` includes all extras.
- Extras are normalized with the same comparison rules used for groups.

## Selection Flags

Current sync flags behave as follows:

- `--group <name>` adds a group to the default selection.
- `--extra <name>` adds an extra to the default selection.
- `--all-groups` adds all groups to the default selection.
- `--all-extras` adds all extras to the selection.
- `--no-group <name>` removes a group after inclusions are applied.
- `--no-dev` removes `dev` after inclusions are applied.
- `--only-group <name>` selects only the named groups and excludes base
  dependencies.
- `--only-dev` selects only `dev` and excludes base dependencies.

Exclusions take precedence over inclusions.

## Duplicate Logical Declarations

Pyra currently preserves repeated logical package declarations across scopes.

That means the same package may appear:

- In base dependencies and a group.
- In multiple groups.
- In extras and groups.

Resolution currently solves one union of all scopes, then installation uses
markers to choose the selected subset from that unified result.
