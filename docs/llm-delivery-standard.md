# LLM Delivery Standard

This document defines how Pyra uses LLMs to turn ideas into shippable,
reviewable, and test-verified work.

It exists to prevent ad hoc prompting from drifting architecture, quality, or
scope.

## Purpose

Use this workflow when:

- evaluating new feature ideas
- planning bug-fix initiatives
- executing roadmap tasks with coding agents
- deciding whether a change is ready for public release

## Non-Negotiable Rules

1. Docs-first before behavior changes.
2. One roadmap task per implementation prompt.
3. Explicit acceptance criteria and required tests per task.
4. No task is complete without test evidence.
5. Keep boundaries intact: CLI parsing, orchestration, domain logic, UI.

## Standard Lifecycle

1. Idea intake
2. Roadmap generation
3. Task-by-task execution
4. Contract and regression verification
5. Release and OSS readiness decision

Each phase has an explicit artifact and owner.

## Required Artifacts

For each initiative create a directory:

`docs/backlog/<initiative-id>/`

Minimum files:

- `idea.md`
- `execution-roadmap.md`
- `status.md`
- `release-readiness.md`

Use the templates under `docs/templates/`.

## Task Contract (Required Shape)

Every task in a roadmap must include:

- stable task ID (`M1.1`, `M2.3`, etc.)
- title
- what to implement
- where in codebase
- acceptance criteria
- tests required
- explicit out-of-scope notes when needed

## LLM Prompt Pack

### 1) Idea -> Roadmap Prompt

Use: [`docs/templates/roadmap-generation-prompt.md`](docs/templates/roadmap-generation-prompt.md)
and render output with
[`docs/templates/execution-roadmap-template.md`](docs/templates/execution-roadmap-template.md)

Goal:
Convert one idea brief into an ordered execution roadmap with stable task IDs
and test requirements.

### 2) Single-Task Execution Prompt

Use: [`docs/templates/task-execution-prompt.md`](docs/templates/task-execution-prompt.md)

Goal:
Execute exactly one roadmap task with narrow edits, explicit acceptance
tracking, and test evidence.

### 3) Release Readiness Prompt

Use: [`docs/templates/release-readiness-prompt.md`](docs/templates/release-readiness-prompt.md)

Goal:
Assess binary-level readiness for public announcement with reproducible
evidence and severity-ranked findings.

## Definition Of Done For An Initiative

An initiative is done only when:

1. all planned tasks are complete or intentionally deferred
2. acceptance criteria are satisfied with test proof
3. docs and behavior are aligned
4. release-readiness assessment is recorded
5. open-source readiness checklist has no unresolved blockers

## Governance Rhythm

- Keep exactly one active execution roadmap per initiative.
- Re-prioritize backlog only at explicit review points.
- Do not let implementation prompts invent undocumented tasks.
- If new scope is discovered, create a new task ID and append intentionally.
