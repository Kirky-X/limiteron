# Change: Refactor Test Suite into Modular Hierarchy

## Why
Current test structure lacks clear organization and separation of concerns. Tests are scattered across multiple entry files without a logical grouping by functionality. This makes it difficult to:
- Quickly locate tests for specific components
- Understand the testing scope for each module
- Maintain and extend the test suite
- Identify missing test coverage

## What Changes
- Reorganize `tests/` directory into a modular hierarchy based on functional components
- Create separate modules for each core component (storage, limiters, ban_manager, quota, circuit_breaker, matchers, governor, cache)
- Each module contains:
  - `integration.rs` - Integration tests for the component
  - `unit/` directory - Unit tests for the component's internal logic
- Preserve existing `e2e/`, `common/`, `fixtures/`, and `benches/` directories
- Update test entry files to use the new modular structure

## Impact
- Affected specs: `testing`
- Affected code: `tests/` directory structure and all test files
- Migration: Existing tests will be moved to new locations, no functional changes to test logic