# Error Model

Pyra should feel calm and precise, especially when things fail.

Error behavior is part of the product, not an afterthought.

## Core Goals

User-facing errors should answer:

1. what failed
2. why it failed
3. what the user should do next

Errors should preserve typed domain meaning for maintainers while keeping the
default terminal experience concise and actionable.

## Current Error Categories

Pyra should continue distinguishing at least:

- user or input errors
- project state errors
- IO or system errors
- internal invariant violations

This category split should remain visible in the domain layer even if the CLI
renders them with a shared presentation model.

## Default Output Rules

Default error output should:

- be concise
- be readable
- avoid debug dumps
- suggest the next useful action

Verbose mode may include:

- paths
- raw subprocess stderr
- parser details
- internal decision context

Verbose mode should add visibility, not change behavior.

## Dependency Flow Errors

Dependency-related commands should keep failures explicit about which layer
failed:

- input parsing
- lock freshness or lock parsing
- resolution
- environment inspection
- installation or removal
- editable install

This makes it easier to debug package manager behavior without collapsing every
failure into "sync failed".

## Documentation And Errors

When behavior is subtle enough that the error text alone cannot carry the full
contract, the relevant doc in `docs/` should also be updated.

That is especially important for:

- lock freshness
- group selection semantics
- environment reconciliation
- future add/remove/run behavior

## Long-Term Direction

As Pyra grows into a stronger package and runtime toolchain, typed errors should
continue flowing upward without losing:

- recoverability
- context
- user guidance

That discipline matters more as the dependency pipeline gets deeper.
