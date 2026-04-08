# Sync Model

`pyra sync` is Pyra's dependency reconciliation command.

The command follows one permanent pipeline:

1. Read project dependency inputs from `pyproject.toml`.
2. Decide the selected dependency set for this invocation.
3. Check whether `pylock.toml` is still fresh for those inputs.
4. Resolve and rewrite `pylock.toml` if it is missing or stale.
5. Reconcile the centralized environment exactly from the lock.

## Project Inputs

The current sync input model includes:

- Project root and stable project identity.
- Pinned managed interpreter from `[tool.pyra].python`.
- `[project].dependencies`.
- `[project.optional-dependencies]`.
- `[dependency-groups]`, including `{ include-group = "..." }`.
- Whether a build system is present.

Sync fails if the project does not pin a Python version first. `pyra sync`
targets one Pyra-managed interpreter and one current platform in this slice.

## Lock Freshness

`pylock.toml` is considered fresh only when all of the following still match:

- The normalized dependency input fingerprint.
- The selected interpreter version.
- The target triple.
- The index URL.
- The resolution strategy identifier.

Freshness does not mean "latest available upstream". A lock stays fresh until
relevant project or resolution inputs change.

## Resolution Scope

The current resolver computes one union resolution for:

- Base project dependencies.
- All dependency groups.
- All extras.

That union is resolved for the selected interpreter and current platform only.
This keeps the first lock implementation small and coherent, but it means Pyra
may reject projects where separate groups or extras are independently valid but
conflict when solved together.

The current strategy identifier is `current-platform-union-v1`.

## Install Selection

The lock is resolved broadly, but installation is a selected subset.

Selection is evaluated from:

- `default-groups`.
- Explicit group flags.
- Explicit extra flags.
- Exclusions.
- Package markers recorded in the lock.

Current default behavior is:

- Include base dependencies.
- Include the `dev` dependency group if it exists.
- Include no extras.

Exclusions always win over inclusions.

## Reconciliation Rules

Installation is driven from `pylock.toml`, not from re-resolution.

The installer:

- Selects lock entries whose markers match the current `dependency_groups` and
  `extras`.
- Installs missing or changed locked packages.
- Removes packages not present in the selected lock subset.
- Installs the current project editable only when a build system is present.

In this slice, Pyra uses `python -m pip install --no-deps` and
`python -m pip uninstall` behind an installer boundary. pip applies artifacts,
but Pyra owns dependency resolution and desired state.
