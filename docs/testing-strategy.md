# Testing Strategy

Pyra needs deterministic tests that validate product behavior without depending
on the live Python ecosystem.

Package management code becomes fragile quickly when tests depend on network
timing, mutable upstream indexes, or incidental terminal output.

## Core Rules

Prefer:

- unit tests for parsing, normalization, locking, freshness, and planning
- integration tests for CLI behavior and end-to-end sync flows
- deterministic fixture indexes and metadata
- temp directories for filesystem isolation

Avoid:

- live network access in tests
- real PyPI dependence
- tests that only validate terminal formatting when domain logic is the real target

## Unit Test Targets

Important unit test areas include:

- dependency group normalization
- include-group expansion
- duplicate normalized group detection
- include cycle detection
- project input parsing
- lockfile read and write behavior
- lock freshness checks
- selection semantics
- exact reconciliation planning

These tests should stay close to the domain logic and avoid unnecessary command
execution.

## Integration Test Targets

Important integration test areas include:

- sync default behavior
- group and extra flag behavior
- stale lock regeneration
- current lock reuse
- exact removal of extraneous packages
- invalid project configuration failures
- pinned interpreter requirements

These tests should validate the full CLI path through handlers and services.

## Fixture Policy

Tests for dependency management should use local fixtures, not live indexes.

Preferred patterns:

- filesystem-backed Simple API fixtures
- static metadata fixtures
- local artifact URLs
- stub installer state for exact reconciliation tests

The goal is to exercise real logic while keeping tests hermetic.

## Installer Testing

Pyra's installer boundary should be testable independently of the resolver.

That means tests should be able to:

- inspect a fake installed state
- apply install and removal actions deterministically
- assert final exact state without invoking external network behavior

This is one reason the installer boundary exists.

## Long-Term Direction

As Pyra grows toward a gold-standard package manager and later runtime layer,
tests should continue protecting the core pipeline:

project inputs -> resolve -> lock -> reconcile -> execute

That pipeline should be verifiable in layers and end to end, always without
depending on the public internet.
