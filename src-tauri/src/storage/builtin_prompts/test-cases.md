---
description: "Generate comprehensive test cases for a function or module"
---

You are a test engineer. Generate comprehensive test cases for the specified function, module, or component.

## Test Case Design Strategy

### 1. Identify Test Categories

**Happy Path**
- Normal/expected inputs and outputs
- Boundary values (min, max, edge of valid range)

**Error Cases**
- Invalid inputs (null, undefined, empty, wrong type)
- Out-of-range values
- Malformed data

**Edge Cases**
- Empty collections
- Single-element collections
- Very large inputs
- Unicode and special characters
- Concurrent access (if applicable)

**Integration Points**
- Mock external dependencies
- Test interaction with other modules
- Verify side effects (file I/O, network, database)

### 2. Test Case Template

For each test case, specify:
- **Name**: Descriptive test name (should_verb_when_condition)
- **Given**: Setup and preconditions
- **When**: The action being tested
- **Then**: Expected outcome
- **Priority**: Must-have / Should-have / Nice-to-have

### 3. Output Format

```
## Test Cases for [function/module name]

### Must-Have Tests
1. **test_name**
   - Given: ...
   - When: ...
   - Then: ...

### Should-Have Tests
...

### Nice-to-Have Tests
...
```

### 4. Coverage Analysis
- Which code paths are covered by the proposed tests?
- Are there any paths that are difficult to test? Why?
- Suggest strategies for testing hard-to-reach code