<div align="center">

<img src="docs/assets/limiteron.png" alt="Limiteron Logo" width="180">

[![CI Status](https://github.com/Kirky-X/limiteron/actions/workflows/ci.yml/badge.svg)](https://github.com/Kirky-X/limiteron/actions/workflows/ci.yml) [![Version](https://img.shields.io/crates/v/limiteron.svg)](https://crates.io/crates/limiteron) [![Docs.rs](https://docs.rs/limiteron/badge.svg)](https://docs.rs/limiteron) [![Downloads](https://img.shields.io/crates/d/limiteron.svg)](https://crates.io/crates/limiteron) [![License](https://img.shields.io/crates/l/limiteron.svg)](LICENSE) [![Rust](https://img.shields.io/badge/rust-1.97.1%2B-orange.svg)](https://www.rust-lang.org/)

[中文](README.md) | **English**

**Unified Flow Control Framework for Rust**

[✨ Features](#-features) • [🚀 Quick Start](#-quick-start) • [📚 Documentation](#-documentation) • [💻 Examples](#-examples) • [🤝 Contributing](#-contributing)

</div>

---

<div align="center" style="padding: 32px; margin: 24px 0">

### 🚦 Unified Traffic Control

One Governor entrypoint where multi-algorithm limiting and layered control decide together:

<table style="width:100%; border-collapse: collapse">
<tr><td align="center" width="25%" style="padding: 12px">🚦<br><b>Multi-Algorithm Limiting</b><br><span style="color:#64748B">token bucket, sliding/fixed window, concurrency, GCRA, HTB</span></td><td align="center" width="25%" style="padding: 12px">🛡️<br><b>Layered Control</b><br><span style="color:#64748B">bans, quotas, circuit breaking, fallback on one decision chain</span></td><td align="center" width="25%" style="padding: 12px">🔌<br><b>Pluggable Foundation</b><br><span style="color:#64748B">in-memory out of the box; persistence via dbnexus, cache via oxcache</span></td><td align="center" width="25%" style="padding: 12px">📈<br><b>Production Observability</b><br><span style="color:#64748B">Prometheus metrics, OTLP tracing export, HMAC audit chain</span></td></tr>
</table>

</div>

---

## 📋 Table of Contents

<details open>
<summary>📑 目录</summary>

- [✨ Features](#-features)
- [🚀 Quick Start](#-quick-start)
- [🎨 Feature Flags](#-feature-flags)
- [📚 Documentation](#-documentation)
- [💻 Examples](#-examples)
- [🏗️ Architecture](#️-architecture)
- [🧪 Testing](#-testing)
- [📊 Performance](#-performance)
- [🔒 Security](#-security)
- [🗺️ Roadmap](#️-roadmap)
- [🤝 Contributing](#-contributing)
- [📋 Changelog](#-changelog)
- [📄 License](#-license)
- [🙏 Acknowledgments](#-acknowledgments)
- [📞 Contact & Support](#-contact--support)
- [⭐ Star History](#-star-history)

</details>

---

## ✨ Features

<table>
<tr>
<td width="50%" valign="top">

### 🎯 Traffic Governance

- ✅ **Multiple Rate Limiting Algorithms** — Token bucket, sliding window, sharded sliding window, fixed window, concurrency control, GCRA, HTB hierarchical token bucket (`src/limiters/`)
- ✅ **Ban Management** — IP / User / MAC / Geo targets, CIDR range bans, priority system, YAML bulk loading with hot reload, cross-instance sync (`ban-sync`)
- ✅ **Quota Control** — Periodic quota allocation, quota alerts, quota overdraw (`src/quota/`)
- ✅ **Circuit Breaking & Fallback** — Automatic failover, state recovery, fallback strategies (`src/circuit/`, `src/fallback.rs`)

</td>
<td width="50%" valign="top">

### ⚡ Engineering

- 🚀 **High Performance** — Token bucket at 12M+ ops/s, P99 latency < 1µs (see [Performance](#-performance))
- 🧩 **Declarative Integration** — `#[flow_control]` procedural macro, Tower middleware, Admin REST API, `limiteron-cli`
- 🏢 **Multi-Tenancy** — tenant+key compound decision keys; cache, bans, and quotas isolated per tenant
- 📈 **Observability** — Prometheus metrics, OTLP tracing export, HMAC-SHA256 hash-chained audit log, K8s probe endpoints
- 🔐 **Built-in Security** — Identifier key sanitization, log redaction, signed and replay-protected webhooks, Admin RBAC

</td>
</tr>
</table>

<details>
<summary><b>📦 Full Capability List</b></summary>

<br>

- Decision chain (DecisionChain): cascades rules by priority with short-circuit support (`src/decision_chain/`)
- Identifier matching: IP, user ID, device ID, API key, geolocation (MaxMindDB), device info (woothee), custom matchers (`src/matchers/`)
- L1 negative cache: only deny/ban decisions are cached; cache hits never bypass rate-limit or ban semantics (`src/l1_cache.rs`)
- Distributed rate limiting: `DistributedLimiter` trait + in-memory implementation + Redis Lua implementation (`distributed` + `lua-script`)
- Batch APIs: batch decision checks and batch token prefetching (`BatchTokenPrefetcher`)
- Hot reload: atomic config swap via `POST /api/v1/config`, config file watching (`config-watcher`), confers-based hot reload
- Event system: `EventEmitter` / `EventDispatcher`, Transactional Outbox, signed outbound webhooks
- Probe endpoints: `/healthz`, `/readyz`, `/metrics` (authentication bypassed by design)
- Internationalization: ICU4X locale-aware formatting (`i18n`)
- Limit pre-check: non-consuming `Limiter::peek(cost)` / `remaining()` with IETF `RateLimit-*` header data

</details>

---

## 🚀 Quick Start

### 📦 Installation

```bash
cargo add limiteron
```

Requirements: Rust 1.97.1+ (see [rust-toolchain.toml](rust-toolchain.toml)). The default feature set is empty (`default = []`) so core rate limiting has zero external storage dependencies; enable a storage feature when persistence is needed:

```toml
[dependencies]
limiteron = { version = "0.3.0-rc.3", features = ["macros"] }
```

### 💡 Minimal Runnable Example

The following example comes from [`examples/src/bin/simple_rate_limit.rs`](examples/src/bin/simple_rate_limit.rs) and demonstrates the most basic token bucket usage:

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Capacity 10, refill 1 token per second
    let limiter = TokenBucketLimiter::new(10, 1);

    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("Request {} allowed", i),
            Ok(false) => println!("Request {} rate limited", i),
            Err(e) => println!("Request {} error: {:?}", i, e),
        }
    }
    Ok(())
}
```

Run it:

```bash
cargo run -p limiteron-examples --bin simple_rate_limit
```

**Declarative macro** (`macros` feature, in the style of the existing examples):

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::LimiteronError> {
    Ok(format!("Processing request for user {}", user_id))
}
```

### 🧭 Core Concepts

- **Governor**: the main controller (`src/governor.rs`) that takes a `RequestContext` and returns a `Decision`
- **FlowControlConfig**: `version` / `global` / `rules` layered configuration with file loading and environment variable overrides (`ConfigLoader::load_from_file_with_env`)
- **Decision**: three outcomes, `Allowed` / `Rejected` / `Banned` (`src/error/mod.rs`)
- **DecisionChain**: a priority-ordered responsibility chain whose nodes are `Limiters` (`src/decision_chain/`)
- **Storage abstraction**: `Storage` / `BanStorage` / `QuotaStorage` (`src/storage/`) with an in-memory implementation out of the box, plus dbnexus adapters for PostgreSQL / SQLite / MySQL

---

## 🎨 Feature Flags

Limiteron enables no optional functionality by default (`default = []`); compose features as needed. The list below mirrors the `[features]` section of [Cargo.toml](Cargo.toml) item by item:

**Feature Presets**

| Preset | Description | Enabled Features |
|--------|-------------|------------------|
| `minimal` | Core rate limiting, no external storage dependencies | none |
| `standard` | Core features (ban / quota / circuit breaker) | `ban-manager`, `quota-control`, `circuit-breaker` |
| `full` | Everything except storage backends (excludes `cli`) | 24 features, see Cargo.toml |

> Note: presets include no storage backend; add `sqlite` / `postgres` / `mysql` on top for persistence (dbnexus drivers are mutually exclusive — only one per build).

<details>
<summary><b>📋 Complete Feature List (by Category)</b></summary>

<br>

<table>
<tr><th>Category</th><th>Feature</th><th>Description</th><th>Default</th></tr>
<tr><td rowspan="5">Storage Backends</td><td><code>postgres</code></td><td>PostgreSQL storage (dbnexus server-side driver + sea-orm)</td><td>❌</td></tr>
<tr><td><code>sqlite</code></td><td>SQLite storage (dbnexus embedded driver, default local backend)</td><td>❌</td></tr>
<tr><td><code>mysql</code></td><td>MySQL storage (dbnexus server-side driver)</td><td>❌</td></tr>
<tr><td><code>cache-redis</code></td><td>Redis cache backend (via oxcache; formerly <code>cache-storage</code>, kept as a compatibility alias)</td><td>❌</td></tr>
<tr><td><code>lua-script</code></td><td>Redis Lua script execution (via oxcache <code>eval_lua</code>)</td><td>❌</td></tr>
<tr><td rowspan="8">Core</td><td><code>ban-manager</code></td><td>Ban management (target bans, priorities, file loading)</td><td>❌</td></tr>
<tr><td><code>bulkhead</code></td><td>Bulkhead isolation: per-resource-group pools + independent concurrency budgets and isolation metrics</td><td>❌</td></tr>
<tr><td><code>quota-control</code></td><td>Quota control</td><td>❌</td></tr>
<tr><td><code>circuit-breaker</code></td><td>Circuit breaker</td><td>❌</td></tr>
<tr><td><code>fallback</code></td><td>Fallback strategies (FallbackManager)</td><td>❌</td></tr>
<tr><td><code>custom-limiter</code></td><td>Custom rate limiter support</td><td>❌</td></tr>
<tr><td><code>cache-service</code></td><td>Unified cache service (DI support)</td><td>❌</td></tr>
<tr><td><code>gcra</code></td><td>GCRA rate limiting algorithm</td><td>❌</td></tr>
<tr><td rowspan="2">Security</td><td><code>log-redaction</code></td><td>Log redaction</td><td>❌</td></tr>
<tr><td><code>validation</code></td><td>Identifier input validation (IP / User ID / MAC)</td><td>❌</td></tr>
<tr><td>Performance</td><td><code>parallel-checker</code></td><td>Parallel ban checking</td><td>❌</td></tr>
<tr><td rowspan="2">Advanced Matching</td><td><code>geo-matching</code></td><td>Geographic matching (MaxMindDB)</td><td>❌</td></tr>
<tr><td><code>device-matching</code></td><td>Device matching (woothee User-Agent parsing)</td><td>❌</td></tr>
<tr><td rowspan="2">Control Plane</td><td><code>admin-api</code></td><td>Admin REST API (axum, with RBAC and self rate-limit protection)</td><td>❌</td></tr>
<tr><td><code>cli</code></td><td><code>limiteron-cli</code> binary: rule file validation / export / apply dry-run</td><td>❌</td></tr>
<tr><td rowspan="5">Observability</td><td><code>telemetry</code></td><td>Tracing initialization (tracing-subscriber)</td><td>❌</td></tr>
<tr><td><code>monitoring</code></td><td>Prometheus metrics</td><td>❌</td></tr>
<tr><td><code>metrics</code></td><td>Governor allow / reject / ban three-point metrics (implies <code>monitoring</code>)</td><td>❌</td></tr>
<tr><td><code>audit-log</code></td><td>Audit logging (HMAC-SHA256 hash chain with tamper detection)</td><td>❌</td></tr>
<tr><td><code>otlp</code></td><td>OTLP/HTTP tracing export</td><td>❌</td></tr>
<tr><td rowspan="3">Tooling</td><td><code>macros</code></td><td><code>#[flow_control]</code> declarative macro (limiteron-macros)</td><td>❌</td></tr>
<tr><td><code>config-watcher</code></td><td>Config file watching and hot reload</td><td>❌</td></tr>
<tr><td><code>webhook</code></td><td>Outbound webhooks (HMAC-SHA256 signature header + timestamp replay protection)</td><td>❌</td></tr>
<tr><td rowspan="2">Events</td><td><code>event-system</code></td><td>Event system (EventEmitter / Dispatcher / Outbox)</td><td>❌</td></tr>
<tr><td><code>ban-sync</code></td><td>Cross-instance ban sync (oxcache Pub/Sub broadcast)</td><td>❌</td></tr>
<tr><td>Multi-Tenancy</td><td><code>multi-tenant</code></td><td>tenant+key compound decision keys and per-tenant isolation</td><td>❌</td></tr>
<tr><td>Middleware</td><td><code>tower-middleware</code></td><td>Tower Layer / Service integration</td><td>❌</td></tr>
<tr><td>Distributed</td><td><code>distributed</code></td><td><code>DistributedLimiter</code> trait + in-memory implementation (Redis implementation additionally requires <code>lua-script</code>)</td><td>❌</td></tr>
<tr><td rowspan="3">Algorithms</td><td><code>adaptive-limiting</code></td><td>AIMD adaptive concurrency limiter (latency/error-rate feedback window tuning)</td><td>✅</td></tr>
<tr><td><code>priority-queue</code></td><td>Compatibility declaration, no effect when enabled</td><td>❌</td></tr>
<tr><td><code>admission-control</code></td><td>Compatibility declaration, no effect when enabled</td><td>❌</td></tr>
<tr><td rowspan="5">Ecosystem</td><td><code>kit</code></td><td>trait-kit <code>LimiteronModule</code> integration (health/lifecycle ports)</td><td>❌</td></tr>
<tr><td><code>i18n</code></td><td>ICU4X locale-aware formatting</td><td>❌</td></tr>
<tr><td><code>inklog</code></td><td>inklog structured logging integration</td><td>❌</td></tr>
<tr><td><code>config-confers</code></td><td>Load configuration from confers sources</td><td>❌</td></tr>
<tr><td><code>config-confers-reload</code></td><td>confers hot reload (implies <code>config-confers</code>)</td><td>❌</td></tr>
<tr><td>Development/Testing</td><td><code>test-clock</code></td><td><code>MockClock</code> test clock (external test consumers only)</td><td>❌</td></tr>
</table>

</details>

> ⚠️ **Storage driver exclusivity**: `postgres` / `sqlite` / `mysql` all go through dbnexus; embedded and server-side drivers cannot coexist in one build, so avoid `--all-features` and use explicit feature combinations.
>
> ⚠️ **No-op features**: `priority-queue` and `admission-control` are declared only for downstream compatibility and have no effect when enabled. Do not rely on them for capability detection.

---

## 📚 Documentation

| Documentation | Description |
|---------------|-------------|
| [📖 User Guide](docs/USER_GUIDE.md) | Complete tutorial from installation and core concepts to advanced usage and troubleshooting |
| [📘 API Reference](docs/API_REFERENCE.md) | Detailed description of all public APIs |
| [🏗️ Architecture](docs/ARCHITECTURE.md) | Design philosophy, module breakdown, and extension mechanisms |
| [❓ FAQ](docs/FAQ.md) | Frequently asked questions and troubleshooting |
| [🧪 Testing Guide](docs/TESTING.md) | Test categories, commands, and coverage notes |
| [🧬 Test Scenarios](docs/TEST_SCENARIOS.md) | Test pyramid baseline and E2E scenario definitions |
| [📈 Coverage Report](docs/COVERAGE_REPORT.md) | Historical baseline data (generated in the v0.1.0 era, pending CI coverage refresh) |
| [🔒 Security](docs/SECURITY.md) | Security design, version support policy, and vulnerability reporting process |
| [📋 Changelog](docs/CHANGELOG.md) | Changes in every release |
| [🤝 Contributing](docs/CONTRIBUTING.md) | How to participate in development |
| [📦 docs.rs](https://docs.rs/limiteron) | Latest API documentation generated by docs.rs |
| [📦 crates.io](https://crates.io/crates/limiteron) | Release page |

---

## 💻 Examples

[`examples/`](examples/) is a standalone sub-crate in the workspace (`limiteron-examples`) containing 21 runnable examples. Enable the required features when running feature-gated examples:

```bash
cargo run -p limiteron-examples --bin simple_rate_limit
cargo run -p limiteron-examples --features "ban-manager,admin-api" --bin ban_http_api
```

| Example | Description |
|---------|-------------|
| `simple_rate_limit` | The most basic token bucket usage |
| `rate_limiters` | Five algorithms: token bucket, sliding window, fixed window, concurrency limiter, GCRA |
| `macro_usage` | Using the `flow_control` macro and its current-version constraints |
| `governor_demo` | Three Governor construction modes, request checks, decision parsing, and statistics |
| `matchers_demo` | Complete flow of identifier extractors, request contexts, and rule matchers |
| `decision_chain` | Responsibility-chain decisions: composing limiters, priority execution, short-circuiting |
| `custom_matchers` | Custom matcher trait implementation, registry, and built-in Header / TimeWindow matchers |
| `authorization_demo` | Implementing the authorization provider trait and the built-in `SimpleAuthorizationProvider` |
| `graceful_shutdown` | Listening for Ctrl+C and invoking the idempotent `Governor::shutdown()` |
| `circuit_breaker` | Circuit breaker: failure detection, opening, half-open probing, and timeout recovery |
| `quota_control` | Quota consumption tracking, enforcement, and usage percentage calculation |
| `ban_manager` | Creating, querying, updating, and removing bans for IP / User ID / MAC targets |
| `ban_file_loader` | Loading ban rules from YAML with file-change hot reload |
| `ban_http_api` | Starting an AdminServer and calling ban management endpoints over HTTP |
| `validation_demo` | Unified validation: IP, user ID, MAC, API key, ban target validation |
| `storage_factory` | Creating Postgres / MySQL / SQLite backends from a DSN via `StorageFactory` |
| `fallback_demo` | Fallback manager: strategy configuration, fault injection, fallback execution, island mode |
| `audit_log_demo` | Audit logging: event recording, configuration, statistics, and signature verification |
| `tower_middleware` | Integrating Governor flow control into a Tower Service pipeline |
| `telemetry_demo` | Prometheus metrics collection and OpenTelemetry distributed tracing |
| `device_geo_matching` | User-Agent parsing, device detection, IP geolocation, and geo condition matching |

---

## 🏗️ Architecture

Limiteron uses a layered architecture: the access layer (Tower middleware, Admin API) hands traffic to the **Governor** main controller, which extracts identifiers and matches rules through **matchers**, then cascades along each rule's **decision_chain** of limiter algorithm instances; bans, quotas, circuit breaking, and fallback participate in decisions as domain components; state lands through the **storage** abstraction (in-memory by default, switchable in production to dbnexus persistence for PostgreSQL / SQLite / MySQL).

The full architecture diagram, core decision sequence, module responsibility table, storage backend list, and extension mechanisms live in the [Architecture document](docs/ARCHITECTURE.md).

---

### 🎯 Core Decision Flow

The complete decision path of a single `Governor::check(context)` call (distilled from `src/governor.rs`): identifier extraction and rule matching (computed once) → L1 negative-cache lookup (fail-closed, deny/ban decisions only) → priority-ordered cascade across each rule's decision chain (any rule rejecting rejects the request; only unanimous approval passes) → rate-limit event emission and an `Allowed` / `Rejected` / `Banned` outcome.

The full decision sequence diagram and the itemized key semantics (including the `parallel-checker` ordering where the ban check runs before the cache read) live in the [Architecture document](docs/ARCHITECTURE.md#-核心决策流程).

---

### 🔗 Ecosystem & Integrations

Limiteron collaborates closely with its sibling crates in the workspace; each integration is feature-gated and excluded by default:

| Integration | Features | Description |
|-------------|----------|-------------|
| [dbnexus](https://github.com/Kirky-X/dbnexus) | `postgres` / `sqlite` / `mysql` | Database abstraction layer providing persistent storage adapters and metrics propagation |
| [oxcache](https://github.com/Kirky-X/oxcache) | `cache-service` / `cache-redis` / `lua-script` / `ban-sync` | Unified cache service, Redis cache backend, Lua execution, Pub/Sub ban broadcast |
| [trait-kit](https://github.com/Kirky-X/trait-kit) | `kit` | `LimiteronModule` modular integration + health/lifecycle ports |
| [inklog](https://github.com/Kirky-X/inklog) | `inklog` | Structured logging (console/file/database sinks) with the `SinkRateLimit` port |
| [confers](https://github.com/Kirky-X/confers) | `config-confers` / `config-confers-reload` | Configuration source loading and hot reload (automatic rollback on validation failure) |

Additionally, the `i18n` feature integrates [ICU4X](https://github.com/unicode-org/icu4x) for locale-aware formatting.

---

## 🧪 Testing

The testing strategy matrix (unit / integration & E2E / property / doc / benchmark layers) and run commands (identical to [CI](.github/workflows/ci.yml), including the coverage gate) live in the [Testing Guide](docs/TESTING.md); the layer baselines and E2E scenario definitions live in [Test Scenarios](docs/TEST_SCENARIOS.md).

**Test scale** (grep count of `#[test]` / `#[tokio::test]` attributes, as of v0.3.0-rc.3):

| Metric | Count |
|--------|-------|
| In-library test functions (`src/`) | 2,160 (#[test] 1,410 + #[tokio::test] 750) |
| External test functions (`tests/`) | 599 (#[test] 157 + #[tokio::test] 442) |
| Property test groups (proptest) | 4 |

---

## 📊 Performance

> **Note:** The following data represents actual results from comprehensive testing on 2026-01-19.

<table>
<tr>
<td width="50%" valign="top">

**Throughput**

| Limiter Type | Actual | Target | Achievement |
|-------------|--------|--------|-------------|
| TokenBucket | **12M+ ops/s** | 500K ops/s | ✅ 24x |
| FixedWindow | **20M+ ops/s** | 300K ops/s | ✅ 66x |
| ConcurrencyLimiter | **12M+ ops/s** | 200K ops/s | ✅ 60x |

</td>
<td width="50%" valign="top">

**Latency**

| Percentile | TokenBucket | FixedWindow |
|-----------|-------------|-------------|
| P50 | < 100ns | < 100ns |
| P95 | < 200ns | < 150ns |
| P99 | < 1µs | < 500ns |

</td>
</tr>
</table>

<details>
<summary><b>📈 Detailed Benchmark Data</b></summary>

<br>

```text
TokenBucket: 12,088,759 ops/s
FixedWindow: 19,920,188 ops/s
ConcurrencyLimiter: 11,891,237 ops/s
Concurrency tests: 100% data consistency, rate limit correctness 1000/1000
```

</details>

**Benchmark facility**: the repository ships four criterion benchmark suites (all require the `full` feature) for reproduction and regression detection:

| Benchmark | File | Contents |
|-----------|------|----------|
| Throughput | `benches/throughput.rs` | Single-thread/concurrent throughput and scaling curves |
| Latency | `benches/latency.rs` | P50/P90/P99/P99.9 latency measurement and operation comparison |
| Memory | `benches/memory.rs` | Memory footprint by key count, data structure comparison, leak detection |
| Regression | `benches/regression.rs` | Historical baseline storage and automatic comparison alerts |

```bash
cargo bench --features full
```

---

## 🔒 Security

**Reporting a vulnerability**: please do not report security vulnerabilities through public issues. Use the GitHub [Security Advisories](https://github.com/Kirky-X/limiteron/security/advisories/new) private disclosure channel ("Report a vulnerability"). The maintainer commits to acknowledging reports within 48 hours and providing an initial assessment within 7 days, following coordinated disclosure. See [SECURITY.md](docs/SECURITY.md) for the full process.

**Security design highlights** (each traceable in the source and the [Security document](docs/SECURITY.md)):

- **Input defenses** — identifier key sanitization (ASCII allowlist + 128-char truncation, defending against key injection and homoglyph attacks); IP / user ID / MAC format validation (`src/validation.rs`)
- **Algorithm boundaries** — saturating arithmetic and capacity capping in the token bucket; clock-fallback protection in quota windows
- **Admin self-protection** — per-path/per-client rate limiting on admin endpoints + bucket memory caps + multi-key token authentication with an admin/viewer role matrix (RBAC)
- **Data protection** — secrecy for sensitive data, log redaction (`log-redaction`), HMAC-SHA256 hash-chained audit events with tamper detection
- **Transport defenses** — trusted-proxy X-Forwarded-For extraction; outbound webhook signatures + timestamp replay protection + URL validation (SSRF)
- **Supply chain** — rustls-webpki minimum version pin (CVE-2025-48369); [cargo-deny](deny.toml) checks vulnerabilities/licenses/duplicate dependencies; the CI Security job and the pre-push hook run `cargo deny check` and `cargo audit`

---

## 🗺️ Roadmap

<table>
<tr>
<td width="12%" align="center"><b>✅ Completed</b></td>
<td>Core rate limiting algorithms, ban management, quota control, circuit breaker, the <code>#[flow_control]</code> macro, unit and integration test suites, PostgreSQL / SQLite storage via dbnexus, Governor graceful shutdown and health check, ConfigLoader environment variable overrides (v0.2.0); Tower middleware refinement, event system enhancements, RedisStorage removal with caching unified through oxcache (v0.2.1)</td>
</tr>
<tr>
<td width="12%" align="center"><b>✅ Shipped in v0.3.0-rc.3</b></td>
<td>Multi-tenancy through Governor, CIDR range bans, Admin RBAC, OTLP tracing export, MySQL storage, distributed limiting, <code>limiteron-cli</code>, and more (see the <a href="docs/CHANGELOG.md">changelog</a>)</td>
</tr>
<tr>
<td width="12%" align="center"><b>🚧 In Progress</b></td>
<td>Performance optimization, monitoring and tracing improvements</td>
</tr>
<tr>
<td width="12%" align="center"><b>📋 Planned</b></td>
<td>Governor shutdown full implementation (background task awaiting/state flush/connection release/Drop trait), Lua script enhancements, custom matcher extensions, additional storage backends, Web UI management interface</td>
</tr>
<tr>
<td width="12%" align="center"><b>💡 Future Ideas</b></td>
<td>Machine learning-driven rate limiting, additional rate limiting algorithms, community plugin system</td>
</tr>
</table>

---

## 🤝 Contributing

Contributions of any kind are welcome! See [CONTRIBUTING.md](docs/CONTRIBUTING.md) for the development environment, TDD workflow, coding conventions, and the PR process.

<table>
<tr>
<td width="33%" align="center">

### 🐛 Report Issues

Found a bug?<br>
[Create Issue](../../issues)

</td>
<td width="33%" align="center">

### 💡 Feature Requests

Have a suggestion?<br>
[Start Discussion](../../discussions)

</td>
<td width="33%" align="center">

### 🔧 Submit Code

Want to contribute?<br>
[Fork & PR](../../pulls)

</td>
</tr>
</table>

<details>
<summary><b>🔧 Development Environment Baseline</b></summary>

<br>

Rust 1.97.1 (pinned in [rust-toolchain.toml](rust-toolchain.toml)); lefthook / pre-commit hooks cover fmt, clippy, supply-chain checks, secret scanning, and the coverage gate. See [CONTRIBUTING.md](docs/CONTRIBUTING.md) for environment setup, hook details, and commit conventions.

</details>

---

## 📋 Changelog

See [CHANGELOG.md](docs/CHANGELOG.md) for the full history. Recent releases:

- **0.3.0-rc.3** (2026-09-10) — Multi-tenancy through Governor, CIDR range bans, Admin RBAC, OTLP tracing export, MySQL storage, HTB hierarchical token bucket, bulkhead isolation, AIMD adaptive limiting, `limiteron-cli`, webhook signatures, event Outbox, cross-instance ban sync
- **0.3.0-rc.2** (2026-09-03) — Documentation synced to the 0.3 line, workspace dependency path localization, `Cargo.lock` committed to version control
- **0.2.10** (2026-07-22) — Added 76 boundary and edge-case tests, sea-orm upgraded to 2.0 stable, unused dependencies removed

---

## 📄 License

This project is licensed under the MIT + Commons Clause License; commercial use requires separate authorization. See [LICENSE](LICENSE). Copyright (c) 2026 Kirky.X🌠.

---

## 🙏 Acknowledgments

- 🌟 **Dependencies** — Built on these excellent projects:
  - [tokio](https://tokio.rs/) — Async runtime
  - [dbnexus](https://github.com/Kirky-X/dbnexus) — Database abstraction layer
  - [oxcache](https://github.com/Kirky-X/oxcache) — Unified cache service
  - [trait-kit](https://github.com/Kirky-X/trait-kit) — Trait integration modules
  - [inklog](https://github.com/Kirky-X/inklog) — Structured logging
  - [dashmap](https://github.com/xacrimon/dashmap) — Concurrent HashMap
- 👥 **Contributors** — Thanks to all contributors!
- 💬 **Community** — Special thanks to community members

---

## 📞 Contact & Support

- 🐛 [Issues](https://github.com/Kirky-X/limiteron/issues) — Report bugs and errors
- 💬 [Discussions](https://github.com/Kirky-X/limiteron/discussions) — Ask questions and share ideas
- 📦 [GitHub Repository](https://github.com/Kirky-X/limiteron) — View the source code
- 🤝 Maintainer: Kirky.X

---

## ⭐ Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Kirky-X/limiteron&type=Date)](https://star-history.com/#Kirky-X/limiteron&Date)

### 💝 Support This Project

If you find this project useful, please consider giving it a ⭐️!

**Built with ❤️ by Kirky.X**
