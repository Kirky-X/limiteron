## Context
The current test structure in `tests/` directory has evolved organically over time, resulting in:
- Flat structure with limited organization
- Tests scattered across `common_tests.rs`, `integration_tests.rs`, `e2e_tests.rs`
- Subdirectories `common/`, `integration/`, `e2e/` with unclear separation of concerns
- Difficulty in locating tests for specific components
- No clear distinction between unit and integration tests

**Stakeholders**: Developers, QA engineers, new contributors
**Constraints**: Must maintain backward compatibility for test commands, no changes to test logic

## Goals / Non-Goals

### Goals
- Organize tests by functional component (storage, limiters, ban_manager, quota, circuit_breaker, matchers, governor, cache)
- Create clear separation between unit tests and integration tests
- Improve test discoverability and maintainability
- Make it easy to identify missing test coverage
- Preserve all existing test functionality

### Non-Goals
- Changing test logic or assertions
- Modifying test coverage
- Changing test command interfaces
- Refactoring test implementation (only reorganizing)

## Decisions

### Decision 1: Module-Based Organization
**What**: Organize tests by functional component matching the source code structure
**Why**: Aligns with project architecture, makes it easy to find tests for any component
**Alternatives considered**:
- Organize by test type (unit/integration/e2e) - rejected because components are split across directories
- Organize by feature (rate limiting, banning, etc.) - rejected because features span multiple components

### Decision 2: Two-Level Hierarchy
**What**: Each module has `integration.rs` and `unit/` subdirectory
**Why**: Clear separation of concerns while keeping related tests together
**Alternatives considered**:
- Single level (all tests in module root) - rejected because unit and integration tests mixed
- Three-level (unit/integration/e2e per module) - rejected because e2e tests are cross-component

### Decision 3: Preserving E2E Tests
**What**: Keep `e2e/` directory unchanged at root level
**Why**: E2E tests span multiple components and don't belong to any single module
**Alternatives considered**:
- Move e2e tests to `modules/e2e/` - rejected because e2e tests are fundamentally different
- Distribute e2e tests across modules - rejected because they test cross-component scenarios

### Decision 4: Module Naming Convention
**What**: Use kebab-case for module directories matching source code naming
**Why**: Consistent with Rust conventions and project style
**Alternatives considered**:
- Use snake_case - rejected because kebab-case is more readable for directories
- Use CamelCase - rejected because inconsistent with Rust conventions

## Directory Structure

```
tests/
├── modules/                    # Functional modules
│   ├── storage/               # Storage backend tests
│   │   ├── mod.rs             # Module exports
│   │   ├── integration.rs     # Integration tests (storage + backend)
│   │   └── unit/              # Unit tests (storage logic)
│   │       ├── mod.rs
│   │       ├── memory.rs
│   │       ├── postgres.rs
│   │       └── redis.rs
│   ├── limiters/              # Rate limiter tests
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── token_bucket.rs
│   │       ├── sliding_window.rs
│   │       ├── fixed_window.rs
│   │       └── concurrency.rs
│   ├── ban_manager/           # Ban management tests
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── ban_storage.rs
│   │       └── auto_ban.rs
│   ├── quota/                 # Quota control tests
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── quota_allocation.rs
│   │       └── overdraft.rs
│   ├── circuit_breaker/       # Circuit breaker tests
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── state_machine.rs
│   │       └── recovery.rs
│   ├── matchers/              # Matcher tests
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── ip_matcher.rs
│   │       ├── user_matcher.rs
│   │       └── device_matcher.rs
│   ├── governor/              # Governor controller tests
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       └── decision_chain.rs
│   └── cache/                 # Cache tests
│       ├── mod.rs
│       ├── integration.rs
│       └── unit/
│           ├── mod.rs
│           ├── l2_cache.rs
│           └── l3_cache.rs
├── e2e/                       # End-to-end tests (unchanged)
│   ├── mod.rs
│   ├── multi_rule_cascade.rs
│   ├── quota_overdraft.rs
│   └── rate_limit_to_ban.rs
├── common/                    # Common test utilities (unchanged)
│   └── mod.rs
├── fixtures/                  # Test fixtures (unchanged)
│   ├── valid_config.yaml
│   └── invalid_config.yaml
├── benches/                   # Benchmarks (unchanged)
│   ├── latency.rs
│   └── throughput.rs
├── common_tests.rs            # Updated to use modules
├── integration_tests.rs       # Updated to use modules
└── e2e_tests.rs               # Preserved
```

## Migration Plan

### Phase 1: Preparation
1. Analyze all existing test files
2. Create new directory structure
3. Document migration mapping

### Phase 2: Migration (per module)
For each module (storage, limiters, ban_manager, quota, circuit_breaker, matchers, governor, cache):
1. Create module directory and subdirectories
2. Move relevant tests to `unit/` subdirectory
3. Create `integration.rs` for integration tests
4. Create `mod.rs` to export tests
5. Verify tests pass

### Phase 3: Integration
1. Update test entry files to use new modules
2. Create root `modules/mod.rs`
3. Run full test suite
4. Verify test coverage

### Phase 4: Documentation
1. Update project documentation
2. Update IFLOW.md
3. Update README.md

### Rollback Plan
If migration fails:
1. Keep original test files in place
2. Revert changes to test entry files
3. Document issues and retry

## Risks / Trade-offs

### Risk 1: Test Command Changes
**Risk**: Existing test commands may break
**Mitigation**: Ensure backward compatibility by maintaining test entry files
**Probability**: Low

### Risk 2: Test Discovery Issues
**Risk**: Cargo may not discover tests in new structure
**Mitigation**: Verify `mod.rs` files correctly export all tests
**Probability**: Low

### Risk 3: Test Dependency Issues
**Risk**: Tests may have dependencies that break when moved
**Mitigation**: Run tests after each module migration, fix dependencies immediately
**Probability**: Medium

### Risk 4: Increased Complexity
**Risk**: New structure may be more complex for new contributors
**Mitigation**: Provide clear documentation and examples in README
**Probability**: Low

### Trade-off: More Directories
**Decision**: More directories vs. better organization
**Rationale**: Improved organization outweighs additional directory depth

## Open Questions

1. Should we create separate test modules for `decision_chain` tests?
   - **Decision**: No, keep them in `governor/` module as they are closely related

2. Should we create separate test modules for `telemetry` tests?
   - **Decision**: Yes, add `telemetry/` module if telemetry tests exist

3. Should we preserve `common_tests.rs` or remove it?
   - **Decision**: Preserve it but update to use new modules for backward compatibility

4. Should we add test coverage reporting?
   - **Decision**: Out of scope for this refactor, consider in future

## Implementation Notes

### Module Exports
Each `mod.rs` should follow this pattern:
```rust
mod integration;
mod unit;

pub use integration::*;
pub use unit::*;
```

### Test Organization Criteria
- **Unit tests**: Test internal logic of a component without external dependencies
- **Integration tests**: Test component interactions with storage, other components, or external services
- **E2E tests**: Test complete workflows spanning multiple components

### Feature Gates
Maintain existing feature gates (`#[cfg(feature = "...")]`) when moving tests

### Test Naming
Use descriptive test names following the pattern: `test_<component>_<scenario>`

## Success Criteria

1. All existing tests pass in new structure
2. Test coverage remains the same or improves
3. Test commands (`cargo test`, `cargo test --test integration_tests`, etc.) work as before
4. Documentation accurately reflects new structure
5. New contributors can easily find and run tests for specific components