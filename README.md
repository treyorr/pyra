<div align="center">
  <img src="icon.png" width="180" alt="Pyra Logo" />
  <h1>Pyra</h1>
  <p><strong>One tool. Python versions, dependencies, environments, and execution — all managed through one deterministic pipeline.</strong></p>
</div>

<br />

Pyra is a modern Python package and project manager built in Rust. It uses `pyproject.toml` as the source of declared intent and `pylock.toml` as the precise source of installed state. Environments are centralized, deterministic, and exactly reconciled from your lockfile.

Pyra is in active development, but the current command surface is already usable
for project setup, dependency sync, diagnostics, and execution workflows.

## Install

Install Pyra:

```bash
curl -fsSL https://tlo3.com/pyra-install.sh | sh
```

The script downloads the right GitHub Release archive for the current machine,
verifies the published SHA-256 checksum, and installs `pyra` onto your `PATH`.

Update Pyra later with:

```bash
pyra self update
```

## Quickstart

```bash
pyra python install 3.13
pyra init --python 3.13
pyra add requests
pyra sync
pyra run main.py
```

## Documentation

Full documentation is tracked in-repository alongside the source code. You can start the local documentation site (using Astro Starlight) by running:

```bash
mise run docs:install
mise run docs:dev
```

Alternatively, you can browse the raw markdown files directly in the [`docs/src/content/docs/`](docs/src/content/docs/) directory.

Useful entry points:

- Public docs: [`docs/src/content/docs/`](docs/src/content/docs/)
- Internal architecture and delivery docs: [`dev-docs/`](dev-docs/)
- Current implementation status: [`docs/src/content/docs/status.md`](docs/src/content/docs/status.md)
- Project roadmap: [`docs/src/content/docs/roadmap.md`](docs/src/content/docs/roadmap.md)

## Core Capabilities

- **Python management** — install, list, and pin Python versions deterministically.
- **Project initialization** — scaffold `pyproject.toml` and `pylock.toml`.
- **Dependency management** — add, remove, and update dependencies across groups and extras.
- **Environment sync** — strict reconciliation from your lockfile to a centralized environment.
- **Execution** — run scripts and commands through the synchronized environment.

## Contributing

Contributions are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md) for the
local workflow, testing commands, and docs-first expectations.

<br />

---
*For internal system architecture boundaries, contracts, and the PIP wrapper specification, refer to the [`dev-docs/`](dev-docs/) directory.*
