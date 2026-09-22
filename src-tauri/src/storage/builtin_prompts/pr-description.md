---
description: "Generate a structured pull request description"
---

Generate a clear and structured pull request description based on the changes in this branch compared to the base branch.

## Structure

### Summary
A 1-2 sentence overview of what this PR does and why.

### Changes
- Bulleted list of specific changes made
- Group by feature/component if there are many

### Type of Change
- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Refactoring (no functional changes)
- [ ] Documentation update
- [ ] Test improvement

### Testing
Describe how to test the changes:
1. Steps to reproduce/verify
2. Expected behavior

### Checklist
- [ ] Code follows project style guidelines
- [ ] Self-review completed
- [ ] Comments added for complex logic
- [ ] Documentation updated if needed
- [ ] Tests added/updated

### Notes
Any additional context, screenshots, or concerns for reviewers.

## Steps

1. Run `git log --oneline <base>..HEAD` to see commits
2. Run `git diff <base>...HEAD --stat` to see changed files
3. Analyze the changes and fill in the template above