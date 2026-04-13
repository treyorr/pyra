# Package Cache Model

Pyra keeps a persistent local cache of verified package artifacts.

This cache exists to make repeated `pyra sync` runs faster. It does not change
the lock model, resolver model, or installer boundary.

## Source Of Truth

`pylock.toml` remains the source of truth for installation.

The cache is not authoritative package state. It is a reusable local copy of an
artifact that was already selected by the lock and verified against the lock's
recorded SHA-256.

## Ownership

The installer owns package artifact caching.

That means the installer may:

- download a locked artifact
- verify its SHA-256 against `pylock.toml`
- store the verified bytes under a Pyra-managed cache path
- reuse that cached artifact on later syncs

The resolver does not own this cache, and the lock does not become a cache
index.

## Cache Key

Verified artifacts are stored under a content-addressed cache path keyed by the
locked SHA-256.

This keeps cache identity aligned with the lock's integrity data rather than
with transient source URLs.

## Reuse Rule

Pyra may reuse a cached artifact only after re-checking that cached file's
SHA-256 against the locked hash.

If the cached bytes do not match the lock:

- Pyra treats the cache entry as corrupted or stale
- Pyra discards that cache entry
- Pyra falls back to the locked source artifact and verifies it again before
  install

Cache reuse must never weaken hash verification.

## Scope

This cache is a local performance layer for repeated installs on one machine.

It does not currently promise:

- shared cache coordination across machines
- lock portability changes
- cross-platform artifact substitution
- separate cache management commands
