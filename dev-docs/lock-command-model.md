# Lock Command Model

`pyra lock` is the explicit lock-generation command introduced in M1.2.

Its job is to make lock lifecycle management explicit without coupling lock
refresh to environment reconciliation side effects.

## Purpose

`pyra lock` only manages `pylock.toml`:

1. load current project dependency inputs
2. evaluate lock freshness
3. resolve and write a lock when needed
4. report why the lock was generated, regenerated, or reused

It does **not** reconcile the centralized environment.

## Freshness Inputs

`pyra lock` uses the same lock freshness model as `pyra sync`:

- normalized dependency input fingerprint
- selected interpreter version
- selected lock target set
- index URL
- resolution strategy identifier

## Behavior

`pyra lock` has one default behavior:

- if `pylock.toml` is missing, resolve and generate it
- if `pylock.toml` exists but is stale (or unreadable due to parse drift), resolve and regenerate it
- if `pylock.toml` is fresh, reuse it without rewriting

The command reports freshness clearly in terminal output:

- generated from missing lock
- regenerated from stale lock
- reused fresh lock

## Target Selection

Lock targets come from:

1. one-command `--target` overrides (if provided)
2. `[tool.pyra].targets` in `pyproject.toml`
3. current host target fallback

The selected set must still include the current host target.

## Boundary With `pyra sync`

- `pyra lock` owns lock freshness and lock rewrite behavior only.
- `pyra sync` still owns lock-aware environment reconciliation.
- `sync --locked` and `sync --frozen` behavior is unchanged.
