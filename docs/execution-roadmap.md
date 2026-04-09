# Execution Roadmap

This document is the execution plan for turning Pyra from a clean prototype
into a production-grade package manager core, then into a broader developer
tool and runtime foundation.

This is not a replacement for the architecture contracts in this directory.
It is the delivery plan that sits on top of those contracts.

Use this document when:

- planning implementation order
- assigning work to coding agents
- deciding what is blocked versus parallelizable
- checking whether Pyra is ready to expand beyond package management

The permanent product model remains:

1. `pyproject.toml` is declared intent.
2. `pylock.toml` is resolved state.
3. the centralized environment is applied state.

The permanent dependency pipeline remains:

1. read project inputs
2. select dependency scopes
3. check lock freshness
4. resolve and write lock if needed
5. reconcile the environment exactly from the lock

## Immediate Docs Sync Checkpoint

Before moving into broader implementation, treat the following as the
highest-priority code-and-doc alignment work:

1. Lock freshness docs already say `resolution-strategy` is part of freshness,
   but current code does not compare it during lock reuse. This is a real
   contract mismatch and is tracked by M1.5.
2. The docs describe `pylock.toml` hashes as authoritative install data, but
   current install flow does not yet verify artifact hashes before handing the
   artifact URL to pip. This is a critical safety gap and is tracked by M1.8.
3. The docs describe exact reconciliation as a Pyra-owned responsibility, but
   current environment inspection still depends on `pip list`. This is a fragile
   implementation gap and is tracked by M1.7.
4. The docs already describe a stable sync, lock, and environment model. The
   roadmap should be read as the plan to make the implementation fully live up
   to those contracts, not as permission to redesign them.

## Milestones

### M1: Production-Grade Sync Correctness

Purpose:
Make `pyra sync` safe, deterministic, and correct for the scope Pyra currently
claims to support.

Why it matters:
Pyra is blocked by correctness gaps, not architecture gaps. `sync` must become
trustworthy before expanding package-manager UX or runtime features.

What becomes possible after it:

- reliable lock reuse
- safe installs from `pylock.toml`
- CI-safe reproducibility work
- exact reconciliation with stronger guarantees

### M2: Package Manager UX And CI Contract

Purpose:
Add the command behavior that makes Pyra usable as a daily package manager:
`--locked`, `--frozen`, `add`, `remove`, and minimal `run`.

Why it matters:
Without these flows, Pyra still behaves like a prototype around `sync` rather
than a complete package manager.

What becomes possible after it:

- CI workflows that can enforce lock discipline
- one-command dependency mutation through `pyproject.toml`
- project execution through the centralized synchronized environment

### M3: Performance And Stability

Purpose:
Make Pyra fast enough and stable enough for repeated use on real projects.

Why it matters:
Correctness is necessary, but a package manager that stays slow or fragile will
not hold up under normal development workflows.

What becomes possible after it:

- warm-cache installs
- faster repeated syncs
- clearer conflict and failure behavior
- stronger confidence on real-world package graphs

### M4: Platform-Aware Lock Evolution

Purpose:
Move from the current single-platform lock model toward honest
platform-aware and eventually multi-environment locking.

Why it matters:
Cross-platform teams and CI cannot rely on a lock model that only describes one
host environment.

What becomes possible after it:

- one lock that can describe more than one target
- current-host installs from a shared multi-target lock
- explicit evolution toward cross-platform package management

### M5: Runtime Foundation

Purpose:
Turn the synchronized environment model into a stable execution substrate for
scripts, test running, and notebook-oriented workflows.

Why it matters:
Pyra is intended to grow into a Bun-like runtime for Python, but that can only
happen safely if execution builds on the same interpreter, lock, and
environment contracts.

What becomes possible after it:

- stable `run` behavior
- thin runtime commands built on the same environment model
- future expansion into notebooks and broader execution features

## Task Breakdown

Tasks are listed in strict order inside each milestone. Task IDs are stable and
intended to be referenced directly in implementation prompts.

### M1 Tasks

#### M1.1: Build Hermetic Resolver Fixture Harness

What to implement:
Create reusable file-backed Simple API fixtures and resolver test helpers so
`pyra-resolver` can be tested without network access. Support wheels, sdists,
core metadata, yanked files, marker-gated dependencies, extras, and conflict
graphs.

Where in codebase:

- `crates/pyra-resolver/src/simple.rs`
- `crates/pyra-resolver/src/provider.rs`
- new test support modules under `crates/pyra-resolver/src/` or `tests/`

Why this matters:
All resolver hardening depends on deterministic local fixtures. Without this,
future correctness work is guesswork.

Dependencies:
None.

Acceptance criteria:

- resolver tests run with no internet access
- fixture helpers can express both successful and failing resolution cases
- new resolver tests do not depend on the CLI crate

Tests required:

- direct dependency resolution fixture
- transitive dependency fixture
- conflict fixture
- missing metadata fixture
- no installable artifact fixture
- wheel-first and sdist fallback fixture

#### M1.2: Add Version Range And Wheel Compatibility Test Matrix

What to implement:
Add exhaustive unit tests for supported PEP 440 range handling and current wheel
compatibility logic.

Where in codebase:

- `crates/pyra-resolver/src/version.rs`

Why this matters:
Version range bugs or wheel-tag bugs produce bad locks while appearing to work.

Dependencies:
M1.1

Acceptance criteria:

- supported operators `==`, `!=`, `<`, `<=`, `>`, `>=`, `~=`, and wildcard
  matching are covered
- supported target triples are covered for positive and negative wheel matches
- `abi3`, `py3-none-any`, macOS arm64/x86_64, and Linux x86_64/aarch64 cases
  are explicitly tested

Tests required:

- unit tests for each supported version operator
- unit tests for wildcard range upper bounds
- unit tests for compatible release upper bounds
- wheel compatibility tests for supported host combinations
- sdist compatibility acceptance test

#### M1.3: Add Marker And Root-Membership Correctness Tests

What to implement:
Verify resolution-time marker evaluation and root-token propagation for base
dependencies, dependency groups, and extras.

Where in codebase:

- `crates/pyra-resolver/src/provider.rs`
- `crates/pyra-resolver/src/marker.rs`
- `crates/pyra-resolver/src/metadata.rs`

Why this matters:
Lock selection in `pyra-project` depends on resolver root membership being
correct.

Dependencies:
M1.1

Acceptance criteria:

- group-only packages receive only group root tokens
- extra-only packages receive only extra root tokens
- shared transitive packages preserve all applicable root memberships
- marker-filtered requirements are excluded when the resolver environment does
  not match

Tests required:

- group-only dependency fixture
- extra-only dependency fixture
- shared dependency across base and group fixture
- marker-filtered dependency fixture
- duplicate logical declarations across scopes fixture

#### M1.4: Enforce Project `requires-python`

What to implement:
Fail `pyra sync` when `[project].requires-python` excludes the pinned managed
interpreter. Apply the same rule before lock reuse.

Where in codebase:

- `crates/pyra-project/src/sync/project_input.rs`
- `crates/pyra-project/src/service.rs`
- `crates/pyra-project/src/error.rs`

Why this matters:
Pyra must not lock or install a project for an interpreter the project itself
declares unsupported.

Dependencies:
M1.1

Acceptance criteria:

- incompatible pinned interpreters fail before resolution
- compatible interpreters continue normally
- error text names the selected interpreter and the project constraint

Tests required:

- service-level test for compatible interpreter
- service-level test for incompatible interpreter
- CLI integration test for sync failure on mismatch

#### M1.5: Complete Lock Freshness Model

What to implement:
Centralize freshness inputs into a typed model and include
`resolution-strategy` in freshness checks. Stop relying on incomplete
comparisons.

Where in codebase:

- `crates/pyra-project/src/sync/lockfile.rs`
- `crates/pyra-project/src/service.rs`

Why this matters:
Incorrect lock reuse is one of the highest-risk failures in the system.

Dependencies:
M1.4

Acceptance criteria:

- freshness compares dependency fingerprint, interpreter version, target
  triple, index URL, and resolution strategy
- unchanged inputs reuse the lock
- any change to those inputs invalidates reuse

Tests required:

- unit tests for each freshness input
- lock round-trip test including strategy
- CLI integration test proving stale lock regeneration
- CLI integration test proving fresh lock reuse

#### M1.6: Replace Fragile Lock Marker Matching

What to implement:
Replace string splitting in install selection with a parser and evaluator for
the limited marker grammar Pyra writes to `pylock.toml` for `dependency_groups`
and `extras`.

Where in codebase:

- `crates/pyra-project/src/sync/install.rs`
- `crates/pyra-project/src/sync/lockfile.rs`
- new `crates/pyra-project/src/sync/marker.rs`

Why this matters:
Current marker matching is structurally fragile and will break as lock markers
become slightly more complex.

Dependencies:
M1.3
- M1.5

Acceptance criteria:

- lock marker parsing is explicit and validated
- malformed markers fail as lock parse or selection errors
- selection supports multiple groups, multiple extras, and mixed clauses

Tests required:

- parser unit tests for supported clauses
- evaluator unit tests for group and extra combinations
- lock round-trip test with generated markers
- reconciliation selection tests for mixed marker cases

#### M1.7: Remove Dependency On `pip list` For Environment Inspection

What to implement:
Inspect installed distributions using `importlib.metadata` via the managed
interpreter instead of `python -m pip list --format=json`.

Where in codebase:

- `crates/pyra-project/src/sync/install.rs`

Why this matters:
Exact reconciliation should not depend on pip being healthy for a read-only
inspection step.

Dependencies:
M1.5

Acceptance criteria:

- Pyra can inspect installed distributions without invoking `pip list`
- inspection still produces normalized package names and versions
- errors clearly identify environment inspection failures

Tests required:

- installer unit test for normalized package inspection
- installer unit test for malformed inspection output
- CLI integration test proving sync does not depend on `pip list`

#### M1.8: Add Verified Artifact Install Path

What to implement:
Download the selected locked artifact, verify its SHA-256, then install from
the verified local file. Do not pass remote artifact URLs directly to pip.

Where in codebase:

- `crates/pyra-project/src/sync/install.rs`
- `crates/pyra-core/src/paths.rs` for cache or staging paths if needed

Why this matters:
Without hash verification, `pylock.toml` does not provide real install
integrity.

Dependencies:
M1.5
- M1.6

Acceptance criteria:

- a hash mismatch aborts sync before install
- successful installs use only verified local artifact paths
- installer cleanup behavior is deterministic on failure

Tests required:

- installer unit test for matching hash
- installer unit test for mismatched hash
- installer unit test for failed download
- CLI integration test for verified local artifact install

### M2 Tasks

#### M2.1: Add `sync --locked` And `sync --frozen`

What to implement:
Add explicit reproducibility modes to `pyra sync`.

Where in codebase:

- `crates/pyra-cli/src/cli/mod.rs`
- `crates/pyra-cli/src/commands/sync.rs`
- `crates/pyra-project/src/service.rs`
- `crates/pyra-project/src/error.rs`

Why this matters:
CI workflows need clear control over whether lock generation is allowed.

Dependencies:
M1 complete

Acceptance criteria:

- default sync resolves when needed
- `--locked` requires an existing fresh lock and never resolves
- `--frozen` requires an existing lock, never resolves, and never rewrites it
- `--locked` and `--frozen` are mutually exclusive

Tests required:

- CLI test for `--locked` with missing lock
- CLI test for `--locked` with stale lock
- CLI test for `--locked` with fresh lock
- CLI test for `--frozen` using a stale lock without rewrite

#### M2.2: Add Typed `pyproject.toml` Mutation API

What to implement:
Add manifest edit helpers for adding and removing dependency declarations in
base dependencies, dependency groups, and extras while preserving unrelated
formatting.

Where in codebase:

- `crates/pyra-project/src/pyproject.rs`

Why this matters:
`add` and `remove` must reuse one domain implementation instead of embedding
TOML editing in CLI handlers.

Dependencies:
M2.1

Acceptance criteria:

- mutation helpers support base dependencies, named groups, and extras
- lookup uses normalized group and extra names
- unrelated project formatting is preserved where practical
- idempotent updates do not introduce duplicate entries

Tests required:

- unit test for add to base dependencies
- unit test for add to dependency group
- unit test for add to extra
- unit test for remove from each scope
- unit test for missing declaration error
- unit test for duplicate prevention

#### M2.3: Implement `pyra add`

What to implement:
Add the CLI command and project service flow that parses a requested
requirement, mutates `pyproject.toml`, then runs the normal sync pipeline.

Where in codebase:

- `crates/pyra-cli/src/cli/mod.rs`
- new `crates/pyra-cli/src/commands/add.rs`
- `crates/pyra-cli/src/commands/mod.rs`
- `crates/pyra-project/src/service.rs`
- `crates/pyra-project/src/pyproject.rs`

Why this matters:
Pyra becomes a real package manager when dependency mutation flows through
declared intent to lock to environment.

Dependencies:
M2.2

Acceptance criteria:

- `pyra add` updates the selected declaration scope in `pyproject.toml`
- `pyra add` triggers normal sync behavior after mutation
- invalid requirements fail before mutating the project

Tests required:

- CLI test for add to base dependencies
- CLI test for add to dependency group
- CLI test for add to extra
- CLI test for already-present requirement
- CLI test for invalid requirement
- CLI test proving `pylock.toml` and environment update after add

#### M2.4: Implement `pyra remove`

What to implement:
Add the CLI command and project service flow that removes a dependency from a
selected declaration scope, then runs normal sync.

Where in codebase:

- `crates/pyra-cli/src/cli/mod.rs`
- new `crates/pyra-cli/src/commands/remove.rs`
- `crates/pyra-cli/src/commands/mod.rs`
- `crates/pyra-project/src/service.rs`
- `crates/pyra-project/src/pyproject.rs`

Why this matters:
Dependency removal must preserve the same one-way model as add and sync.

Dependencies:
M2.2

Acceptance criteria:

- remove targets only the selected scope
- missing declarations fail clearly
- sync removes now-unselected installed packages through exact reconciliation

Tests required:

- CLI test for remove from base dependencies
- CLI test for remove from dependency group
- CLI test for remove from extra
- CLI test for missing dependency in selected scope
- CLI test proving environment cleanup after remove

#### M2.5: Implement Minimal `pyra run`

What to implement:
Add a minimal execution command that discovers the project, ensures the
centralized environment is synchronized, then executes using the managed
interpreter.

Where in codebase:

- `crates/pyra-cli/src/cli/mod.rs`
- new `crates/pyra-cli/src/commands/run.rs`
- `crates/pyra-cli/src/commands/mod.rs`
- new execution module under `crates/pyra-project/src/`
- `crates/pyra-project/src/service.rs`

Why this matters:
Execution must build on sync, not create a second environment model.

Dependencies:
M2.1
- M1 complete

Acceptance criteria:

- `pyra run` ensures the project environment is synchronized before execution
- lookup order follows `docs/run-model.md`
- child exit codes are preserved
- no ad hoc package installation path is introduced

Tests required:

- CLI test for `[project.scripts]` lookup
- CLI test for console script lookup
- CLI test for `.py` file fallback
- CLI test for sync-before-run behavior
- CLI test for exit code propagation

### M3 Tasks

#### M3.1: Add Persistent Verified Artifact Cache

What to implement:
Store verified package artifacts in a persistent content-addressed cache keyed
by hash and reuse them across syncs.

Where in codebase:

- `crates/pyra-core/src/paths.rs`
- `crates/pyra-project/src/sync/install.rs`

Why this matters:
M1 makes installs safe. This task makes them fast and repeatable.

Dependencies:
M1.8

Acceptance criteria:

- repeated sync of the same lock reuses cached artifacts
- corrupted cache entries are detected and discarded
- cache reuse does not weaken hash verification

Tests required:

- unit test for cache miss
- unit test for cache hit
- unit test for corrupted cache entry
- CLI integration test for warm-cache reuse

#### M3.2: Add Parallel Artifact Preparation

What to implement:
Download and verify artifacts concurrently before installation while keeping
final apply order deterministic.

Where in codebase:

- `crates/pyra-project/src/sync/install.rs`

Why this matters:
Repeated package installs become network- and IO-bound without parallelism.

Dependencies:
M3.1

Acceptance criteria:

- artifact preparation uses bounded concurrency
- installation and removal planning stays deterministic
- failures stop the sync cleanly

Tests required:

- async installer test for concurrent preparation
- async installer test for early failure cancellation
- deterministic action ordering test

#### M3.3: Harden Interpreter Lifecycle Behavior

What to implement:
Make the interaction between `pyra use`, environment reuse, and lock freshness
fully reliable across interpreter changes.

Where in codebase:

- `crates/pyra-project/src/environment.rs`
- `crates/pyra-project/src/service.rs`

Why this matters:
Repinning Python is a normal workflow and must not produce mixed or stale
environments.

Dependencies:
M1.4
- M2.1

Acceptance criteria:

- interpreter changes always invalidate lock reuse as needed
- environment reuse remains stable when interpreter identity truly matches
- environment rebuild happens when interpreter identity changes

Tests required:

- integration test for repin to a different patch version
- integration test for repin to a different minor version
- integration test for incompatible repin against `requires-python`

#### M3.4: Build Resolver Regression Corpus And Conflict UX

What to implement:
Expand resolver fixtures to cover common real-world package graph patterns and
improve user-facing conflict messages.

Where in codebase:

- `crates/pyra-resolver/src/provider.rs`
- `crates/pyra-resolver/src/error.rs`
- `crates/pyra-project/src/error.rs`
- `crates/pyra-ui/src/output.rs` as needed

Why this matters:
Phase 2 usability depends on real-world stability and understandable failures.

Dependencies:
M1.1
- M1.6

Acceptance criteria:

- fixture corpus covers extras, markers, multiple artifact choices, sdist-only
  cases, and version conflicts
- conflict output identifies the important incompatible constraints

Tests required:

- resolver integration fixtures for common conflict shapes
- CLI snapshot tests for conflict error rendering

### M4 Tasks

#### M4.1: Introduce Environment-Scoped Lock Schema

What to implement:
Extend `pylock.toml` to describe more than one target environment under a new
strategy identifier.

Where in codebase:

- `crates/pyra-project/src/sync/lockfile.rs`
- `docs/lock-format-notes.md`

Why this matters:
The current lock model cannot honestly describe multiple target environments.

Dependencies:
M1.5
- M1.6

Acceptance criteria:

- lock schema round-trips with multiple environment identifiers
- strategy identifier changes explicitly
- old single-platform locks are not silently trusted under the new model

Tests required:

- lock round-trip test for multi-environment schema
- malformed environment-schema parse test

#### M4.2: Add Multi-Target Resolver Execution

What to implement:
Resolve a target matrix one environment at a time and merge the results into one
lock.

Where in codebase:

- `crates/pyra-project/src/service.rs`
- request helpers in `crates/pyra-resolver/src/`

Why this matters:
Cross-platform support should evolve by composing the existing pipeline, not by
replacing it.

Dependencies:
M4.1

Acceptance criteria:

- Pyra can generate a lock containing more than one environment slice
- per-environment interpreter and target metadata is recorded
- failures identify which target environment failed

Tests required:

- host-plus-linux fixture test
- host-plus-macOS fixture test
- target-specific failure fixture

#### M4.3: Install Current Host From A Multi-Target Lock

What to implement:
Select only the current environment slice plus current groups and extras during
reconciliation.

Where in codebase:

- `crates/pyra-project/src/sync/install.rs`
- `crates/pyra-project/src/service.rs`

Why this matters:
Platform-aware locks only help if installation remains exact and host-specific.

Dependencies:
M4.1
- M4.2

Acceptance criteria:

- current-host sync ignores foreign-target packages
- exact reconciliation semantics stay unchanged for the current host
- verified artifact installs still apply

Tests required:

- selection unit test with mixed-target lock entries
- CLI integration test using a multi-target lock on the current host

#### M4.4: Add Target Configuration Surface

What to implement:
Add project config and CLI support for choosing the target matrix used for lock
generation.

Where in codebase:

- `crates/pyra-project/src/sync/project_input.rs`
- `crates/pyra-project/src/pyproject.rs`
- `crates/pyra-cli/src/cli/mod.rs`

Why this matters:
Cross-platform locking needs a declared source of target intent.

Dependencies:
M4.2

Acceptance criteria:

- projects can declare lock targets
- CLI can override targets for one invocation
- freshness accounts for the target set

Tests required:

- project config parsing tests for targets
- CLI override tests
- freshness invalidation tests for target-set changes

### M5 Tasks

#### M5.1: Extract Reusable Execution Context Service

What to implement:
Factor sync-before-exec, interpreter lookup, environment lookup, and child
process setup into one execution service used by `run` and later runtime
commands.

Where in codebase:

- new execution module under `crates/pyra-project/src/`
- `crates/pyra-project/src/service.rs`

Why this matters:
Future runtime features must not duplicate environment and interpreter logic.

Dependencies:
M2.5
- M3.3

Acceptance criteria:

- one execution service owns project discovery and sync-before-exec behavior
- `pyra run` is refactored to use that service without changing behavior

Tests required:

- unit tests for execution context assembly
- CLI regression tests proving `run` behavior is unchanged after refactor

#### M5.2: Expand Script And Console Execution

What to implement:
Broaden execution support for console entrypoints and script files while keeping
the same synchronized environment model.

Where in codebase:

- execution module under `crates/pyra-project/src/`
- `crates/pyra-cli/src/cli/mod.rs`
- `crates/pyra-cli/src/commands/run.rs`

Why this matters:
Runtime usefulness starts with stable execution, not a second package model.

Dependencies:
M5.1

Acceptance criteria:

- `pyra run script.py` behaves predictably through the centralized environment
- console scripts resolve from installed packages in the synchronized
  environment
- argument passthrough is stable

Tests required:

- script execution integration test
- console script integration test
- argument passthrough integration test

#### M5.3: Add Thin Runtime Commands

What to implement:
Add first thin runtime commands such as `pyra test` and notebook-kernel setup on
top of the execution service.

Where in codebase:

- `crates/pyra-cli/src/cli/mod.rs`
- new runtime command modules under `crates/pyra-cli/src/commands/`
- execution helpers in `crates/pyra-project/src/`

Why this matters:
This validates that the package-manager foundation can support runtime work
without architectural drift.

Dependencies:
M5.1
- M5.2

Acceptance criteria:

- runtime commands are thin wrappers over execution
- no new environment model or ad hoc install path is introduced

Tests required:

- CLI test for thin `pyra test` dispatch
- integration test for notebook kernel registration or launch plumbing

## Critical Path

These tasks block the rest of the roadmap:

1. M1.1 blocks all resolver hardening work.
2. M1.2, M1.3, M1.4, M1.5, and M1.6 are the non-negotiable correctness core.
3. M1.8 must land before phase-1 package-manager claims are credible.
4. M2.1 must land before CI-facing package-manager workflows are complete.
5. M2.2 must land before M2.3 and M2.4.
6. M4 work must not begin before M1 freshness and selection logic are stable.

Non-negotiable order:

1. M1.1
2. M1.2 and M1.3
3. M1.4
4. M1.5
5. M1.6
6. M1.7
7. M1.8
8. M2.1
9. M2.2
10. M2.3 and M2.4
11. M2.5
12. M3
13. M4
14. M5

Parallelizable later:

- M1.2 and M1.3 after M1.1
- M2.3 and M2.4 after M2.2
- M3.1 and M3.4 once M1.8 is complete

## Sync Hardening Plan

Production-grade sync means all of the following guarantees are true together:

1. lock reuse is truthful
2. interpreter constraints are enforced
3. lock selection is exact
4. installation integrity is verified
5. environment inspection does not depend on pip behavior
6. failures identify the correct layer

### Guarantee 1: Lock Reuse Is Truthful

Required features:

- typed freshness inputs
- interpreter version comparison
- target triple comparison
- index URL comparison
- resolution strategy comparison
- dependency fingerprint comparison

If skipped:
Pyra can silently reuse stale locks or regenerate locks unnecessarily.

### Guarantee 2: Interpreter Constraints Are Enforced

Required features:

- project `requires-python` enforcement
- package `requires-python` filtering during resolution

If skipped:
Pyra can resolve and install an environment that the project does not claim to
support.

### Guarantee 3: Lock Selection Is Exact

Required features:

- correct root membership in the resolver
- parsed lock marker evaluation in install selection
- exact reconciliation planning

If skipped:
Pyra can install the wrong subset or leave incorrect packages in the
environment.

### Guarantee 4: Installation Integrity Is Verified

Required features:

- artifact hash recording in the lock
- artifact download before install
- hash verification before install
- local verified install path

If skipped:
`pylock.toml` does not actually protect the applied environment.

### Guarantee 5: Environment Inspection Does Not Depend On pip

Required features:

- inspect distributions through Python metadata
- normalize names and versions consistently

If skipped:
exact sync can fail because pip inspection is unhealthy even when the
environment itself is usable.

### Guarantee 6: Failures Identify The Correct Layer

Required features:

- distinct error paths for input parsing
- lock parse and freshness errors
- resolution errors
- environment inspection errors
- install and removal errors
- editable install errors

If skipped:
users cannot tell whether the problem is in the project input, lock, resolver,
environment, or installer layer.

Pyra should not be called production-grade on `sync` until all six guarantees
are implemented and covered by tests.

## Resolver Strategy

Minimum viable resolver correctness:

1. correct supported PEP 440 range handling
2. correct marker evaluation for the supported resolver environment
3. correct wheel and sdist compatibility filtering for supported targets
4. correct package `requires-python` filtering
5. correct root-token membership for later lock selection
6. deterministic behavior under hermetic fixtures

Must be implemented before resolver viability:

- M1.1 fixture harness
- M1.2 version and wheel tests
- M1.3 marker and root-membership tests
- M1.4 project interpreter enforcement
- M3.4 regression corpus

Can be deferred:

- direct URL requirements
- forked resolution for independently valid but mutually incompatible optional
  scopes
- exhaustive wheel-tag support beyond supported targets
- universal multi-platform solving in one graph
- pip-free native installer work

Incremental improvement rule:
Only broaden resolver guarantees when the new behavior is covered by hermetic
tests and the lock strategy identifier changes when lock assumptions broaden.

## Cross-Platform Plan

### Phase 1: Current Single-Platform Model

Lock format:

- one interpreter version
- one target triple
- one environment slice
- strategy identifier `current-platform-union-v1`

Resolver:

- solve one union graph for current interpreter and current platform

Install:

- install current-host subset only

Freshness:

- compare fingerprint, interpreter, target, index, and strategy

Risks:

- lock is only truthful for one host target

### Phase 2: Platform-Aware Lock

Lock format:

- add explicit environment identifiers
- package entries carry environment membership
- introduce a new strategy identifier

Resolver:

- resolve one target environment at a time
- merge results into one lock

Install:

- select only current environment id plus chosen groups and extras

Freshness:

- compare target set and per-environment metadata, not just one target triple

Risks:

- schema churn if introduced before current freshness and selection logic is
  stable
- lock growth and more complex conflict reporting

### Phase 3: Multi-Environment Lock

Lock format:

- record a target matrix and multiple environment slices in one lock

Resolver:

- solve each matrix cell independently first
- deduplicate only after correctness is established

Install:

- install only the host cell from the shared lock

Freshness:

- compare the full target matrix fingerprint and strategy identifier

Risks:

- larger lockfiles
- merge noise
- harder user-facing error reporting if target failures differ

## CLI Evolution Plan

Final command set before broader runtime expansion:

1. `pyra python list`
2. `pyra python search`
3. `pyra python install`
4. `pyra python uninstall`
5. `pyra init`
6. `pyra use`
7. `pyra sync`
8. `pyra add`
9. `pyra remove`
10. `pyra run`

Build order:

1. stabilize `sync`
2. add `--locked` and `--frozen`
3. add `add`
4. add `remove`
5. add minimal `run`
6. add thin runtime commands later

Intentionally excluded until the package-manager core is done:

- direct `install <package>` that bypasses `pyproject.toml`
- separate shell or temp-env workflows
- publish or build orchestration expansion
- notebook-heavy UX before execution plumbing is stable

## Documentation Plan

Create:

- `docs/reproducibility-modes.md` during M2
- `docs/package-cache-model.md` during M3
- `docs/platform-locking.md` during M4

Update in M1:

- `docs/sync-model.md`
- `docs/lock-format-notes.md`
- `docs/resolution-scope.md`
- `docs/installer-boundary.md`
- `docs/error-model.md`
- `docs/testing-strategy.md`
- `docs/pyproject-contract.md`

Update in M2:

- `docs/add-remove-model.md`
- `docs/run-model.md`
- `docs/sync-model.md`
- `docs/error-model.md`

Update in M3:

- `docs/environment-model.md`
- `docs/installer-boundary.md`
- `docs/testing-strategy.md`
- `docs/error-model.md`

Update in M4:

- `docs/lock-format-notes.md`
- `docs/resolution-scope.md`
- `docs/sync-model.md`
- `docs/pyproject-contract.md`

Update in M5:

- `docs/run-model.md`
- `docs/environment-model.md`

## 30 / 60 / 90 Day Plan

### 30 Days

Finish M1:

- resolver fixture harness
- resolver correctness tests
- project `requires-python` enforcement
- complete freshness model
- parsed lock marker evaluator
- environment inspection without `pip list`
- verified artifact install path

### 60 Days

Finish M2:

- `sync --locked`
- `sync --frozen`
- typed manifest mutation helpers
- `pyra add`
- `pyra remove`
- minimal `pyra run`

### 90 Days

Finish M3 and start M4:

- persistent artifact cache
- parallel artifact preparation
- interpreter lifecycle hardening
- resolver regression corpus and conflict UX
- environment-scoped lock schema
- initial multi-target lock generation behind an explicit new strategy

This sequence is intended to be realistic for one developer.

## Definition Of Done

Pyra is a real package manager when all of the following are true:

1. `pyra sync` is deterministic on its documented supported scope.
2. `pyra sync` installs only verified artifacts from `pylock.toml`.
3. lock freshness is complete and truthful.
4. project and package interpreter constraints are enforced.
5. exact reconciliation does not depend on `pip list`.
6. resolver behavior is protected by hermetic tests and regression fixtures.
7. `pyra sync --locked` is reliable in CI.
8. `pyra add` and `pyra remove` mutate `pyproject.toml` and then reuse sync.
9. `pyra run` executes from the centralized synchronized environment.
10. docs and code describe the same contracts.

Pyra should not expand aggressively into runtime features until M1, M2, and M3
are complete and stable.
