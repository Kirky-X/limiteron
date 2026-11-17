# 🧪 Limiteron Testing Guide

This document explains how to run the Limiteron project's tests, including detailed instructions for running tests per feature. For the test pyramid baseline and E2E scenario definitions see [Test Scenarios](TEST_SCENARIOS.md).

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [🚀 Quick Start](#-quick-start)
- [🎨 Running Tests per Feature](#-running-tests-per-feature)
- [🧱 Test Layout](#-test-layout)
- [📊 Test Count Baseline](#-test-count-baseline)
- [📈 Test Coverage](#-test-coverage)
- [✅ Testing Best Practices](#-testing-best-practices)
- [🤖 CI Integration](#-ci-integration)
- [🔧 Troubleshooting](#-troubleshooting)
- [📚 More Information](#-more-information)
- [🧪 Test Statistics](#-test-statistics)

</details>

## 🚀 Quick Start

### Common Test Commands

```bash
# CI full-suite methodology (same as .github/workflows/ci.yml)
cargo test --workspace --no-default-features --features full

# Library unit tests (full feature)
cargo test --features full --lib

# Unified integration tests (enable features explicitly)
cargo test --test unified_tests --features "ban-manager,quota-control,circuit-breaker"

# Benchmarks
cargo bench --features full
```

> 📌 The `postgres` / `sqlite` / `mysql` storage drivers are mutually exclusive; `--all-features` triggers dbnexus compile errors, so always use explicit feature combinations.

## 🎨 Running Tests per Feature

Limiteron uses feature flags to modularize functionality. How to run tests for each feature:

### Core Features

```bash
# Default features (default = [], core rate limiting)
cargo test --no-default-features

# Ban management
cargo test --features ban-manager
cargo test --test unified_tests --features ban-manager test_list_bans

# Quota control
cargo test --features quota-control
cargo test --test unified_tests --features quota-control test_quota

# Circuit breaker
cargo test --features circuit-breaker
cargo test --test unified_tests --features circuit-breaker test_circuit_breaker
```

### Combined Features

```bash
# Combine multiple features
cargo test --features "ban-manager,quota-control,circuit-breaker"

# Full feature set (full includes postgres, not sqlite/mysql; compiles safely)
cargo test --features full

# Standard feature set (sqlite preset)
cargo test --features standard
```

### Common Features List

| Feature | Description | Test command |
|---------|------|----------|
| `ban-manager` | Ban management | `cargo test --features ban-manager` |
| `quota-control` | Quota control | `cargo test --features quota-control` |
| `circuit-breaker` | Circuit breaker | `cargo test --features circuit-breaker` |
| `prometheus` | Prometheus metrics | `cargo test --features prometheus` |
| `telemetry` | Tracing telemetry | `cargo test --features telemetry` |
| `postgres` | PostgreSQL storage | `cargo test --features postgres` |
| `sqlite` | SQLite storage | `cargo test --features sqlite` |
| `cache-storage` | Redis cache backend (via oxcache) | `cargo test --features cache-storage` |
| `distributed` | Distributed rate limiting | `cargo test --features distributed` |
| `gcra` | GCRA rate-limiting algorithm | `cargo test --features gcra` |
| `parallel-checker` | Parallel ban checks | `cargo test --features parallel-checker` |
| `audit-log` | Audit log | `cargo test --features audit-log` |
| `fallback` | Fallback strategies | `cargo test --features fallback` |
| `validation` | Input validation | `cargo test --features validation` |
| `full` | Full feature preset | `cargo test --features full` |

## 🧱 Test Layout

Tests are organized by layer (for detailed layer baselines and scenario definitions see [Test Scenarios](TEST_SCENARIOS.md)):

| Layer | Location | Notes |
|------|---------|------|
| Unit tests | inline `#[cfg(test)]` in `src/**` | self-tests of governor, limiters, ban, quota, circuit, fallback, matchers, etc. |
| Integration tests | `tests/` top-level targets and subdirectory modules | `unified_tests`, `integration_tests`, `e2e_tests`, `common_tests`, `security_tests`, `admin_security_tests`, `chaos_tests`, `e2e_advanced`, `probes_e2e`, `otlp_export_tests`, etc. |
| Property tests | `tests/property_tests/` | proptest: concurrency, fixed window, sliding window, token bucket |
| Doc tests | doc-comment code blocks | compiled and run with `cargo test` |
| Benchmarks | `benches/` | criterion: throughput / latency / memory / regression |

```bash
# Run the traditional test entry points
cargo test --test common_tests
cargo test --test integration_tests
cargo test --test e2e_tests

# Run unit tests of a specific in-library module
cargo test --lib test_token_bucket
cargo test --lib test_sliding_window
cargo test --lib test_fixed_window
cargo test --lib test_concurrency_limiter
```

## 📊 Test Count Baseline

The count baseline of full CI-methodology runs (`--workspace --no-default-features --features full`; targets / passed / failed / ignored and the composition of ignored) is governed by [Test Scenarios](TEST_SCENARIOS.md#-test-pyramid-baseline).

> 📌 Test counts evolve with versions; defer to the output of the most recent full CI run.

## 📈 Test Coverage

### Coverage Gate

CI and the pre-push hook both enforce the **cargo-llvm-cov line coverage ≥ 90%** gate (see the [CI configuration](../../.github/workflows/ci.yml)):

```bash
# Exactly the CI methodology
cargo llvm-cov --workspace --no-default-features --features full --lib --fail-under-lines 90

# Generate an HTML report
cargo llvm-cov --workspace --no-default-features --features full --lib --html
# Report output in target/llvm-cov/html/
```

> Historically cargo-tarpaulin generated coverage (a v0.1.0-era baseline; no separate report is kept anymore — that data is outdated and must not be cited). When using tarpaulin, note the same `postgres` vs `sqlite` exclusivity; do not use `--all-features`.

## ✅ Testing Best Practices

### 1. Preparation Before Running Tests

```bash
# Make sure formatting is correct
cargo fmt --all -- --check

# Run clippy (same methodology as CI)
cargo clippy --all-targets --no-default-features --features full -- -D warnings

# Run compile checks
cargo check --no-default-features --features full
```

### 2. Parallelism and Output Control

```bash
# Speed tests up with multiple threads
cargo test --features full -- --test-threads=4

# Show test output
cargo test --features full -- --show-output
```

### 3. Debugging Failing Tests

```bash
# Run a single test with output
cargo test --features ban-manager test_list_bans_pagination -- --nocapture

# Show prints of a test
cargo test --test unified_tests --features quota-control test_quota_persists_state -- --exact --nocapture

# Run tests with logging enabled
RUST_LOG=debug cargo test --features circuit-breaker -- --nocapture
```

### 4. Controlling the Run Scope

```bash
# Run tests of a specific package
cargo test -p limiteron --features full

# Filter by name
cargo test --features full -- quota
```

## 🤖 CI Integration

CI quality gates are defined in [ci.yml](../../.github/workflows/ci.yml) and include:

| Task | Command methodology |
|------|---------|
| Format | `cargo fmt --all -- --check` |
| Clippy | `cargo clippy --workspace --all-targets --no-default-features --features full -- -D warnings` |
| Check | `cargo check --workspace --no-default-features --features full` and `--no-default-features`, both |
| Test | `cargo test --workspace --no-default-features --features full` |
| Build | ubuntu / macos / windows matrix, full and no-default both |
| Documentation | `cargo doc --workspace --no-deps --no-default-features --features full` |
| Security | `cargo deny check` and `cargo audit` |
| Coverage | `cargo llvm-cov ... --fail-under-lines 90` |

## 🔧 Troubleshooting

### 1. Feature Conflicts

```bash
# When postgres/sqlite/mysql exclusivity breaks compilation, specify a single-driver combination explicitly
cargo test --no-default-features --features "ban-manager,quota-control,sqlite"
```

### 2. Test Timeouts

```bash
# Run serially to investigate timing issues
cargo test --test unified_tests -- --test-threads=1
```

### 3. Out of Memory

```bash
# Reduce parallel test threads
cargo test --features full -- --test-threads=1
```

## 📚 More Information

- [Test Scenarios](TEST_SCENARIOS.md): test pyramid baseline and E2E scenario definitions
- [API Reference](API_REFERENCE.md)
- [User Guide](USER_GUIDE.md)
- [FAQ](FAQ.md)

## 🧪 Test Statistics

**Test scale** (counted by grepping the `#[test]` / `#[tokio::test]` attributes (`grep -rEc '#\[(tokio::)?test\b' --include='*.rs'`), as of v0.3.1):

| Item | Count |
|--------|------|
| In-library test functions (`src/`) | 2,610 (#[test] 1,594 + #[tokio::test] 1,016) |
| External test functions (`tests/`) | 609 (#[test] 160 + #[tokio::test] 449) |
| Macro crate tests (`macros/`) | 47 (#[test]) |
| Integration adapter crate tests (`integrations/`) | 27 (#[test] 11 + #[tokio::test] 16) |
| Property test groups (proptest) | 4 |
