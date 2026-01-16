# Project Context

## Purpose
Unified flow control framework for Rust - providing rate limiting, quota management, ban management, and circuit breaking capabilities. Designed for high-concurrency, low-latency (< 200μs P99) enterprise applications, APIs, and web services.

## Tech Stack
- **Language**: Rust (Edition 2021)
- **Async Runtime**: Tokio 1.35 (rt-multi-thread, macros, time, net, signal)
- **Database**: PostgreSQL (sqlx 0.7), Redis 0.24
- **Concurrency**: DashMap 5.5, Parking_lot 0.12, LRU Cache 0.12
- **Error Handling**: Thiserror + Anyhow
- **Serialization**: Serde (JSON, YAML, TOML)
- **Tracing**: Tracing 0.1 + OpenTelemetry 0.22
- **Metrics**: Prometheus 0.13
- **Proc Macros**: Flow control declarative macro (`#[flow_control]`)
- **Utils**: Chrono (datetime), UUID 1.6, Secrecy 0.8

**Features**:
- `default`: postgres + redis
- `telemetry`: tracing-subscriber
- `webhook`: reqwest

## Project Conventions

### Code Style
- **Formatter**: rustfmt (max_width=100, tab_spaces=4, hard_tabs=false)
- **ABI**: Explicit `extern "C"` required
- **Imports**: Auto-reorder enabled for imports and modules
- **Documentation**: All public APIs must have doc comments with examples
- **Comments**: Chinese for internal documentation, English for API docs
- **Module Organization**: One `pub mod` per logical component in lib.rs

**Naming Conventions**:
- Types: `PascalCase` (e.g., `Governor`, `TokenBucketLimiter`)
- Functions/Variables: `snake_case` (e.g., `check_request`, `ban_storage`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `MAX_COST`, `MAX_RETRY`)
- Type Parameters: `T`, `U`, `E` generics

**Code Organization**:
```
src/
├── lib.rs              # Module exports and re-exports
├── *.rs                # Core modules (governor.rs, limiters.rs, etc.)
├── factory/mod.rs      # Submodule pattern
└── bin/*.rs            # Binary targets
```

### Architecture Patterns

**Core Patterns**:
- **Governor Pattern**: Main controller orchestrating all flow control decisions
- **Decision Chain**: Sequential/parallel decision nodes (ban check → rate limit → quota → circuit breaker)
- **L2/L3 Caching**: Two-tier caching strategy for performance
- **Parallel Ban Checking**: Async parallel checks for multiple ban targets
- **Strategy Pattern**: Trait-based design (`Limiter` trait, `Storage` trait)

**Concurrency Model**:
- `Arc<RwLock<T>>` for shared mutable state
- `Arc<Trait>` for trait objects (storage backends)
- `std::sync::atomic::*` for lock-free counters (SeqCst ordering)
- CAS loops for atomic updates with exponential backoff

**Error Handling**:
- `thiserror` for enum-based error types
- `FlowGuardError` as main error enum with From implementations
- No empty catch blocks; all errors propagated

**Async/Await**:
- All I/O operations are async (Tokio)
- `Pin<Box<dyn Future<Output = ...> + Send + '_>>` for trait object futures
- Proper error propagation with `?` operator

### Testing Strategy

**Test Structure**:
- **Unit Tests**: Inline in source files with `#[cfg(test)] mod tests`
- **Integration Tests**: `tests/integration/` (postgres_test.rs, redis_test.rs)
- **E2E Tests**: `tests/e2e/` (multi_rule_cascade.rs, rate_limit_to_ban.rs)
- **Benchmarks**: `tests/benches/` (throughput.rs, latency.rs)
- **Common Fixtures**: `tests/common/mod.rs`

**Testing Tools**:
- `tokio-test` for async tests
- `criterion` for benchmarks
- `tempfile` for temporary test files
- `tracing-subscriber` for test observability

**Test Requirements**:
- All public APIs must have doc tests
- Core algorithms need concurrent stress tests
- Each limiter type requires basic, exceeds, and concurrency tests

**Test Commands**:
```bash
cargo test --all-features              # All tests
cargo test test_name                   # Specific test
cargo test --test integration_tests    # Integration tests
cargo bench                            # Benchmarks
```

### Git Workflow

**Branching**:
- `main`: Production-ready code
- `feature/*`: New features
- `fix/*`: Bug fixes
- `refactor/*`: Code restructuring

**Commit Messages**:
- Chinese comments for commit messages (project convention)
- Format: `<type>(<scope>): <subject>`
- Types: feat, fix, docs, style, refactor, test, chore

**Pull Requests**:
- Require tests for new functionality
- Must pass `cargo fmt` and `cargo clippy`
- Include examples for new APIs

**Release**:
- Semantic versioning
- Auto-generated changelog
- Version in Cargo.toml (workspace managed)

## Domain Context

**Core Concepts**:
- **Governor**: Main flow control controller
- **Limiter**: Rate limiting algorithm (TokenBucket, FixedWindow, SlidingWindow, Concurrency)
- **BanManager**: IP/user ban with priority and source tracking
- **QuotaController**: Periodic quota allocation with overdraft support
- **CircuitBreaker**: Auto-breaker with Closed/HalfOpen/Open states
- **Identifier**: Extractable request keys (UserId, Ip, Mac, ApiKey, DeviceId)
- **Matcher**: Rule matching engine with composite conditions

**Request Flow**:
1. Extract identifier from RequestContext
2. Check ban status (parallel for multiple targets)
3. Apply decision chain (rate limit → quota → circuit breaker)
4. Return Decision (Allowed/Rejected/Banned)

**Identifier Extraction**:
- `X-User-Id` header → UserId
- `X-API-Key` header → ApiKey
- Connection remote addr → Ip
- `X-Device-Id` header → DeviceId

**Performance Targets**:
- P99 Latency: < 200μs
- Throughput: 500K ops/sec (rate limit), 300K ops/sec (quota), 200K ops/sec (concurrency)

## Important Constraints

**Technical**:
- Must compile with `rustc 1.70+` (MSRV policy)
- No `unsafe` code in new implementations
- No type suppression (`as any`, `@ts-ignore`)
- Must handle all error cases with proper propagation

**Performance**:
- Zero allocation hot path
- Lock-free where possible
- Connection pooling for storage backends

**Security**:
- Input validation for all extracted identifiers
- Parameterized queries for SQL (sqlx compile-time checks)
- No sensitive data in logs (use `secrecy` crate)

**Compatibility**:
- Maintain backward compatibility for public APIs
- Deprecate with clear migration path before removal
- Feature-gate breaking changes

## External Dependencies

**Storage Backends**:
- PostgreSQL 14+ (via sqlx, optional feature `postgres`)
- Redis 6+ (via redis-rs, optional feature `redis`)
- Memory storage (in-memory for testing/development)

**Observability**:
- OpenTelemetry Jaeger exporter (optional, via opentelemetry-jaeger)
- Prometheus metrics endpoint
- Tracing spans for all operations

**Optional**:
- MaxMindDB (geo IP lookup, via maxminddb)
- Woothee (device parsing, via woothee)
- Reqwest (webhooks, via webhook feature)
- Notify (config file watching)
