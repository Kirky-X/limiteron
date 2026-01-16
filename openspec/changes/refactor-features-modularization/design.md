# Design: Features Modularization and Pluggability

## Context

Limiteron 是一个基于 Rust 的统一流量控制框架，当前设计为 monolithic 架构：
- 所有功能模块在编译时强制启用
- 包含 80+ 直接依赖 crate
- 二进制体积约 5-8 MB
- 编译时间长（尤其是 telemetry 和 postgres/redis）

### Current Architecture

```
Governor (主控制器)
    ├── DecisionChain (决策链)
    │   ├── Limiters (限流器)
    │   ├── BanManager (封禁管理)
    │   ├── QuotaController (配额控制)
    │   └── CircuitBreaker (熔断器)
    ├── Matchers (匹配器)
    │   ├── GeoMatcher (MaxMindDB)
    │   ├── DeviceMatcher (Woothee)
    │   └── CustomMatcher
    ├── Storage (存储)
    │   ├── Postgres (sqlx)
    │   ├── Redis (redis-rs)
    │   └── Memory (内联)
    └── Observability (可观测性)
        ├── Telemetry (OpenTelemetry)
        ├── Metrics (Prometheus)
        └── AuditLog
```

### Constraints

- **Rust Edition**: 2021
- **MSRV**: 1.70+
- **Performance**: P99 < 200μs
- **Safety**: No unsafe code, no type suppression
- **Backward Compatibility**: Maintain public API stability where possible

### Stakeholders

- **Library Users**: Need flexible dependency management for different use cases
- **Edge Computing**: Need minimal binary size and fast compilation
- **Enterprise**: Need full-featured implementation with observability
- **Contributors**: Need clear module boundaries for extensibility

## Goals / Non-Goals

### Goals

1. **Minimize Binary Size**: Reduce minimal binary by 60-70% (5-8 MB → 1-2 MB)
2. **Reduce Compile Time**: Improve incremental compile time by 40%
3. **Enable On-Demand Compilation**: Users select features via Cargo features
4. **Maintain Performance**: Zero runtime overhead for disabled features
5. **Simplify Customization**: Support custom implementations via traits
6. **Preserve API Stability**: Existing public APIs remain unchanged (with feature flags)

### Non-Goals

1. **Plugin System**: Runtime dynamic loading (DLLs) - not needed for Rust
2. **Breaking All APIs**: Most APIs remain, only conditionally available
3. **Remove Features**: All existing features remain, just optional
4. **Rewrite Core Logic**: Governor and limiters remain unchanged

## Decisions

### Decision 1: Feature Matrix Design

**What**: Organize features into logical groups with dependencies

**Why**:
- Clear separation of concerns
- Easy to reason about feature combinations
- Prevents invalid combinations (e.g., telemetry without tracing)

**Design**:

```toml
[features]
# Core (always enabled)
default = ["memory"]
memory = []

# Storage (can select multiple)
postgres = ["dep:sqlx"]
redis = ["dep:redis"]

# Advanced features (independent)
ban-manager = []
quota-control = []
circuit-breaker = []
fallback = []

# Matching (independent)
geo-matching = ["dep:maxminddb"]
device-matching = ["dep:woothee"]

# Observability (independent)
telemetry = ["dep:tracing-subscriber", "dep:tracing-opentelemetry"]
monitoring = ["dep:prometheus"]
audit-log = ["telemetry"]

# Tooling (independent)
macros = ["dep:flowguard-macros"]
config-watcher = ["dep:notify"]
webhook = ["dep:reqwest"]

# Presets (for convenience)
minimal = ["memory"]
standard = ["memory", "ban-manager", "quota-control", "circuit-breaker"]
full = ["postgres", "redis", "ban-manager", "quota-control", "circuit-breaker",
         "geo-matching", "device-matching", "telemetry", "monitoring",
         "audit-log", "macros", "config-watcher", "webhook"]
```

**Alternatives Considered**:
1. **Flat Features**: No grouping, just individual flags
   - *Pros*: Simple
   - *Cons*: Hard to manage, no presets, user confusion
2. **Strict Mutability**: Enforce single storage backend
   - *Pros*: Prevents accidental bloat
   - *Cons*: Too restrictive, some use cases need both (caching + persistence)
3. **Selected**: Logical grouping with presets - balances flexibility and usability

---

### Decision 2: Conditional Compilation Strategy

**What**: Use `#[cfg(feature = "...")]` for entire modules

**Why**:
- Zero runtime overhead (code not compiled in)
- Clear at compile-time what's available
- Rust best practice for optional features

**Pattern**:

```rust
// geo_matcher.rs
#[cfg(feature = "geo-matching")]
pub struct GeoMatcher {
    reader: MaxMindReader,
}

#[cfg(feature = "geo-matching")]
impl GeoMatcher {
    pub fn new(database_path: &str) -> Result<Self, FlowGuardError> {
        // Real implementation
    }
}

#[cfg(not(feature = "geo-matching"))]
pub struct GeoMatcher;

#[cfg(not(feature = "geo-matching"))]
impl GeoMatcher {
    pub fn new(_database_path: &str) -> Result<Self, FlowGuardError> {
        Err(FlowGuardError::FeatureNotEnabled("geo-matching"))
    }
}
```

**Alternative Considered**: Runtime feature detection with `if cfg!(feature = "...")`
- *Pros*: Single code path
- *Cons*: Runtime overhead, still compiles dependencies
- *Selected*: Full conditional compilation for zero overhead

---

### Decision 3: No-Op Stub Pattern

**What**: Provide no-op implementations when features are disabled

**Why**:
- Maintains API compatibility
- Prevents compilation errors in user code
- Allows graceful degradation

**Pattern**:

```rust
#[cfg(feature = "circuit-breaker")]
pub type CircuitBreaker = RealCircuitBreaker;

#[cfg(not(feature = "circuit-breaker"))]
pub struct NoOpCircuitBreaker;

#[cfg(not(feature = "circuit-breaker"))]
impl NoOpCircuitBreaker {
    pub async fn check(&self, _key: &str) -> Result<bool, FlowGuardError> {
        Ok(true) // Always allow
    }
}
```

**Alternatives Considered**:
1. **Compile Error**: Fail if feature not used
   - *Pros*: Explicit about missing feature
   - *Cons*: Breaking change, user frustration
2. **Selected**: No-op stub with descriptive error - maintains compatibility

---

### Decision 4: Storage Trait Abstraction

**What**: Keep `Storage` trait always available, implementations optional

**Why**:
- Users can provide custom implementations
- API remains consistent
- Future storage backends can be added easily

**Design**:

```rust
// storage.rs - always compiled
pub trait Storage: Send + Sync {
    async fn get_quota(&self, key: &str) -> Result<Option<QuotaState>, StorageError>;
    async fn set_quota(&self, key: &str, state: QuotaState) -> Result<(), StorageError>;
    // ... other methods
}

// postgres_storage.rs - conditional
#[cfg(feature = "postgres")]
pub struct PostgresStorage { ... }

#[cfg(feature = "postgres")]
impl Storage for PostgresStorage { ... }
```

**Alternative Considered**: Storage trait also conditional
- *Pros*: Simpler, less code
- *Cons*: Breaks custom implementations, less flexible
- *Selected*: Trait always available, implementations conditional

---

### Decision 5: Feature Dependency Management

**What**: Use Rust's feature dependency syntax

**Why**:
- Automatic feature resolution
- Prevents incomplete configurations
- Clear dependency graph

**Pattern**:

```toml
[features]
# Observability depends on telemetry
audit-log = ["telemetry"]

# Telemetry requires multiple dependencies
telemetry = [
    "dep:tracing-subscriber",
    "dep:tracing-opentelemetry",
    "dep:opentelemetry",
    "dep:opentelemetry-jaeger"
]
```

**Considerations**:
- Avoid circular dependencies
- Use weak dependencies where appropriate
- Document feature interactions in README

---

### Decision 6: Backward Compatibility

**What**: Provide `legacy-compat` feature for gradual migration

**Why**:
- Reduces breaking change impact
- Allows gradual adoption
- Gives users time to migrate

**Design**:

```toml
[features]
# Legacy compatibility - enables all features from v0.1.0
legacy-compat = ["postgres", "redis", "telemetry", "macros"]
```

**Migration Path**:

1. **Immediate**: Add `legacy-compat` feature (no code changes needed)
2. **Short-term**: Review enabled features, remove unused ones
3. **Long-term**: Switch to granular features

**Alternative Considered**: No compatibility layer
- *Pros*: Simpler codebase
- *Cons*: High friction for users
- *Selected*: Provide compatibility layer, document deprecation

---

### Decision 7: Runtime Configuration Integration

**What**: Combine compile-time features with runtime config

**Why**:
- Some behavior best configured at runtime (thresholds, timeouts)
- Re-configuration without recompilation
- Separates deployment concerns from implementation

**Pattern**:

```rust
// Config file (YAML)
features:
  ban_manager:
    enabled: true
    auto_ban:
      threshold: 100
      duration: 3600

// Code (check both compile-time and runtime)
#[cfg(feature = "ban-manager")]
{
    if config.features.ban_manager.enabled {
        governor.enable_ban_manager();
    }
}
```

**Alternative Considered**: Compile-time only configuration
- *Pros*: Simpler, less runtime overhead
- *Cons*: Inflexible, requires recompilation
- *Selected*: Hybrid approach - features for capabilities, config for parameters

---

## Risks / Trade-offs

### Risk 1: Feature Combinatorial Explosion

**Risk**: 10+ features → 2^10 = 1024 combinations, impossible to test all

**Impact**: Untested combinations may have bugs

**Mitigation**:
- Focus on key combinations: minimal, standard, full, common subsets
- Use property-based testing for core logic
- Document recommended combinations
- Enable community testing via CI matrix builds on PRs

**Trade-off**: Accept incomplete testing for edge combinations in exchange for flexibility

---

### Risk 2: Breaking Existing User Code

**Risk**: Users relying on default features will see compilation errors

**Impact**: High friction on upgrade

**Mitigation**:
- Provide `legacy-compat` feature
- Create detailed MIGRATION.md
- Issue deprecation warnings in v0.9.x
- Release blog post with migration guide

**Trade-off**: Temporarily increase code complexity (compat shims) to smooth migration

---

### Risk 3: No-Op Implementation Bugs

**Risk**: No-op stubs may have different behavior than real implementations

**Impact**: Silent failures or unexpected behavior

**Mitigation**:
- Return explicit errors (`FeatureNotEnabled`)
- Document no-op behavior clearly
- Add tests that verify correct error returns
- Use static assertions where possible

**Trade-off**: Slightly more complex error handling in exchange for safety

---

### Risk 4: Increased Compile-Time Complexity

**Risk**: More `#[cfg]` directives make code harder to read and maintain

**Impact**: Developer productivity and code clarity

**Mitigation**:
- Use module-level `#[cfg]` where possible
- Keep no-op implementations simple
- Document feature requirements clearly
- Use IDE features (rust-analyzer) to highlight active code

**Trade-off**: Accept some complexity for the benefits of modularity

---

### Risk 5: Dependency Hell

**Risk**: Complex feature dependencies may cause version conflicts

**Impact**: Users unable to compile certain feature combinations

**Mitigation**:
- Use workspace dependencies
- Pin dependency versions carefully
- Test common dependency combinations
- Provide troubleshooting guide

**Trade-off**: More dependency management effort for better compatibility

---

## Migration Plan

### Phase 1: Preparation (Week 1-2)

**Goals**: Setup infrastructure, no user impact

**Steps**:
1. Create feature branch `refactor/features`
2. Setup CI matrix build configuration
3. Write feature specification document
4. Create test suite for all planned features

**Rollback**: Delete branch, no code changes

---

### Phase 2: Core Refactoring (Week 3-4)

**Goals**: Implement core feature system without breaking existing APIs

**Steps**:
1. Refactor Cargo.toml with feature matrix
2. Conditionally compile storage backends
3. Add no-op stubs for all optional modules
4. Update lib.rs conditional exports
5. Run full test suite (all combinations)

**Rollback**: Revert to pre-refactor commit

**Success Criteria**:
- All existing tests pass
- Binary size measured (baseline)
- Compile time measured (baseline)

---

### Phase 3: Gradual Feature Rollout (Week 5-6)

**Goals**: Incrementally enable feature flags

**Steps**:
1. Merge `legacy-compat` feature (release v0.9.0)
2. Document deprecation in CHANGELOG
3. Wait for user feedback (2 weeks)
4. Merge modular feature system (release v1.0.0-rc.1)

**Rollback**: Issue v0.9.1 bugfix, reconsider v1.0.0-rc.1

**Success Criteria**:
- No critical bugs reported
- Migration path tested by early adopters

---

### Phase 4: Stable Release (Week 7-8)

**Goals**: Finalize v1.0.0

**Steps**:
1. Address rc.1 feedback
2. Release v1.0.0-rc.2
3. Final verification testing
4. Release v1.0.0 stable
5. Update all documentation

**Rollback**: Release v1.0.0 with known issues documented

**Success Criteria**:
- All acceptance criteria met
- Migration guide published
- Examples updated

---

### Post-Migration Support

**Ongoing**:
- Monitor GitHub issues for migration problems
- Provide quick fixes for common issues
- Collect feedback on feature usage patterns
- Iterate on feature documentation

---

## Open Questions

1. **Feature Naming Convention**
   - Should we use `geo-matching` or `geo_matching`?
   - Current plan: `kebab-case` for features (Rust convention)
   - **Status**: Decided

2. **Default Feature Set**
   - Should `default` be `memory` only, or include `circuit-breaker`?
   - Current plan: `default = ["memory"]`
   - **Rationale**: Most basic use case is simple rate limiting
   - **Status**: Decided, open for community feedback

3. **Legacy Compat Duration**
   - How long to maintain `legacy-compat` feature?
   - Current plan: 2 minor versions (v1.0.x and v1.1.x)
   - **Status**: Decided, can be extended based on user feedback

4. **Feature Combinations in CI**
   - How many feature combinations to test in CI?
   - Current plan: Test 5-10 key combinations, not full matrix
   - **Rationale**: Full matrix is 1024 combinations, impractical
   - **Status**: Decided, can be expanded later

5. **Custom Implementation Documentation**
   - How much documentation for custom storage/matcher implementations?
   - Current plan: One comprehensive guide + multiple examples
   - **Status**: In progress, will be part of documentation phase

---

## Future Enhancements

### Post-v1.0.0 Roadmap

1. **Additional Storage Backends**
   - MySQL support (via sqlx)
   - MongoDB support (via mongodb crate)
   - Distributed stores (etcd, Consul)

2. **Advanced Matching**
   - Custom regex matchers
   - Lua script matching engine
   - ML-based anomaly detection

3. **Observability Enhancements**
   - OpenTelemetry OTLP exporter
   - Grafana dashboard templates
   - Custom metric exporters

4. **Performance Optimizations**
   - SIMD optimizations for hot paths
   - Zero-copy data structures
   - Profile-guided optimization (PGO)

---

**Design Version**: 1.0
**Last Updated**: 2026-01-11
**Author**: AI Assistant
