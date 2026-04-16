# Title

Open-source readiness

## Problem

Pyra has solid product and engineering documentation, but the repository still
needs the operational surface expected from a public project: contributor docs,
security policy, issue/PR templates, CI, release automation, and a documented
maintainer workflow.

## Outcome

Pyra can be presented publicly with clear contributor guidance, enforceable CI
expectations, a repeatable release flow, and honest install/update
documentation.

## In Scope

- Public repository documentation and templates
- GitHub Actions CI and tagged release automation
- Maintainer instructions for branch protection and release management
- Honest install/update guidance that matches what is actually published today

## Out Of Scope

- Marketing site or launch announcement copy

## User Impact

Potential users get a trustworthy public repository and maintainers get a
repeatable release and triage workflow.

## Constraints

- Preserve documented architecture boundaries
- Do not advertise install/update paths that are not live yet
- Keep the release contract compatible with a future `self_update` command
- Solo-maintainer workflows must remain practical

## Evidence

- Public docs currently mention `https://pyra.dev/install.sh`, which is not yet
  published from this repository
- Standard public repo files and GitHub workflows are missing
- Release and merge policy are not yet captured in-repo
