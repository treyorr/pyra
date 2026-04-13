# Installer Boundary

Pyra keeps dependency management split across three responsibilities:

- Resolver
- Lock
- Installer

That boundary must stay sharp as `add`, `remove`, and `run` build on top of
sync.

## Resolver Owns

The resolver owns:

- Reading index metadata.
- Interpreting dependency specifiers and markers during resolution.
- Choosing compatible package versions.
- Producing typed resolved package results.

The resolver does not:

- Read `pyproject.toml` directly.
- Render terminal output.
- Install packages.
- Decide lock freshness.

## Lock Owns

The lock layer owns:

- Persisting the resolved package set to `pylock.toml`.
- Recording the selected environments, groups, extras, and artifacts.
- Recording Pyra-specific freshness metadata.
- Providing the source of truth for installation.

The lock layer does not:

- Re-resolve during installation.
- Act as the package installer.

## Installer Owns

The installer owns:

- Inspecting the current centralized environment state.
- Comparing installed state with the selected lock subset.
- Planning exact install and removal actions.
- Applying those actions to the environment.
- Downloading, verifying, caching, and reusing locked package artifacts.
- Installing the current project editable when appropriate.

The installer does not:

- Resolve dependency versions.
- Decide what the desired package graph should be.

## Why pip Is Behind a Boundary

Current Pyra uses `python -m pip install --no-deps` and `python -m pip uninstall`
behind the installer boundary.

This is intentional:

- pip is used only to apply explicit locked artifacts.
- pip is not allowed to resolve dependencies.
- The desired package set still comes from `pylock.toml`.

That boundary keeps the system honest today and leaves room for a future Pyra
native installer backend without changing the sync model.
