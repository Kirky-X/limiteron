## MODIFIED Requirements

### Requirement: Core Rate Limiting Functionality
The system SHALL provide multi-algorithm rate limiting capabilities including token bucket, fixed window, sliding window, and concurrency control. Core functionality SHALL be always available without requiring any optional features.

#### Scenario: Basic token bucket rate limiting
- **WHEN** a TokenBucketLimiter is configured with 100 tokens and 10 refill rate
- **AND** the user makes 5 requests
- **THEN** the first 5 requests SHALL be allowed
- **AND** the 6th request SHALL be allowed after waiting for token refill

#### Scenario: Core module compilation without optional features
- **WHEN** Limiteron is compiled with `default-features = false` and `features = ["memory"]`
- **THEN** the code SHALL compile successfully
- **AND** the binary size SHALL be less than 2 MB
- **AND** all core rate limiting algorithms SHALL be available

#### Scenario: Core functionality with zero optional dependencies
- **WHEN** only core features are enabled (no postgres, redis, telemetry, etc.)
- **THEN** the number of direct dependencies SHALL be ≤ 30 crates
- **AND** the compile time SHALL be reduced by ≥ 40% compared to full-features build
- **AND** all core API functions SHALL work correctly

---

### Requirement: Decision Chain Composition
The system SHALL provide a decision chain mechanism that allows sequential or parallel execution of multiple decision nodes (ban check, rate limit, quota check, circuit breaker). Each node SHALL be independently enable/disable via features.

#### Scenario: Decision chain with only rate limiting
- **WHEN** decision chain is configured with only rate limiter enabled
- **AND** ban_manager, quota_controller, circuit_breaker features are disabled
- **THEN** the decision chain SHALL execute only the rate limiter check
- **AND** no-op stubs for disabled features SHALL return neutral results
- **AND** performance SHALL not be impacted by disabled features

#### Scenario: Decision chain with all features enabled
- **WHEN** all features are enabled (ban-manager, quota-control, circuit-breaker)
- **AND** a request is made
- **THEN** the decision chain SHALL execute all checks in the configured order
- **AND** if any check fails, subsequent checks SHALL be skipped (short-circuit)

#### Scenario: Parallel decision execution
- **WHEN** decision chain is configured for parallel execution
- **AND** multiple checks can run independently
- **THEN** all parallel checks SHALL execute concurrently
- **AND** results SHALL be aggregated based on the configured strategy

---

## ADDED Requirements

### Requirement: Minimal Feature Set
The system SHALL provide a minimal feature set that includes only core rate limiting and memory storage, suitable for edge computing and embedded environments.

#### Scenario: Minimal deployment to edge device
- **WHEN** Limiteron is compiled with `features = ["memory"]`
- **AND** deployed to an edge device with limited resources
- **THEN** the binary size SHALL be ≤ 2 MB
- **AND** memory footprint SHALL be minimal (< 50 MB)
- **AND** all core rate limiting functions SHALL work correctly

#### Scenario: Minimal feature set compilation speed
- **WHEN** developing with only minimal features enabled
- **THEN** incremental compilation SHALL complete in < 5 seconds
- **AND** the build cache SHALL be small and efficient

---

### Requirement: Feature Presets
The system SHALL provide pre-defined feature presets (minimal, standard, full) to simplify common use cases and reduce configuration complexity.

#### Scenario: Using standard preset for API service
- **WHEN** Limiteron is compiled with `features = ["standard"]`
- **THEN** the following features SHALL be enabled: memory, ban-manager, quota-control, circuit-breaker
- **AND** the following features SHALL be disabled: postgres, redis, geo-matching, device-matching, telemetry, monitoring
- **AND** the binary SHALL be suitable for API rate limiting use cases

#### Scenario: Using full preset for enterprise deployment
- **WHEN** Limiteron is compiled with `features = ["full"]`
- **THEN** all features SHALL be enabled
- **AND** the configuration SHALL match the previous v0.1.0 default behavior
- **AND** all advanced features (geo, device, telemetry) SHALL be available
