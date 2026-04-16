# Self-Update Model

`pyra self update` owns only the installed Pyra binary lifecycle.

It exists so Pyra can update itself without overloading the meaning of
`pyra update`, which already means "refresh project lock state within existing
dependency intent."

## Command boundary

`pyra self update`:

- checks GitHub Releases for the latest compatible Pyra binary
- downloads the matching release asset for the current target
- replaces the installed executable in place
- leaves project files and environments untouched

It must not:

- read or mutate `pyproject.toml`
- rewrite `pylock.toml`
- sync a project environment
- change declared dependency intent

## Release contract

The self-update command shares one release contract with `install.sh`.

It expects:

- GitHub Releases under `treyorr/pyra`
- tags in the form `vX.Y.Z`
- stable asset names like `pyra-<target>.tar.gz` and `pyra-<target>.zip`

The wider release flow should also publish paired `.sha256` checksum files for
the install script, even though `pyra self update` itself only depends on the
release tags and compatible archives.

That shared contract keeps first install and later update aligned.

## UX contract

Default output should stay concise:

- report when Pyra is already current
- report the previous and new version when an update is applied
- keep release-backend details for verbose mode

Failures must explain:

- whether Pyra could not find the right release metadata
- whether the download/apply step failed
- what the user should do next

## Why it is a separate command

Pyra already has a strong product meaning for:

- `pyra update` -> refresh project lock state
- `pyra add` / `pyra remove` -> mutate dependency intent
- `pyra sync` -> reconcile environment state

Adding binary self-update behind `pyra self update` preserves those meanings and
avoids turning `update` into an overloaded command with unrelated state changes.
