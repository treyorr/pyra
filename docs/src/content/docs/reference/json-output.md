---
title: JSON Output
description: Machine-readable output envelope, exit codes, and output schema for CI and automation.
---

Pyra supports `--json` as a global flag on every command. When enabled, output is a single JSON object with a deterministic envelope structure.

## Envelope schema

```json
{
  "status": "success",
  "exit": {
    "code": 0,
    "category": "success"
  },
  "output": { ... },
  "error": null
}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `status` | `"success" \| "warn" \| "fail"` | Result category |
| `exit.code` | integer | Process exit code |
| `exit.category` | string | Exit code classification |
| `output` | object \| null | Command-specific output tree |
| `error` | object \| null | Error payload on failure |

### Status values

| Status | Meaning |
|--------|---------|
| `success` | Command completed with exit code `0`, no warnings |
| `warn` | Command completed with exit code `0`, includes warnings |
| `fail` | Command returned a non-zero exit code or error |

## Error payload

When `status` is `"fail"`, the `error` field contains:

```json
{
  "error": {
    "summary": "No managed Python found for version 3.14",
    "detail": "Pyra searched the Python store but found no installation matching '3.14'.",
    "suggestion": "Run 'pyra python install 3.14' to install this version first."
  }
}
```

| Field | Description |
|-------|-------------|
| `summary` | Short error description |
| `detail` | Extended explanation |
| `suggestion` | Actionable next step for the user |

## Exit code categories

| Code | Category | Meaning |
|------|----------|---------|
| `0` | `success` | Successful completion |
| `2` | `user` | User or input error |
| `3` | `system` | IO or system error |
| `4` | `internal` | Internal invariant violation |
| varies | `external` | Child process exit code (from `pyra run`) |

## Determinism

JSON output is deterministic. Given the same inputs, the same envelope structure and field ordering is produced. This makes JSON output suitable for:

- CI assertion scripts
- Dashboard integrations
- Automated dependency management
- Health monitoring

## Examples

### Successful sync

```bash
pyra sync --json
```

```json
{
  "status": "success",
  "exit": { "code": 0, "category": "success" },
  "output": { ... },
  "error": null
}
```

### Doctor with warnings

```bash
pyra doctor --json
```

```json
{
  "status": "warn",
  "exit": { "code": 0, "category": "success" },
  "output": {
    "findings": [
      {
        "kind": "stale_lock",
        "message": "Lock file does not match current project inputs.",
        "suggestion": "Run 'pyra sync' to update the lock."
      }
    ]
  },
  "error": null
}
```

### User error

```bash
pyra sync --locked --json
# (when lock is missing)
```

```json
{
  "status": "fail",
  "exit": { "code": 2, "category": "user" },
  "output": null,
  "error": {
    "summary": "Lock file not found",
    "detail": "pyra sync --locked requires an existing pylock.toml.",
    "suggestion": "Run 'pyra sync' first to generate the lock file."
  }
}
```

## Usage in CI

```bash
# Fail the build if lock is out of date
pyra sync --locked --json | jq -e '.status == "success"'

# Check for outdated packages
pyra outdated --json | jq '.output.packages[]'

# Health check
pyra doctor --json | jq '.output.findings'
```
