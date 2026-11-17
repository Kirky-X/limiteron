English | [中文](README_CN.md)

<div align="center">

<img src="docs/assets/limiteron.png" alt="Limiteron Logo" width="240">

[![Rust 2024](https://img.shields.io/badge/Rust-2024-edded?logo=rust&style=for-the-badge)](https://www.rust-lang.org/) [![Rustc 1.97.1+](https://img.shields.io/badge/Rustc-1.97.1+-orange.svg?style=for-the-badge)](https://www.rust-lang.org/) [![crates.io](https://img.shields.io/crates/v/limiteron?style=for-the-badge)](https://crates.io/crates/limiteron) [![docs.rs](https://img.shields.io/docsrs/limiteron?style=for-the-badge)](https://docs.rs/limiteron) [![downloads](https://img.shields.io/crates/d/limiteron?style=for-the-badge)](https://crates.io/crates/limiteron) [![CI](https://img.shields.io/github/actions/workflow/status/Kirky-X/limiteron/ci.yml?branch=main&label=CI&style=for-the-badge)](https://github.com/Kirky-X/limiteron/actions/workflows/ci.yml) [![coverage](https://img.shields.io/codecov/c/github/Kirky-X/limiteron?style=for-the-badge)](https://codecov.io/gh/Kirky-X/limiteron) [![license](https://img.shields.io/crates/l/limiteron?style=for-the-badge)](LICENSE)

[中文](README_CN.md) | **English**

**Unified Flow Control Framework for Rust**

[✨ Features](#-features) • [🚀 Quick Start](#-quick-start) • [📚 Documentation](#-documentation) • [💻 Examples](#-examples) • [🤝 Contributing](#-contributing)

</div>

---

<div align="center" style="padding: 32px; margin: 24px 0">

### 🚦 Unified Traffic Control

One Governor entrypoint where multi-algorithm limiting and layered control decide together:

<table style="width:100%; border-collapse: collapse">
<tr><td align="center" width="25%" style="padding: 12px">🚦<br><b>Multi-Algorithm Limiting</b><br><span style="color:#64748B">token bucket, sliding/fixed window, leaky bucket, sliding window log, concurrency, GCRA, HTB</span></td><td align="center" width="25%" style="padding: 12px">🛡️<br><b>Layered Control</b><br><span style="color:#64748B">bans, quotas, circuit breaking, fallback on one decision chain</span></td><td align="center" width="25%" style="padding: 12px">🔌<br><b>Pluggable Foundation</b><br><span style="color:#64748B">in-memory out of the box; persistence via dbnexus, cache via oxcache</span></td><td align="center" width="25%" style="padding: 12px">📈<br><b>Production Observability</b><br><span style="color:#64748B">Prometheus metrics, OTLP tracing export, HMAC audit chain</span></td></tr>
</table>

</div>

---

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

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

<table style="width:100%; border-collapse: collapse">
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🎯 <b>Rate-limiting algorithm matrix</b><br><span style="color:#64748B">Token bucket, sliding window, sharded sliding window, sliding window log (exact sliding window), fixed window, leaky bucket (water-level meter, dual of token bucket), concurrency control, GCRA, HTB hierarchical token bucket (<code>src/limiters/</code>)</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🚫 <b>Ban management</b><br><span style="color:#64748B">IP / User / MAC / Geo target bans, CIDR range bans, priority system, YAML bulk loading with hot reload, cross-instance sync (<code>ban-sync</code>)</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">⚖️ <b>Quota control</b><br><span style="color:#64748B">Periodic quota allocation, quota alerts, quota overdraw (<code>src/quota/</code>)</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🧯 <b>Circuit breaking &amp; fallback</b><br><span style="color:#64748B">Automatic failover, state recovery, fallback strategies (<code>src/circuit/</code>, <code>src/fallback.rs</code>)</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🚀 <b>High performance</b><br><span style="color:#64748B">Token bucket at 12M+ ops/s, P99 latency &lt; 1µs (see [Performance](#-performance))</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🧩 <b>Declarative integration</b><br><span style="color:#64748B"><code>#[flow_control]</code> procedural macro, Tower middleware, Admin REST API, <code>limiteron-cli</code></span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🏢 <b>Multi-tenancy</b><br><span style="color:#64748B">tenant+key compound decision keys; cache, bans, and quotas isolated per tenant</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">📈 <b>Observability</b><br><span style="color:#64748B">Prometheus metrics, OTLP tracing export, HMAC-SHA256 hash-chained audit log, K8s probe endpoints</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🔐 <b>Built-in security</b><br><span style="color:#64748B">Identifier key sanitization, log redaction, signed and replay-protected webhooks, Admin RBAC</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🧠 <b>Decision-chain cascade</b><br><span style="color:#64748B">DecisionChain cascades rules by priority with short-circuit support (<code>src/decision_chain/</code>)</span></td>
</tr>
</table>

For the full capability list and API details see the [API Reference](docs/en/API_REFERENCE.md).

---

## 🚀 Quick Start

### 📦 Installation

<details>
<summary>📄 Code · bash</summary>

```bash
cargo add limiteron
```

</details>

Requirements: Rust 1.97.1+ (see [rust-toolchain.toml](rust-toolchain.toml)). The default feature set is empty (`default = []`) so core rate limiting has zero external storage dependencies; enable a storage feature when persistence is needed:

<details>
<summary>📄 Code · toml</summary>

```toml
[dependencies]
limiteron = { version = "0.3", features = ["macros"] }
```

</details>

### 💡 Minimal Runnable Example

The following example comes from [`examples/src/bin/simple_rate_limit.rs`](examples/src/bin/simple_rate_limit.rs) and demonstrates the most basic token bucket usage:

<details>
<summary>📄 Code · rust</summary>

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

</details>

Run it:

<details>
<summary>📄 Code · bash</summary>

```bash
cargo run -p limiteron-examples --bin simple_rate_limit
```

</details>

For declarative-macro integration see [User Guide · Common Patterns](docs/en/USER_GUIDE.md#-common-patterns); for core concepts and the full API see the [API Reference](docs/en/API_REFERENCE.md).

---

## 🎨 Feature Flags

Limiteron enables no optional functionality by default (`default = []`); compose features as needed. For feature presets (minimal / standard / full), the complete feature list, and the file-backed SQLite deployment note see [User Guide · Feature Flags](docs/en/USER_GUIDE.md#-feature-flags); for feature dependencies see the [Architecture document](docs/en/ARCHITECTURE.md#-feature-dependency-overview).

---

## 📚 Documentation

<details>
<summary>📋 Table</summary>

| Documentation | Description |
|---------------|-------------|
| [📖 User Guide](docs/en/USER_GUIDE.md) | Complete tutorial from installation and core concepts to advanced usage and troubleshooting |
| [📘 API Reference](docs/en/API_REFERENCE.md) | Detailed description of all public APIs |
| [🏗️ Architecture](docs/en/ARCHITECTURE.md) | Design philosophy, module breakdown, and extension mechanisms |
| [❓ FAQ](docs/en/FAQ.md) | Frequently asked questions and troubleshooting |
| [🧪 Testing Guide](docs/en/TESTING.md) | Test categories, commands, and coverage notes |
| [📈 Benchmark Procedure](docs/en/BENCHMARKS.md) | `cargo bench` methodology, baseline refresh timing, and criterion comparison usage |
| [🧬 Test Scenarios](docs/en/TEST_SCENARIOS.md) | Test pyramid baseline and E2E scenario definitions |
| [🔒 Security](docs/en/SECURITY.md) | Security design, version support policy, and vulnerability reporting process |
| [📋 Changelog](docs/CHANGELOG.md) | Changes in every release |
| [🤝 Contributing](docs/en/CONTRIBUTING.md) | How to participate in development |
| [📦 docs.rs](https://docs.rs/limiteron) | Latest API documentation generated by docs.rs |
| [📦 crates.io](https://crates.io/crates/limiteron) | Release page |

</details>

---

## 💻 Examples

[`examples/`](examples/) is a standalone sub-crate in the workspace (`limiteron-examples`) containing 21 runnable examples; for the complete example list and run commands see [examples/README.md](examples/README.md).

---

## 🏗️ Architecture

Limiteron uses a layered architecture: the access layer (Tower middleware, Admin API) hands traffic to the **Governor** main controller, which extracts identifiers and matches rules through **matchers**, then cascades along each rule's **decision_chain** of limiter algorithm instances; bans, quotas, circuit breaking, and fallback participate in decisions as domain components; state lands through the **storage** abstraction (in-memory by default, switchable in production to dbnexus persistence for PostgreSQL / SQLite / MySQL).

The full architecture diagram, core decision sequence, module responsibility table, storage backend list, and extension mechanisms live in the [Architecture document](docs/en/ARCHITECTURE.md).

---

### 🎯 Core Decision Flow

The complete decision path of a single `Governor::check(context)` call (distilled from `src/governor.rs`): identifier extraction and rule matching (computed once) → L1 negative-cache lookup (fail-closed, deny/ban decisions only) → priority-ordered cascade across each rule's decision chain (any rule rejecting rejects the request; only unanimous approval passes) → rate-limit event emission and an `Allowed` / `Rejected` / `Banned` outcome.

The full decision sequence diagram and the itemized key semantics (including the `parallel-checker` ordering where the ban check runs before the cache read) live in the [Architecture document](docs/en/ARCHITECTURE.md#-core-decision-flow).

---

### 🔗 Ecosystem & Integrations

Limiteron collaborates closely with its sibling crates in the workspace; each integration is feature-gated and excluded by default:

| Integration | Features | Description |
|-------------|----------|-------------|
| [dbnexus](https://github.com/Kirky-X/dbnexus) | `postgres` / `sqlite` / `mysql` | Database abstraction layer providing persistent storage adapters and metrics propagation |
| [oxcache](https://github.com/Kirky-X/oxcache) | `cache-service` / `cache-redis` / `lua-script` / `ban-sync` | Unified cache service, Redis cache backend, Lua execution, Pub/Sub ban broadcast |
| [trait-kit](https://github.com/Kirky-X/trait-kit) | `kit` | `LimiteronModule` modular integration + health/lifecycle ports |
| [confers](https://github.com/Kirky-X/confers) | `config-confers` / `config-confers-reload` | Configuration source loading and hot reload (automatic rollback on validation failure) |

For integration design notes on inklog, ICU4X, the sdforge guard layer, and the dbnexus rate-limit port adapter, see the [Architecture document · Ecosystem and Integrations](docs/en/ARCHITECTURE.md#-ecosystem-and-integrations).

---

## 🧪 Testing

The testing strategy matrix (unit / integration & E2E / property / doc / benchmark layers) and run commands (identical to [CI](.github/workflows/ci.yml), including the coverage gate) live in the [Testing Guide](docs/en/TESTING.md); the layer baselines and E2E scenario definitions live in [Test Scenarios](docs/en/TEST_SCENARIOS.md).

For test-scale statistics (grep count via the `#[test]` / `#[tokio::test]` attributes) see [Testing Guide · Test Statistics](docs/en/TESTING.md#-test-statistics).

---

## 📊 Performance

Historical measurements (v0.1.0-era methodology, order-of-magnitude reference only) and the benchmark-infrastructure details are archived in the [Benchmark Procedure · Historical Comparison](docs/en/BENCHMARKS.md#historical-comparison); for the current reproducible baseline, follow the [Running](docs/en/BENCHMARKS.md#running) commands in that file together with same-machine re-measurements.

---

## 🔒 Security

**Reporting a vulnerability**: please do not report security vulnerabilities through public issues. Use the GitHub [Security Advisories](https://github.com/Kirky-X/limiteron/security/policy) private disclosure channel ("Report a vulnerability"). Kirky.X commits to acknowledging reports within 48 hours and providing an initial assessment within 7 days, following coordinated disclosure. See [SECURITY.md](docs/en/SECURITY.md) for the full process.

For the security design highlights (input defenses, algorithm boundaries, Admin self-protection, read-only Web UI warnings, data protection, transport defenses, and supply chain), see the [Security document](docs/en/SECURITY.md).

---

## 🗺️ Roadmap

<table>
<tr>
<td width="12%" align="center"><b>✅ Completed</b></td>
<td>Core rate limiting algorithms, ban management, quota control, circuit breaker, the <code>#[flow_control]</code> macro, unit and integration test suites, PostgreSQL / SQLite storage via dbnexus, Governor graceful shutdown and health check, ConfigLoader environment variable overrides (v0.2.0); Tower middleware refinement, event system enhancements, RedisStorage removal with caching unified through oxcache (v0.2.1)</td>
</tr>
<tr>
<td width="12%" align="center"><b>✅ Shipped (released with 0.3.0-rc.4)</b></td>
<td>Multi-tenancy through Governor, CIDR range bans, Admin RBAC, OTLP tracing export, MySQL storage, distributed limiting, <code>limiteron-cli</code>, and more (see the <a href="docs/CHANGELOG.md">changelog</a>)</td>
</tr>
<tr>
<td width="12%" align="center"><b>✅ Delivered</b></td>
<td>Performance optimization and monitoring/tracing improvements — four criterion benchmark suites repaired with unified fresh-instance pass-path methodology (<code>90fb0cb</code>), HeaderMatcher zero-allocation ASCII fast-path hotspot reduction (-10.0%~-33.4% across 12 benchmarks, <code>4cc7c6f</code>), per-rule/per-limiter dimensional metrics and <code>governor_check</code> span (<code>5422c6d</code>); OTLP trace export shipped with 0.3.0-rc.4</td>
</tr>
<tr>
<td width="12%" align="center"><b>📋 Planned</b></td>
<td>Additional storage backends</td>
</tr>
<tr>
<td width="12%" align="center"><b>💡 Future Ideas</b></td>
<td>"ML-driven rate limiting" was adjudicated and materialized as statistical heuristics (`adaptive-limiting` AIMD concurrency window and `adaptive-threshold` sliding-window dynamic quota, both deterministic threshold logic, not neural networks — see `src/limiters/adaptive_threshold.rs`); the "community plugin system" has landed as compile-time registration (`plugins`, dynamic .so explicitly not supported, see `src/plugins/`)</td>
</tr>
</table>

---

## 🤝 Contributing

Contributions of any kind are welcome! See [CONTRIBUTING.md](docs/en/CONTRIBUTING.md) for the development environment, TDD workflow, coding conventions, and the PR process.

<table>
<tr>
<td width="33%" align="center">

### 🐛 Report Issues

Found a bug?<br>
[Create Issue](https://github.com/Kirky-X/limiteron/issues)

</td>
<td width="33%" align="center">

### 💡 Feature Requests

Have a suggestion?<br>
[Start Discussion](https://github.com/Kirky-X/limiteron/discussions)

</td>
<td width="33%" align="center">

### 🔧 Submit Code

Want to contribute?<br>
[Fork & PR](https://github.com/Kirky-X/limiteron/pulls)

</td>
</tr>
</table>

<details>
<summary><b>🔧 Development Environment Baseline</b></summary>

<br>

Rust 1.97.1 (pinned in [rust-toolchain.toml](rust-toolchain.toml)); lefthook / pre-commit hooks cover fmt, clippy, supply-chain checks, secret scanning, and the coverage gate. See [CONTRIBUTING.md](docs/en/CONTRIBUTING.md) for environment setup, hook details, and commit conventions.

</details>

---

## 📋 Changelog

See [CHANGELOG.md](docs/CHANGELOG.md) for the full history.

---

## 📄 License

This project is licensed under the [MIT](LICENSE) License.

Copyright (c) 2026 Kirky.X🌠

---

## 🙏 Acknowledgments

- 🌟 **Dependencies** — Built on these excellent projects:
  - [tokio](https://tokio.rs/) — Async runtime
  - [dbnexus](https://github.com/Kirky-X/dbnexus) — Database abstraction layer
  - [oxcache](https://github.com/Kirky-X/oxcache) — Unified cache service
  - [trait-kit](https://github.com/Kirky-X/trait-kit) — Module standard interfaces and the masking engine (the masking feature of the trait-kit repo, consumed here via log-redaction)
  - [inklog](https://github.com/Kirky-X/inklog) — Structured logging (per-target bucketed sink rate limiting via the `integrations/limiteron-inklog` adapter)
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
