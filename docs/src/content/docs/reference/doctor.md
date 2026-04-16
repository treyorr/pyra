---
title: Doctor
description: Project and environment health diagnostics — what doctor checks and how to read its output.
---

`pyra doctor` is a read-only diagnostics command. It reports project and environment health without changing any state.

## What doctor checks

Doctor evaluates four areas and reports actionable findings:

### Interpreter state

- Is the pinned interpreter (`[tool.pyra].python`) installed?
- Does it satisfy `[project].requires-python` (if present)?
- Is it consistent with the environment metadata?

### Lock state

- Does `pylock.toml` exist?
- Is it fresh for current project inputs (fingerprint, interpreter, target, index, strategy)?

### Environment state

- Does the centralized environment exist?
- Is the environment metadata readable?
- Do installed packages match the selected lock state?

## Read-only guarantee

Doctor never mutates:

- `pyproject.toml`
- `pylock.toml`
- Centralized environment package state
- Environment metadata

It reads all of these inputs to produce diagnostics.

## Output

### Human output

```text
$ pyra doctor

[WARN] Lock file does not match current project inputs.
  > Run 'pyra sync' to update the lock.

[WARN] Environment has 2 packages not in lock.
  > Run 'pyra sync' to reconcile the environment.
```

When no findings are reported, the project is healthy:

```text
$ pyra doctor

[OK] Project is healthy.
```

### JSON output

```bash
pyra doctor --json
```

Returns the standard JSON envelope. Findings appear in `output.findings`:

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

## Exit behavior

| State | Status | Exit code |
|-------|--------|-----------|
| No findings | `success` | `0` |
| Findings present | `warn` | `0` |
| Unexpected failure | `fail` | `2` / `3` / `4` |

Doctor reports warnings with exit code `0` — findings are informational, not failures. This makes doctor safe to run in scripts that check exit codes.

## Corrupt lock handling

If `pylock.toml` exists but cannot be parsed, doctor reports this as a finding rather than crashing. The suggestion directs the user to regenerate the lock with `pyra sync`.

## When to use doctor

- After cloning a repo to check project state
- In CI to verify environment health without running sync
- When debugging unexpected behavior
- After upgrading Pyra to check existing projects
