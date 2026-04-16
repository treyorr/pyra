# Initiative

open-source-readiness

## Current Phase

implementation

## Progress

- [x] M1
- [x] M2
- [ ] M3

## Active Task

M3.1

## Completed Tasks

- M1.1 - 2026-04-16 - Added public repository policy files and GitHub templates
- M1.2 - 2026-04-16 - Replaced unsupported install guidance with release-ready documentation
- M2.1 - 2026-04-16 - Added CI workflow for Rust verification and docs validation
- M2.2 - 2026-04-16 - Added tagged release workflow with stable asset names
- M3.1 - 2026-04-16 - Added install script-aligned CLI self-update and updated docs contracts

## Risks / Blockers

- Medium - GitHub rulesets, required checks, and private vulnerability reporting
  still need to be enabled manually in repository settings
- Low - GitHub Actions still need one green run on the updated docs build job
  before you can require the `docs` check in branch protection

## Next Decision

Apply the GitHub repository settings manually, then decide whether the next
follow-up is release polish, docs refinement, or broader launch prep.
