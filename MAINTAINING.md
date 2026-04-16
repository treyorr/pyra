# Maintaining Pyra

This document captures the repo-level setup and release flow needed to keep
Pyra healthy as a public project.

## GitHub repository setup

Apply these settings manually in GitHub after the workflows in this repository
have run at least once.

### Branch protection / ruleset for `main`

Recommended settings for a solo maintainer:

- Require a pull request before merging
- Require status checks to pass before merging
- Require conversation resolution before merging
- Do not allow bypassing the above settings
- Block force pushes
- Block deletions

Recommended merge settings:

- Enable squash merge
- Disable merge commits
- Disable rebase merge unless you explicitly want it
- Enable auto-merge if you want GitHub to merge once checks finish

Status checks to require after the first green run:

- `rust`
- `docs`

Important solo-maintainer note:

- Do not require approving reviews or code owner reviews while you are the only
  maintainer opening PRs from this repository. GitHub does not let authors
  approve their own pull requests, so that setting would block your releases and
  routine maintenance work.
- If you later add a second maintainer, turn on required approvals and required
  code owner review.

### Access control

If this remains a personal repository, external contributors will not be able
to merge unless you explicitly grant them write access.

Keep the collaborator list minimal. If you ever add more maintainers:

- review their role explicitly
- keep `CODEOWNERS` current
- revisit whether `main` should require approvals

### Security settings

Enable:

- private vulnerability reporting
- Dependabot security alerts
- secret scanning, if available for the repository plan

### GitHub Pages

If you want the docs site live on GitHub Pages for now:

- In repository settings, set Pages to build from GitHub Actions
- Let the release workflow deploy the docs after each tagged release
- Use the default GitHub Pages URL first, which should be `https://treyorr.github.io/pyra`
- Later, if you move the docs to a custom domain, update the release workflow
  `DOCS_SITE_URL` and `DOCS_BASE_PATH` values together

## Release flow

Pyra uses a tag-driven release workflow.

### Before tagging

1. Update the workspace version in [Cargo.toml](/Users/treyorr/pyra-project/Cargo.toml).
2. Add release notes to [CHANGELOG.md](/Users/treyorr/pyra-project/CHANGELOG.md).
3. Make sure CI is green on `main`.
4. Create and merge a release-prep PR.

### Publish a release

1. Create a tag that matches the workspace version, for example `v0.1.0`.
2. Push the tag to GitHub.
3. Let `.github/workflows/release.yml` build the release archives and create the
   GitHub Release automatically.
4. Let the same workflow deploy the docs site to GitHub Pages.
5. Review the generated release notes and edit them if needed.

The workflow publishes these asset names:

- `pyra-aarch64-apple-darwin.tar.gz`
- `pyra-x86_64-apple-darwin.tar.gz`
- `pyra-x86_64-unknown-linux-gnu.tar.gz`
- `pyra-x86_64-pc-windows-msvc.zip`

Each asset is paired with a `.sha256` checksum file.

## Install script contract

This repository ships [install.sh](/Users/treyorr/pyra-project/install.sh).
Keep it aligned with the release workflow above:

- download assets from GitHub Releases
- select the archive by OS and architecture
- verify checksums when practical
- install the `pyra` binary onto the user's `PATH`

If you host the installer at `https://tlo3.com/pyra-install.sh`, publish the same
script from this repository so the website, README, and release assets all
follow one contract.

## CLI self-update

Pyra uses `pyra self update` for binary updates and keeps `pyra update` for
dependency lock refreshes.

The self-update command reads the same GitHub Release assets listed above, so
the installer and the updater share one release contract:

- install once with `curl -fsSL https://tlo3.com/pyra-install.sh | sh`
- update later with `pyra self update`

Keep release asset names, archive contents, and checksum publishing stable so
both flows continue to work without special cases.
