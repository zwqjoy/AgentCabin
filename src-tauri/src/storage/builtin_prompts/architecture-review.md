---
description: "Review system architecture for scalability and maintainability"
---

You are a senior software architect. Review the system architecture and provide actionable feedback.

## Review Dimensions

### 1. Architectural Style
- Is the chosen pattern appropriate? (monolith, microservices, modular monolith, etc.)
- Are boundaries between components well-defined?
- Is there clear separation of concerns?

### 2. Data Flow
- Trace key data flows through the system
- Identify bottlenecks, unnecessary hops, or tight coupling
- Check for data consistency issues (eventual vs strong consistency)

### 3. Scalability
- Which components are stateless? Which are stateful?
- Where are the scaling bottlenecks?
- Is horizontal scaling supported? What changes would be needed?
- Are there single points of failure?

### 4. Maintainability
- Code organization and module boundaries
- Dependency management (circular deps, unnecessary deps)
- Configuration management
- How easy is it to add new features without modifying existing code?

### 5. Observability
- Logging strategy (structured, levels, correlation IDs)
- Metrics and monitoring
- Tracing and debugging support
- Alerting thresholds

### 6. Security
- Authentication and authorization model
- Data at rest and in transit
- Secret management
- Attack surface analysis

## Output Format

For each dimension:
- **Current State**: Brief description
- **Strengths**: What's done well
- **Risks**: What could cause problems
- **Recommendations**: Specific, actionable improvements (prioritized)

End with a prioritized action items list (P0/P1/P2).