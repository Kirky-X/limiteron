# 🧪 Limiteron Test Scenarios

This document pins down Limiteron's test pyramid baseline, test target mapping, E2E scenario definitions (11 domains, 291 tests), combination matrix, and static gates. All verification uses `cargo test` (CI methodology: `--workspace --no-default-features --features full`). For run commands see the [Testing Guide](TESTING.md).

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [🧱 Test Pyramid Baseline](#-test-pyramid-baseline)
- [🎯 Test Target Mapping](#-test-target-mapping)
- [🎬 E2E Scenario Definitions](#-e2e-scenario-definitions)
- [🔀 Combination Matrix](#-combination-matrix)
- [🚧 Static Gates](#-static-gates)

</details>

---

## 🧱 Test Pyramid Baseline

| Layer | Location | Count baseline |
| --- | --- | --- |
| L1 lib unit tests | `#[cfg(test)]` inside `src/**` | 1964 passed / 6 ignored (self-tests of governor/limiters/ban/quota/circuit/fallback/matchers and other modules) |
| L2 integration tests | `tests/**` top-level targets + subdirectory modules | integration_tests 302+14i / e2e_tests 239 / common_tests 213 / unified_tests 211 / security_tests 57 / e2e_advanced 52 / property_tests_runner 24 / chaos_tests 16 / unit_tests 13 / admin_security_tests 6 / on_exceed_modes_test 1 |
| L3 E2E scenarios | `tests/e2e/` (directory-hosted: e2e_tests + e2e_advanced dual targets) | 291 passed (239+52) |
| L4 macros & docs | limiteron_macros (proc-macro self-tests) + Doc-tests | 43 passed / 157 passed + 22 ignored |
| L5 examples | `examples/` workspace member (picked up by --workspace) | 24 empty test targets (compile-level verification) |

Full CI methodology result: **38 targets / 3298 passed / 0 failed / 42 ignored** (ignored = manual/container-dependent gating + doctest gating).

## 🎯 Test Target Mapping

- Top-level auto-discovery (11 entry files, all referencing subdirectory modules via `mod`):
  unit_tests→unit/, integration_tests→integration/+common/+modules/,
  unified_tests→modules/, common_tests→common/+modules/, security_tests→security/,
  chaos_tests→chaos/, property_tests_runner→property_tests/ (incl. .proptest-regressions),
  e2e_tests→e2e/scenarios/, admin_security_tests/on_exceed_modes_test (self-contained)
- Explicit `[[test]]` registration: **e2e_advanced** (located in the `tests/e2e/` directory; e2e_* test targets live uniformly in subdirectories;
  once moved into a subdirectory a target leaves the auto-discovery scope, and failing to register the `[[test]]` target makes its tests silently disappear.
  The file is self-contained with inline mods and no include_str relative references)
- test-clock feature: public `MockClock` gating, not part of production presets; tests/ enable it via a self dev-dep
  (CI needs no explicit addition, no silent-skip risk)

## 🎬 E2E Scenario Definitions

### tests/e2e/scenarios/ (e2e_tests target, 239 tests ×4 business domains)

| Domain | Scenario highlights |
| --- | --- |
| rate_limiting | Governor low-threshold creation → request over limit rejected, full flow / window boundaries / multi-strategy |
| ban_management | violation accumulation → automatic ban → unban flow / ban queries / persistence |
| quota_control | quota allocation → consumption → exhausted rejection → reset period |
| circuit_breaker | failure accumulation → circuit opens → half-open probing → recovery, full state machine |

### tests/e2e/e2e_advanced.rs (e2e_advanced target, 52 tests ×7 mod domains)

| Domain | Scenario highlights |
| --- | --- |
| limiter_boundary | boundary values of each limiter type (zero/max/overflow/negative) |
| gcra_limiter | GCRA burst/sustained-rate semantics (gated by cfg(feature="gcra")) |
| concurrency_limiter | concurrency permit acquire/release/exhaustion queuing |
| fallback_strategy | fallback strategy trigger/switch/recovery |
| multi_tenant | multi-tenant isolation/independent quotas/tenant-level bans |
| distributed_limiter | distributed instance coordination (shared storage semantics) |
| cross_module | cross-module integration (rate limiting + ban + quota + circuit interplay) |

## 🔀 Combination Matrix

| Combination | Coverage | Result |
| --- | --- | --- |
| full (CI methodology) | all 38 targets | 3298 passed / 0 failed |
| no-default-features | minimal-surface compile + test | covered by CI check/build jobs; local fmt/clippy verified with the same methodology |
| standard (sqlite preset) | core rate limiting + ban + quota + circuit | feature-combination compile verification |
| gcra standalone gating | e2e_advanced gcra domain | activated under full (the 52 include gcra cases) |

## 🚧 Static Gates

| Gate | Command methodology | Result |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | clean |
| clippy | `cargo clippy --workspace --all-targets --no-default-features --features full -- -D warnings` | zero warnings |
| doc | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --no-default-features --features full` | zero warnings |
| deny | `cargo deny check` | 4 items ok |
| audit | `cargo audit` | rc=0 |
| Coverage | `cargo llvm-cov --workspace --no-default-features --features full,postgres --lib --fail-under-lines 90` | ≥90% line coverage (CI/pre-push gate) |
| MSRV | rust-version (workspace-pinned 1.97.1) | consistent |
