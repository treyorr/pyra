# Task Execution Prompt

```text
You are working on exactly one roadmap task for Pyra: <TASK_ID>.

Read first, in order:
1. AGENTS.md
2. docs/README.md
3. docs/product-direction.md
4. docs/backlog/<initiative-id>/execution-roadmap.md
5. any task-specific docs referenced by the target task

Operating mode:
- preserve architecture boundaries
- keep changes narrow and intentional
- do not work on later tasks
- update docs only if behavior/contract changes
- prefer deterministic tests and local fixtures

Before coding:
1. Restate the task.
2. List files to inspect.
3. List files to change.
4. Quote acceptance criteria for <TASK_ID>.
5. State out-of-scope.

Then implement.

Testing requirements:
- Add tests required by the task.
- Run relevant tests.
- If any test cannot be run, state exact reason.

Stop condition:
- Stop when <TASK_ID> is complete.
- Do not start next task automatically.

Final response format:
1. Task completed: yes/no
2. Files changed
3. What was implemented
4. Acceptance criteria status (one line per criterion)
5. Tests added
6. Tests run and results
7. Docs updated
8. Remaining risks or follow-ups
9. Next recommended task
```
