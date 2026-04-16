# Open-Source Readiness Audit

## Current status

Partially ready.

## Ready now

- Public docs site exists
- Internal architecture docs exist
- Contributor, security, and maintainer docs now exist
- CI and release automation are defined in-repo
- The install script and `pyra self update` now share one release contract
- The release workflow can also publish the docs site to GitHub Pages

## Remaining blockers before wider announcement

- Run the new GitHub Actions workflows successfully on GitHub
- Configure branch protection / rulesets for `main`
- Enable private vulnerability reporting
- Publish the first tagged release so install docs can point to a real release
- Host the installer script at `https://tlo3.com/pyra-install.sh`
- Turn on GitHub Pages in repository settings so the release workflow can deploy docs

## Notes

The release contract now supports both:

- first install through `https://tlo3.com/pyra-install.sh`
- later binary updates through `pyra self update`

That still remains separate from `pyra update`, which continues to mean
dependency lock refresh.
