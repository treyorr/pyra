# Roadmap Generation Prompt

```text
You are planning implementation for Pyra.

Input:
- Idea brief: <paste docs/backlog/<initiative-id>/idea.md>
- Product direction: docs/product-direction.md
- Existing architecture contracts under docs/

Output:
Create docs/backlog/<initiative-id>/execution-roadmap.md.

Rules:
1. Stay within Pyra architecture boundaries.
2. Prefer minimal, high-leverage scope first.
3. Use milestones and stable task IDs (M1.1, M1.2, ...).
4. For each task include:
   - What to implement
   - Where in codebase
   - Acceptance criteria
   - Tests required
5. Include explicit non-goals and deferred items.
6. Include a critical-path ordering.
7. Include definition of done.
8. Keep language concrete and testable.

Quality bar:
- No vague tasks
- No missing test requirements
- No architecture-violating shortcuts
- No hidden dependency on future tasks
```
