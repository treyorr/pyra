# AGENTS.md

This file defines the engineering standards for Pyra.

Pyra is a long-term Rust workspace for a modern Python toolchain. The architecture must stay clean as the project grows. Contributors and coding agents must follow these rules strictly.

---

## Project Philosophy

Pyra should feel:

- simple
- fast
- calm
- modern
- opinionated

The codebase should reflect those same qualities:

- clear boundaries
- low coupling
- consistent naming
- direct flow
- minimal noise

Do not optimize for short-term hacking at the expense of long-term clarity.

---

## Docs First Rule

Pyra's `docs/` directory is part of the product architecture, not optional background material.

Before changing behavior, adding commands, or extending an existing subsystem, contributors and coding agents must:

1. read the relevant docs in `docs/`
2. follow the documented model unless the change is intentionally updating that model
3. update the docs in the same change when behavior, boundaries, semantics, or guarantees change

When code and docs appear to disagree, do not silently pick one and move on. Investigate the mismatch, resolve it intentionally, and leave both in sync.

Current high-priority architecture references include:

- `docs/README.md`
- `docs/pyproject-contract.md`
- `docs/environment-model.md`
- `docs/sync-model.md`
- `docs/group-semantics.md`
- `docs/lock-format-notes.md`
- `docs/resolution-scope.md`
- `docs/installer-boundary.md`
- `docs/add-remove-model.md`
- `docs/run-model.md`
- `docs/error-model.md`
- `docs/testing-strategy.md`

Future agents should start with `docs/README.md`, follow its reading map to the relevant contract docs, and treat those docs as the first source of truth for product behavior and architecture before confirming implementation details in code.

---

## Core Architecture Rules

Pyra must maintain strict separation between:

1. CLI parsing
2. command orchestration
3. domain/application logic
4. terminal presentation

These concerns must not be mixed.

### CLI parsing layer
Responsible only for:
- clap structs/enums
- argument parsing
- command routing inputs

Must not contain:
- business logic
- filesystem logic
- formatted terminal output
- dependency resolution logic

### Command orchestration layer
Responsible for:
- taking parsed CLI input
- calling application/domain services
- mapping results into UI output

Must not contain:
- deep business rules
- terminal style definitions
- raw clap parsing types leaking everywhere

### Domain/application layer
Responsible for:
- project logic
- python management logic
- environment logic
- storage logic
- validation and state changes

Must:
- return typed results and typed errors
- remain independent of terminal formatting

Must not:
- print directly
- depend on CLI crate types
- embed presentation logic

### UI/presentation layer
Responsible for:
- terminal output
- styles
- status formatting
- error rendering
- progress indicators

Must be the only place where user-facing terminal formatting is implemented.

---

## Error Handling Rules

Pyra must have strong, typed error handling.

### Requirements
- Use `thiserror` for domain errors
- Reserve `anyhow` for top-level composition only when necessary
- Do not use stringly-typed errors in core logic
- Do not hide important domain failures behind generic error wrappers too early

### User-facing errors must explain:
1. what failed
2. why it failed
3. what the user should do next

### Error categories
At minimum, distinguish between:
- user/input errors
- project state errors
- IO/system errors
- internal invariant violations

### Never
- dump debug output directly to the user in normal mode
- expose internal implementation detail unless useful
- return vague messages like "something went wrong"

---

## Output and Terminal UX Rules

Default terminal output must be:

- concise
- readable
- low-noise
- consistent

### Prefer
- short status lines
- useful empty-state messages
- actionable warnings
- clean success messages

### Avoid
- giant paragraphs
- excessive color
- random emoji
- noisy banners
- tables unless they clearly help

### Verbose mode
Verbose output may include:
- resolved paths
- timing
- internal decisions
- cache details
- subprocess details

Verbose mode must not change behavior, only visibility.

---

## Workspace and Crate Boundaries

Pyra should remain a Rust workspace with small focused crates.

Expected crate responsibilities:

- `pyra-cli`: clap definitions, command dispatch, entrypoint
- `pyra-core`: shared context, paths, config, common types
- `pyra-ui`: output, styles, presentation helpers
- `pyra-errors`: shared error abstractions if needed
- `pyra-python`: python version management domain logic
- `pyra-project`: project initialization and pyproject logic

Do not collapse unrelated logic into one crate for convenience.

Before adding a new crate, ask:
- does this represent a real stable boundary?
- does it reduce coupling?
- is it likely to grow independently?

If not, keep it inside an existing crate.

---

## Module Design Rules

### Prefer
- small modules with clear responsibility
- explicit names
- direct flows
- composition over tangled abstraction

### Avoid
- god modules
- giant util files
- premature generic abstractions
- deeply nested modules without reason

### Naming
Use names that describe responsibility clearly:
- `paths.rs`
- `install.rs`
- `list.rs`
- `renderer.rs`
- `context.rs`

Avoid vague names like:
- `helpers.rs`
- `misc.rs`
- `common.rs` unless truly justified

---

## Documentation and Commenting Rules

Pyra code should be commented consistently. Comments are required because they help future contributors and coding agents understand intent, avoid re-implementing existing behavior, and preserve boundaries as the workspace grows.

### Requirements
- Every module should start with a short comment or doc comment explaining its responsibility.
- Public types and functions should have comments when their purpose, contract, or boundary is not obvious from the name alone.
- Non-obvious logic, normalization rules, filesystem conventions, fallback behavior, and invariants should be explained inline.
- Comments should explain **why** the code exists, what contract it preserves, or what future maintainers must not accidentally change.
- If a change adds a new subsystem, crate, service, or workflow, add or update comments in the touched area as part of the same change.

### Avoid
- comments that merely restate the code line-by-line
- stale comments that no longer match behavior
- large comment blocks that describe plans the code does not actually implement

### Preferred style
- Keep comments short, direct, and maintainership-focused.
- Favor doc comments for module-level and API-level intent.
- Favor inline comments for tricky control flow, normalization, edge cases, and architectural boundaries.

---

## Filesystem and Paths

Pyra manages:
- config
- data
- cache
- state

Use cross-platform standard app directories.
Do not hardcode OS-specific locations.

Prefer UTF-8 safe path handling where practical.

All path resolution should be centralized. Do not scatter path-building logic throughout the codebase.

---

## Config Rules

Configuration should follow a clear precedence model:

1. CLI flags
2. project config
3. global config
4. defaults

Do not implement ad hoc config lookup in random modules.
All config loading and merging logic must be centralized.

Start minimal. Add config only when it has a real use case.

---

## Dependency Rules

Prefer stable, widely used crates.
Do not add dependencies casually.

When adding a dependency, justify:
- what problem it solves
- why standard library is not enough
- why this crate is the right fit long-term

Remove unused dependencies quickly.

Use `mise` for local toolchain installation and for project task running. Keep routine developer commands available as `mise` tasks instead of tribal shell snippets.

---

## Testing Rules

New logic should be written so it can be tested cleanly.

Prefer:
- unit tests for domain logic
- integration tests for CLI behavior where appropriate
- deterministic behavior
- temp directories for filesystem tests

Avoid designs that force tests to exercise terminal output just to validate core logic.

---

## LLM Contribution Rules

When making changes as an automated coding agent:

1. Preserve layer boundaries
2. Do not move presentation logic into domain logic
3. Do not move domain logic into clap structs
4. Do not add shortcut abstractions that weaken architecture
5. Keep changes narrow and intentional
6. Read the relevant files in `docs/` before making architectural or behavioral changes
7. Add or update comments in the code when behavior, boundaries, or intent would otherwise be easy to misread
8. Update docs when architecture changes
9. Keep code and docs aligned; do not leave semantic drift behind
10. Prefer completing one vertical slice cleanly over partially scaffolding many things

When uncertain, choose the simpler structure with clearer boundaries.

---

## Initial Priority Order

When building out Pyra further, prioritize:

1. workspace and CLI foundation
2. app context and storage paths
3. typed error model
4. reusable UI/output model
5. `pyra python list`
6. `pyra python install`
7. `pyra init`

Do not jump to resolver/package complexity before the CLI foundation is solid.

---

## Code Review Standard

Every change should be judged by these questions:

- Is the responsibility clear?
- Is the layer boundary preserved?
- Are comments present where maintainers need context?
- Will this scale as Pyra grows?
- Is the terminal UX consistent?
- Are errors useful?
- Is this easy to extend without rewrite?

If the answer is no, rework it.
