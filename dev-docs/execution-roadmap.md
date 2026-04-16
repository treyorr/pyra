# Execution Roadmap

This document is the delivery plan for keeping Pyra focused on the smallest
command surface that covers the vast majority of day-to-day Python workflows.

This is not a replacement for the architecture contracts in this directory. It
is the execution plan that sits on top of those contracts.

Use this document when:

- planning implementation order
- deciding what ships now versus later
- deciding which commands are demand-driven and can wait
- checking whether Pyra is ready for broad daily-driver use

The permanent product model remains:

1. `pyproject.toml` is declared intent.
2. `pylock.toml` is resolved state.
3. the centralized environment is applied state.

## Scope Decision

### Honest 99% Command Set

For now, Pyra should optimize for the command set that covers what most Python
developers actually do every day:

- initialize a project
- pick and pin Python
- add and remove dependencies
- synchronize lock and environment
- run scripts and tests
- inspect project health
- refresh dependencies within existing intent

In command terms, that means keeping focus on:

- existing baseline: `init`, `python ...`, `use`, `sync`, `add`, `remove`, `run`
- next minimum adds: `doctor`, `lock`, `outdated`, `update`

### Strip For Now (Wait For Demand)

The following should be deferred until users explicitly ask for them or usage
signals make them unavoidable:

- cache command suite (`cache dir/info/clean/prune`)
- environment and global cleanup suite (`clean`, `env list/gc`, `gc global`)
- dependency introspection extras (`tree`, `why`)
- intent controls beyond core (`pin`, `unpin`, `upgrade`)
- export formats beyond immediate need (`constraints`, SBOM)
- release ergonomics (`version`, `changelog`, `release preflight`)
- package release surface (`build`, `publish`) unless current target users are package publishers
- policy command surface (allowlists, direct URL policy, hash strict profiles)
- monorepo/workspace strategy implementation
- self-update beyond the simple `pyra self update` binary updater

These are valuable, but they are not required for 99% of everyday Python app
development workflows.

## Milestones

### M1: 99% Workflow Completeness

Purpose:
Finish the smallest missing command set needed for broad day-to-day use.

Commands:

- `pyra doctor`
- `pyra lock`
- `pyra outdated`
- `pyra update`

Why it matters:
This closes the biggest practical gaps without bloating command surface area.

### M2: CI Contract Hardening

Purpose:
Make the lean command set stable for automation.

Focus:

- explicit exit code semantics
- stable machine-readable output for major commands
- non-interactive behavior where required
- golden CI scenarios for lock and sync behavior

Why it matters:
Confidence comes from predictable behavior, not command count.

### M3: Demand-Driven Expansion

Purpose:
Add deferred commands only when user demand is clear.

Why it matters:
Prevents overbuilding and keeps Pyra calm, focused, and maintainable.

## Task Breakdown

Tasks are listed in strict order inside each milestone. Task IDs are stable and
intended to be referenced directly in implementation prompts.

### M1 Tasks

#### M1.1: Shared Command Contract Foundation

What to implement:
Define shared command result and error envelopes so new commands behave
consistently in human and machine-readable modes.

Where in codebase:

- `crates/pyra-project/src/error.rs`
- `crates/pyra-ui/src/output.rs`
- `crates/pyra-cli/src/lib.rs`

Acceptance criteria:

- consistent success/warn/fail structure
- consistent JSON envelope for major commands
- clear mapping to exit code categories

Tests required:

- result envelope unit tests
- JSON snapshot tests
- exit mapping tests

#### M1.2: Add Explicit `pyra lock`

What to implement:
Add explicit lock generation command separated from sync environment side effects.

Where in codebase:

- new `crates/pyra-cli/src/commands/lock.rs`
- lock service additions under `crates/pyra-project/src/`

Acceptance criteria:

- deterministic lock generation
- no environment reconciliation side effect
- clear lock freshness messaging

Tests required:

- lock generation test
- stale lock regeneration test

#### M1.3: Add `pyra doctor`

What to implement:
Add read-only diagnostics for interpreter mismatch, missing or stale lock, and
environment drift.

Where in codebase:

- new `crates/pyra-cli/src/commands/doctor.rs`
- diagnostic modules under `crates/pyra-project/src/`

Acceptance criteria:

- actionable diagnosis output
- no state mutation
- JSON output mode

Tests required:

- missing lock diagnosis test
- stale lock diagnosis test
- environment drift diagnosis test

#### M1.4: Add `pyra outdated`

What to implement:
Add read-only report of newer available versions from current dependency intent.

Where in codebase:

- new `crates/pyra-cli/src/commands/outdated.rs`
- resolver/query service additions under `crates/pyra-project/src/`

Acceptance criteria:

- no mutation of lock or manifest
- clear package-level report
- machine-readable mode for CI dashboards

Tests required:

- outdated report coverage test
- no-mutation behavior test

#### M1.5: Add `pyra update`

What to implement:
Refresh lock to latest versions allowed by existing specifiers.

Where in codebase:

- new `crates/pyra-cli/src/commands/update.rs`
- service updates under `crates/pyra-project/src/`

Acceptance criteria:

- deterministic lock rewrite
- dry-run summary mode
- clear difference from upgrade-style intent mutation

Tests required:

- update lock rewrite test
- dry-run summary test
- deterministic output test

### M2 Tasks

#### M2.1: Exit Code Contract Finalization

What to implement:
Document and enforce explicit exit code semantics for major commands.

Where in codebase:

- `crates/pyra-errors/src/lib.rs`
- `crates/pyra-cli/tests/`

Tests required:

- command-level exit-code consistency suite

#### M2.2: Machine-Readable Contract Hardening

What to implement:
Harden JSON output contracts for major commands and add regression snapshots.

Where in codebase:

- output rendering and CLI command modules

Tests required:

- JSON contract snapshot suite

#### M2.3: CI Golden Scenarios

What to implement:
Add integration scenarios for fresh/stale/missing/corrupt lock and key sync
failure paths.

Where in codebase:

- `crates/pyra-cli/tests/`

Tests required:

- fresh lock scenario
- stale lock scenario
- missing lock scenario
- corrupt lock scenario

### M3 Tasks

#### M3.1: Demand Signal Review

What to implement:
Review real user requests and support pain to select first deferred commands to
promote.

Acceptance criteria:

- promotion decisions are evidence-based
- every promoted command has explicit owner and scope

#### M3.2: Promote First Deferred Command Tranche

What to implement:
Implement only the top one to three deferred command areas with strongest demand.

Example candidates:

- dependency graph introspection (`tree`, `why`)
- cleanup and cache controls
- release workflow controls

## Critical Path

These tasks block broad daily-driver confidence:

1. M1.1 foundation
2. M1.2 lock command
3. M1.3 doctor
4. M1.4 outdated
5. M1.5 update
6. M2 contract hardening

Non-negotiable order:

1. M1.1
2. M1.2
3. M1.3
4. M1.4
5. M1.5
6. M2.1
7. M2.2
8. M2.3
9. M3

## Documentation Plan

Create:

- `docs/lock-command-model.md`
- `docs/doctor-model.md`
- `docs/outdated-model.md`
- `docs/update-model.md`
- `docs/command-contracts.md`

Update:

- `docs/product-direction.md`
- `docs/error-model.md`
- `docs/testing-strategy.md`
- `docs/sync-model.md`

## 30 / 60 / 90 Day Plan

### 30 Days

Ship M1.1 to M1.3:

- command contract foundation
- explicit lock command
- doctor diagnostics

### 60 Days

Ship M1.4, M1.5, and M2.1:

- outdated visibility
- update command
- exit code contract finalization

### 90 Days

Ship M2.2, M2.3, and complete M3.1:

- machine-readable contract hardening
- CI golden scenarios
- demand review for deferred command promotion

## Definition Of Done

Pyra is ready for broad everyday use when all of the following are true:

1. Lean 99% command set is complete (`doctor`, `lock`, `outdated`, `update`).
2. Existing core commands remain stable (`init`, `python`, `use`, `sync`,
   `add`, `remove`, `run`).
3. Major commands have stable JSON and exit code semantics.
4. CI golden scenarios cover lock lifecycle failures and sync safety behavior.
5. Deferred command additions are explicitly demand-gated rather than assumed.

Pyra should avoid adding broad command families until those criteria hold and
real users request the next tranche.
