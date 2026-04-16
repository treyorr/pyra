# Open-Source Readiness Roadmap

## Scope Decision

### In Scope

- Public-facing repository hygiene and contributor documentation
- GitHub Actions CI for Rust and docs
- Tag-driven release automation with stable asset naming
- Maintainer runbook for GitHub settings and release steps

### Deferred

- `self_update` implementation
- One-line installer publication
- Signed artifacts and package-manager distribution

## Milestones

### M1: Public repository baseline

Purpose:
Add the public files and templates users expect from an open-source project.

### M2: Automation and release pipeline

Purpose:
Make CI and tagged releases repeatable before increasing project visibility.

### M3: Maintainer operations and launch follow-through

Purpose:
Document the manual GitHub settings and the remaining pre-launch tasks.

## Task Breakdown

### M1 Tasks

#### M1.1: Add repository policy and contributor files

What to implement:
Add `LICENSE`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, maintainer
guidance, and issue/PR templates.

Where in codebase:

- repository root
- `.github/`

Acceptance criteria:

- Public repo essentials are present and coherent
- Security reporting path is documented
- Contribution workflow references `mise` tasks and docs-first rules

Tests required:

- Manual content review

#### M1.2: Fix install and release docs drift

What to implement:
Remove install instructions that point to unpublished assets and replace them
with release-compatible guidance.

Where in codebase:

- `README.md`
- `docs/src/content/docs/`

Acceptance criteria:

- No public doc points to a nonexistent installer
- Install instructions match the actual release strategy

Tests required:

- Manual content review

### M2 Tasks

#### M2.1: Add required CI workflow

What to implement:
Create a GitHub Actions workflow for Rust verification and docs validation on
pushes and PRs to `main`.

Where in codebase:

- `.github/workflows/ci.yml`

Acceptance criteria:

- Rust checks run on PRs and pushes to `main`
- Docs content and config validation run in CI
- Job names are stable enough to use in branch protection

Tests required:

- GitHub Actions dry run after merge

#### M2.2: Add tagged release workflow

What to implement:
Create a tag-driven release workflow that validates the tag version, builds
platform archives, and publishes a GitHub Release.

Where in codebase:

- `.github/workflows/release.yml`

Acceptance criteria:

- Tag and workspace version must match
- Release assets use stable, updater-friendly names
- Checksums are published alongside archives

Tests required:

- Dry run on a test tag

### M3 Tasks

#### M3.1: Document GitHub ruleset decisions

What to implement:
Capture the exact manual GitHub settings for a solo maintainer and call out
settings that would block self-review.

Where in codebase:

- `MAINTAINING.md`

Acceptance criteria:

- Maintainer can configure branch protection without guesswork
- Solo-maintainer review caveats are explicit

Tests required:

- Manual review

#### M3.2: Plan updater and installer follow-up

What to implement:
Define the release contract that a future install script and `self_update`
integration must consume.

Where in codebase:

- `MAINTAINING.md`
- future implementation tasks

Acceptance criteria:

- Asset naming contract is documented
- Deferred work is explicit, not implied as already shipped

Tests required:

- Manual review

## Critical Path

1. M1.1
2. M1.2
3. M2.1
4. M2.2
5. M3.1
6. M3.2

## Definition Of Done

1. Repo essentials, templates, and maintainer guidance are merged
2. CI and release workflows exist and run successfully in GitHub
3. Public docs only advertise supported install paths
4. GitHub rulesets are configured manually and verified
5. Full production docs build is either fixed or intentionally deferred with a
   documented follow-up plan
