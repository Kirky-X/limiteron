## ADDED Requirements

### Requirement: Modular Test Structure
The test suite SHALL be organized into a modular hierarchy where each functional component has its own directory containing unit tests and integration tests.

#### Scenario: Test module organization
- **WHEN** a developer wants to find tests for a specific component
- **THEN** tests are located in `tests/modules/<component>/` directory

#### Scenario: Unit and integration test separation
- **WHEN** tests are organized within a module
- **THEN** unit tests are in `unit/` subdirectory and integration tests are in `integration.rs`

#### Scenario: Module discovery
- **WHEN** a developer lists the `tests/modules/` directory
- **THEN** they see subdirectories for each functional component (storage, limiters, ban_manager, quota, circuit_breaker, matchers, governor, cache)

### Requirement: Storage Module Tests
The storage module SHALL contain tests for all storage backends organized by implementation.

#### Scenario: Storage module structure
- **WHEN** the storage module is viewed
- **THEN** it contains `unit/memory.rs`, `unit/postgres.rs`, `unit/redis.rs`, and `integration.rs`

#### Scenario: Memory storage unit tests
- **WHEN** memory storage tests are run
- **THEN** they test mock storage functionality without external dependencies

#### Scenario: PostgreSQL storage unit tests
- **WHEN** PostgreSQL storage tests are run
- **THEN** they test PostgreSQL-specific storage logic

#### Scenario: Redis storage unit tests
- **WHEN** Redis storage tests are run
- **THEN** they test Redis-specific storage logic

#### Scenario: Storage integration tests
- **WHEN** storage integration tests are run
- **THEN** they test storage interactions with the system

### Requirement: Limiter Module Tests
The limiter module SHALL contain tests for all rate limiting algorithms organized by algorithm type.

#### Scenario: Limiter module structure
- **WHEN** the limiter module is viewed
- **THEN** it contains `unit/token_bucket.rs`, `unit/sliding_window.rs`, `unit/fixed_window.rs`, `unit/concurrency.rs`, and `integration.rs`

#### Scenario: Token bucket tests
- **WHEN** token bucket tests are run
- **THEN** they test token bucket algorithm logic

#### Scenario: Sliding window tests
- **WHEN** sliding window tests are run
- **THEN** they test sliding window algorithm logic

#### Scenario: Fixed window tests
- **WHEN** fixed window tests are run
- **THEN** they test fixed window algorithm logic

#### Scenario: Concurrency tests
- **WHEN** concurrency tests are run
- **THEN** they test concurrent request limiting logic

### Requirement: Ban Manager Module Tests
The ban manager module SHALL contain tests for ban storage and automatic banning logic.

#### Scenario: Ban manager module structure
- **WHEN** the ban manager module is viewed
- **THEN** it contains `unit/ban_storage.rs`, `unit/auto_ban.rs`, and `integration.rs`

#### Scenario: Ban storage tests
- **WHEN** ban storage tests are run
- **THEN** they test ban record storage and retrieval

#### Scenario: Auto ban tests
- **WHEN** auto ban tests are run
- **THEN** they test automatic ban triggering logic

### Requirement: Quota Module Tests
The quota module SHALL contain tests for quota allocation and overdraft functionality.

#### Scenario: Quota module structure
- **WHEN** the quota module is viewed
- **THEN** it contains `unit/quota_allocation.rs`, `unit/overdraft.rs`, and `integration.rs`

#### Scenario: Quota allocation tests
- **WHEN** quota allocation tests are run
- **THEN** they test periodic quota allocation logic

#### Scenario: Overdraft tests
- **WHEN** overdraft tests are run
- **THEN** they test quota overdraft and recovery logic

### Requirement: Circuit Breaker Module Tests
The circuit breaker module SHALL contain tests for circuit breaker state machine and recovery logic.

#### Scenario: Circuit breaker module structure
- **WHEN** the circuit breaker module is viewed
- **THEN** it contains `unit/state_machine.rs`, `unit/recovery.rs`, and `integration.rs`

#### Scenario: State machine tests
- **WHEN** state machine tests are run
- **THEN** they test circuit breaker state transitions (Closed, Open, HalfOpen)

#### Scenario: Recovery tests
- **WHEN** recovery tests are run
- **THEN** they test circuit breaker automatic recovery logic

### Requirement: Matchers Module Tests
The matchers module SHALL contain tests for all matcher types organized by matcher type.

#### Scenario: Matchers module structure
- **WHEN** the matchers module is viewed
- **THEN** it contains `unit/ip_matcher.rs`, `unit/user_matcher.rs`, `unit/device_matcher.rs`, and `integration.rs`

#### Scenario: IP matcher tests
- **WHEN** IP matcher tests are run
- **THEN** they test IP address matching logic

#### Scenario: User matcher tests
- **WHEN** user matcher tests are run
- **THEN** they test user ID matching logic

#### Scenario: Device matcher tests
- **WHEN** device matcher tests are run
- **THEN** they test device ID matching logic

### Requirement: Governor Module Tests
The governor module SHALL contain tests for the main controller and decision chain logic.

#### Scenario: Governor module structure
- **WHEN** the governor module is viewed
- **THEN** it contains `unit/decision_chain.rs` and `integration.rs`

#### Scenario: Decision chain tests
- **WHEN** decision chain tests are run
- **THEN** they test decision chain execution logic

### Requirement: Cache Module Tests
The cache module SHALL contain tests for L2 and L3 cache implementations.

#### Scenario: Cache module structure
- **WHEN** the cache module is viewed
- **THEN** it contains `unit/l2_cache.rs`, `unit/l3_cache.rs`, and `integration.rs`

#### Scenario: L2 cache tests
- **WHEN** L2 cache tests are run
- **THEN** they test L2 cache logic

#### Scenario: L3 cache tests
- **WHEN** L3 cache tests are run
- **THEN** they test L3 cache logic

### Requirement: Preserved E2E Tests
The E2E tests SHALL remain in the `tests/e2e/` directory and test complete workflows spanning multiple components.

#### Scenario: E2E test preservation
- **WHEN** E2E tests are viewed
- **THEN** they remain in `tests/e2e/` directory with existing structure

#### Scenario: Multi-rule cascade test
- **WHEN** multi-rule cascade test is run
- **THEN** it tests multiple rules with different priorities

#### Scenario: Quota overdraft test
- **WHEN** quota overdraft test is run
- **THEN** it tests quota overdraft and recovery workflow

#### Scenario: Rate limit to ban test
- **WHEN** rate limit to ban test is run
- **THEN** it tests automatic ban triggering after rate limit exceeded

### Requirement: Backward Compatibility
Test commands SHALL continue to work as before after the refactoring.

#### Scenario: Cargo test command
- **WHEN** `cargo test --all-features` is run
- **THEN** all tests are discovered and executed

#### Scenario: Integration test command
- **WHEN** `cargo test --test integration_tests` is run
- **THEN** integration tests are executed

#### Scenario: E2E test command
- **WHEN** `cargo test --test e2e_tests` is run
- **THEN** E2E tests are executed

### Requirement: Test Coverage
Test coverage SHALL remain the same or improve after the refactoring.

#### Scenario: Coverage verification
- **WHEN** test coverage is measured before and after refactoring
- **THEN** coverage is equal or higher after refactoring

#### Scenario: Missing test identification
- **WHEN** a developer reviews the new test structure
- **THEN** they can easily identify components with missing tests

### Requirement: Documentation Updates
Project documentation SHALL be updated to reflect the new test structure.

#### Scenario: User guide update
- **WHEN** the USER_GUIDE.md is reviewed
- **THEN** it documents the new modular test structure

#### Scenario: IFLOW.md update
- **WHEN** the IFLOW.md is reviewed
- **THEN** it documents the new test directory structure

#### Scenario: README update
- **WHEN** the README.md is reviewed
- **THEN** it provides information about running tests in the new structure