---
description: "Generate a conventional commit message from staged changes"
---

Analyze the staged git changes and generate a conventional commit message.

## Rules

1. Use the format: `type(scope): description`
2. Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`, `ci`, `build`
3. Scope: the module or component affected (optional but recommended)
4. Description: imperative mood, lowercase, no trailing period, max 72 chars
5. If the change is significant, add a body explaining the "why"

## Steps

1. Run `git diff --cached` to see staged changes
2. Identify the primary type of change
3. Determine the appropriate scope
4. Write a concise description
5. If needed, add a body with bullet points

## Output

```
type(scope): description

- Bullet point explaining key changes
- Another important detail
```