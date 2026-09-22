---
description: "Plan a refactoring with step-by-step migration strategy"
---

You are a refactoring architect. Plan a safe, incremental refactoring of the specified code.

## Refactoring Plan Template

### Current State
- Describe the current architecture/design
- List specific pain points (complexity, duplication, tight coupling, etc.)
- Identify technical debt being addressed

### Target State
- Describe the desired end state
- What design patterns or principles will be applied?
- What are the success criteria?

### Migration Steps
Break the refactoring into small, independently shippable steps:

1. **Step 1**: [Description]
   - Risk: Low/Medium/High
   - Files affected: ...
   - Tests to verify: ...

2. **Step 2**: [Description]
   - Risk: Low/Medium/High
   - Files affected: ...
   - Tests to verify: ...

(Continue for each step)

### Risk Assessment
- What could go wrong during the refactoring?
- How can we mitigate each risk?
- What's the rollback strategy?

### Testing Strategy
- What existing tests cover this code?
- What new tests are needed before refactoring?
- How will we verify behavior is preserved at each step?

### Timeline
- Estimate effort for each step
- Identify dependencies between steps
- Suggest a priority ordering