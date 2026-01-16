## Context
This design document outlines the technical approach for refactoring two code quality issues identified during code review.

## Goals / Non-Goals

### Goals
- Reduce `list_bans` function complexity by extracting helper methods
- Evaluate and potentially improve L2 cache concurrent access performance
- Maintain backward compatibility and pass all existing tests

### Non-Goals
- No API signature changes
- No database schema changes
- No new external dependencies (use existing DashMap if needed)

## Decisions

### Decision 1: list_bans Function Refactoring Approach
**Selected:** Extract helper methods approach
**Rationale:** 
- Minimal risk compared to complete rewrite
- Preserves existing behavior
- Makes code more testable
- Maintains SQL query structure

**Alternatives considered:**
- Complete rewrite with builder pattern (over-engineering)
- Using a query builder library (new dependency)

### Decision 2: L2 Cache Lock Optimization
**Selected:** Evaluate DashMap vs Mutex with benchmarking
**Rationale:**
- DashMap provides better concurrent access patterns
- Mutex may be sufficient for current load
- Benchmarking ensures we don't over-engineer

**Alternatives considered:**
- Always use DashMap (premature optimization)
- Use RwLock instead of Mutex (minimal improvement)

## Risks / Trade-offs

| Risk | Impact | Mitigation |
|------|--------|------------|
| Regression in query logic | High | Comprehensive test coverage |
| Performance regression in cache | Medium | Benchmark before/after |
| Increased code complexity | Low | Clear method separation |

## Migration Plan

### For list_bans Refactoring
1. Create new helper methods with private visibility
2. Update `list_bans` to call helper methods
3. Run tests to verify behavior
4. Remove unused code after verification

### For L2 Cache Optimization
1. Create benchmark test for concurrent access
2. Implement DashMap-based alternative
3. Run benchmarks to compare
4. Choose better implementation based on data
5. Update production code if DashMap wins

## Open Questions
- [ ] Should we use `parking_lot` or `std::sync::Mutex` for the baseline benchmark?
- [ ] What concurrency level should we benchmark at (100, 1000, 10000 concurrent requests)?
