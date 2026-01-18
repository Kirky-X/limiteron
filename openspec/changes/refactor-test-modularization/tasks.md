## 1. Analysis and Planning
- [x] 1.1 Analyze existing test files and categorize them by functionality
- [x] 1.2 Design new modular directory structure
- [x] 1.3 Create migration plan for each test file

## 2. Create New Directory Structure
- [x] 2.1 Create `tests/modules/` directory
- [x] 2.2 Create module subdirectories: `storage/`, `limiters/`, `ban_manager/`, `quota/`, `circuit_breaker/`, `matchers/`, `governor/`, `cache/`
- [x] 2.3 Create `unit/` subdirectories for each module

## 3. Migrate Storage Tests
- [x] 3.1 Move mock storage tests from `common_tests.rs` to `tests/modules/storage/unit/memory.rs`
- [x] 3.2 Move PostgreSQL tests from `integration/postgres_test.rs` to `tests/modules/storage/unit/postgres.rs`
- [x] 3.3 Move Redis tests from `integration/redis_test.rs` to `tests/modules/storage/unit/redis.rs`
- [x] 3.4 Create `tests/modules/storage/integration.rs` for storage integration tests
- [x] 3.5 Create `tests/modules/storage/mod.rs` to export all storage tests

## 4. Migrate Limiter Tests
- [x] 4.1 Extract limiter tests from source files to `tests/modules/limiters/unit/`
- [x] 4.2 Create `tests/modules/limiters/integration.rs`
- [x] 4.3 Create `tests/modules/limiters/mod.rs`

## 5. Migrate Ban Manager Tests
- [x] 5.1 Extract ban manager tests to `tests/modules/ban_manager/unit/`
- [x] 5.2 Create `tests/modules/ban_manager/integration.rs`
- [x] 5.3 Create `tests/modules/ban_manager/mod.rs`

## 6. Migrate Quota Controller Tests
- [x] 6.1 Extract quota tests to `tests/modules/quota/unit/`
- [x] 6.2 Create `tests/modules/quota/integration.rs`
- [x] 6.3 Create `tests/modules/quota/mod.rs`

## 7. Migrate Circuit Breaker Tests
- [x] 7.1 Extract circuit breaker tests to `tests/modules/circuit_breaker/unit/`
- [x] 7.2 Create `tests/modules/circuit_breaker/integration.rs`
- [x] 7.3 Create `tests/modules/circuit_breaker/mod.rs`

## 8. Migrate Matcher Tests
- [x] 8.1 Extract matcher tests to `tests/modules/matchers/unit/`
- [x] 8.2 Create `tests/modules/matchers/integration.rs`
- [x] 8.3 Create `tests/modules/matchers/mod.rs`

## 9. Migrate Governor Tests
- [x] 9.1 Extract governor tests to `tests/modules/governor/unit/`
- [x] 9.2 Create `tests/modules/governor/integration.rs`
- [x] 9.3 Create `tests/modules/governor/mod.rs`

## 10. Migrate Cache Tests
- [x] 10.1 Extract cache tests to `tests/modules/cache/unit/`
- [x] 10.2 Create `tests/modules/cache/integration.rs`
- [x] 10.3 Create `tests/modules/cache/mod.rs`

## 11. Update Test Entry Files
- [x] 11.1 Update `tests/common_tests.rs` to use new modular structure
- [x] 11.2 Update `tests/integration_tests.rs` to use new modular structure
- [x] 11.3 Update `tests/e2e_tests.rs` to preserve e2e tests

## 12. Create Module Root
- [x] 12.1 Create `tests/modules/mod.rs` to export all test modules

## 13. Verification
- [x] 13.1 Run `cargo check --all-features` to ensure all tests compile
- [ ] 13.2 Run `cargo test --all-features` to ensure all tests pass
- [ ] 13.3 Run `cargo test --test integration_tests` to verify integration tests
- [ ] 13.4 Run `cargo test --test e2e_tests` to verify e2e tests
- [ ] 13.5 Verify test coverage remains the same or improves

## 14. Documentation
- [ ] 14.1 Update `docs/USER_GUIDE.md` with new test structure
- [x] 14.2 Update `IFLOW.md` with new test structure
- [ ] 14.3 Update README.md with test structure information