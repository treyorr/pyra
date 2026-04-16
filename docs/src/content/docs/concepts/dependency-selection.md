---
title: Dependency Selection
description: How Pyra selects base dependencies, groups, and extras for resolution and installation.
---

Pyra supports three dependency scopes from `pyproject.toml` and provides granular control over which scopes are included during sync.

## Dependency scopes

### Base dependencies

Declared in `[project].dependencies`:

```toml
[project]
dependencies = [
    "requests>=2.31",
    "rich>=13",
]
```

Base dependencies are always included by default. Internally, Pyra represents them through a synthetic group called `pyra-default`. This is an implementation detail — you never interact with `pyra-default` directly.

### Dependency groups

Declared in `[dependency-groups]`:

```toml
[dependency-groups]
dev = ["pytest>=8", "mypy>=1.11"]
docs = ["sphinx>=7"]
```

Groups model development, testing, documentation, or other workflow dependencies that are separate from your package's runtime requirements.

### Optional dependencies / extras

Declared in `[project.optional-dependencies]`:

```toml
[project.optional-dependencies]
performance = ["uvloop>=0.19"]
socks = ["httpx[socks]"]
```

Extras represent optional runtime capabilities that consumers of your package can opt into.

## Default selection

When you run `pyra sync` without flags, Pyra selects:

- **Included:** Base dependencies (`pyra-default`)
- **Included:** `dev` group (if it exists)
- **Excluded:** Apps and extras
- **Excluded:** Other groups

## Selection flags

### Including more

| Flag | Effect |
|------|--------|
| `--group <name>` | Add a group to the default selection |
| `--extra <name>` | Add an extra |
| `--all-groups` | Add all groups |
| `--all-extras` | Add all extras |

### Excluding

| Flag | Effect |
|------|--------|
| `--no-group <name>` | Remove a group after inclusions |
| `--no-dev` | Remove `dev` after inclusions |

### Only mode

| Flag | Effect |
|------|--------|
| `--only-group <name>` | Select only named groups, exclude base dependencies |
| `--only-dev` | Select only `dev`, exclude base dependencies |

## Precedence rules

**Exclusions always win over inclusions.**

If you write `--all-groups --no-group docs`, the `docs` group is excluded even though `--all-groups` nominally includes it.

`--only-group` and `--only-dev` are stronger exclusions — they remove base dependencies entirely and select only the specified groups.

## Group name normalization

Group and extra names are compared using normalized form:

- Case-insensitive
- `-`, `_`, and `.` all normalize to `-`
- Original names are preserved for display

Pyra rejects duplicate group names after normalization. If your `pyproject.toml` declares both `my-group` and `my_group`, Pyra will refuse to proceed.

## Include-group support

`[dependency-groups]` supports `{ include-group = "..." }` directives:

```toml
[dependency-groups]
dev = ["pytest>=8", { include-group = "typing" }]
typing = ["mypy>=1.11", "pyright>=1.1"]
```

Rules:

- Includes are expanded eagerly into one requirement list per group
- Include cycles are rejected
- Missing include targets are rejected
- Repeated requirements from includes are preserved (no silent deduplication)

## Duplicate declarations

The same package can appear in multiple scopes:

- In base dependencies and a group
- In multiple groups
- In extras and groups

This is valid. Resolution solves one union of all scopes, and installation uses markers to select the appropriate subset.

## Union resolution tradeoff

Pyra currently resolves all scopes (base, all groups, all extras) as one combined graph. This means:

- Groups and extras that are independently valid may still fail if they require incompatible versions when solved together
- This is stricter than the ideal model, but keeps the resolver and lock simple

See [Sync Model](/concepts/sync-model/) for how this fits into the full pipeline.
