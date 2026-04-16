# Contributing

Thanks for taking an interest in Pyra.

Pyra is a long-term Rust workspace for a modern Python toolchain. We care about
clear boundaries, calm UX, and predictable behavior more than short-term hacks.

## Before you change code

Start with the docs:

- Read [README.md](/Users/treyorr/pyra-project/README.md) for the public product
  surface.
- Read [dev-docs/README.md](/Users/treyorr/pyra-project/dev-docs/README.md) and
  the relevant contract docs before changing behavior.
- Follow [AGENTS.md](/Users/treyorr/pyra-project/AGENTS.md) for repository
  standards, architecture boundaries, and maintainership expectations.

If code and docs disagree, fix the mismatch intentionally. Do not leave them
drifting.

## Local setup

Pyra uses [`mise`](https://mise.jdx.dev/) for local tools and routine tasks.

```bash
mise install
mise run verify
```

Useful commands:

```bash
mise run fmt
mise run check
mise run clippy
mise run test
mise run docs:install
mise run docs:dev
mise run docs:build
```

## Engineering rules

- Preserve the architecture split between CLI parsing, command orchestration,
  domain logic, and terminal presentation.
- Keep changes narrow and intentional.
- Add or update docs when behavior or guarantees change.
- Keep user-facing output concise and actionable.
- Use typed errors in domain logic.
- Add comments where contracts, invariants, or non-obvious behavior would be
  easy to misread later.

## Pull requests

For the fastest review:

- Keep one focused change per PR.
- Explain the user-visible outcome and the key implementation choices.
- Include test evidence for the behavior you changed.
- Update docs, examples, and release notes when needed.

PRs that change behavior should usually include:

- code
- tests
- docs

## Reporting bugs and ideas

- Use the issue templates for bug reports and feature requests.
- For security issues, follow [SECURITY.md](/Users/treyorr/pyra-project/SECURITY.md)
  and do not open a public issue.
