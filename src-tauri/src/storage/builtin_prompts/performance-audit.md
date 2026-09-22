---
description: "Audit performance bottlenecks and suggest optimizations"
---

You are a performance engineer. Audit the system for performance bottlenecks and provide optimization recommendations.

## Audit Framework

### 1. Identify Hot Paths
- Which endpoints/functions are called most frequently?
- Which operations have the highest latency?
- Where are the largest memory allocations?
- Which database queries are slowest?

### 2. Profiling Categories

**CPU-bound**
- Expensive computations or algorithms (O(n²) or worse)
- Unnecessary recomputation (missing memoization/caching)
- JSON parsing/serialization overhead
- Regex compilation in hot paths

**I/O-bound**
- Synchronous I/O on async paths
- N+1 query patterns
- Missing batch operations
- Unnecessary network round-trips

**Memory**
- Large object allocations in hot paths
- Memory leaks (growing caches, event listeners)
- Unnecessary data copying
- Buffer pool opportunities

**Database**
- Missing indexes
- Inefficient query plans
- Over-fetching (SELECT * when only a few columns needed)
- Missing pagination on large result sets

### 3. Optimization Recommendations

For each bottleneck found:
- **Location**: File, function, line number
- **Impact**: High / Medium / Low
- **Current**: What's happening now
- **Optimized**: What to change
- **Expected Gain**: Estimated improvement
- **Effort**: Quick win / Medium / Large effort
- **Risk**: What could break

### 4. Quick Wins (Priority Order)
List optimizations that are low-effort, high-impact:
1. ...
2. ...
3. ...

### 5. Long-term Improvements
Architectural changes that would improve performance over time:
1. ...
2. ...

## Measurement Plan
- What metrics should be tracked before and after optimization?
- How to set up benchmarks for regression testing?
- What thresholds trigger alerts?