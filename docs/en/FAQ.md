# ❓ Limiteron FAQ

This document collects Limiteron's frequently asked questions by topic, covering general questions, installation and setup, usage and features, performance, security, and troubleshooting. For a systematic tutorial see the [User Guide](USER_GUIDE.md); for API details see the [API Reference](API_REFERENCE.md).

[🏠 Home](../../README.md) • [📖 User Guide](USER_GUIDE.md) • [📘 API Reference](API_REFERENCE.md)

---

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [🤔 General](#-general)
- [📦 Installation and Setup](#-installation-and-setup)
- [💡 Usage and Features](#-usage-and-features)
- [⚡ Performance](#-performance)
- [🔒 Security](#-security)
- [🔧 Troubleshooting](#-troubleshooting)
- [🤝 Contributing](#-contributing)
- [📄 License](#-license)

</details>

---

## 🤔 General

<details>
<summary><b>❓ What is Limiteron?</b></summary>

<br>

**Limiteron** is a unified flow-control framework for Rust that provides:

- Multiple rate-limiting algorithms (token bucket, leaky bucket, sliding/sharded-sliding/sliding-window-log/fixed window, concurrency control, GCRA, HTB hierarchical token bucket, AIMD adaptive)
- Ban management (IP / user / MAC / Geo / CIDR bans, automatic bans, ban priorities, bulk YAML loading)
- Quota management (periodic quotas, quota alerts, quota overdraft)
- Circuit breaking and fallback (automatic circuit breaking, state recovery, fallback strategies)

It is designed for developers who need to protect API services against abuse and traffic spikes.

**Learn more:** [User Guide](USER_GUIDE.md)

</details>

<details>
<summary><b>❓ What are Limiteron's core capabilities?</b></summary>

<br>

| Capability | Description |
|------|------|
| Multi-dimensional rate limiting | Token bucket, leaky bucket, sliding/sharded-sliding/sliding-window-log/fixed window, concurrency control, GCRA, HTB, AIMD adaptive |
| Defense-in-depth control | Bans, quotas, circuit breaking, and fallback execute cooperatively along one decision chain |
| Pluggable foundation | In-memory storage out of the box; persistence and distributed cache via dbnexus and oxcache |
| Production observability | Prometheus metrics, OTLP trace export, HMAC chained audit logs, K8s probe endpoints |
| Declarative integration | `#[flow_control]` proc macro, Tower middleware, Admin REST API, `limiteron-cli` |

**Key advantages:**

- 🚀 High performance (token-bucket throughput 12M+ ops/s, P99 latency < 1µs, see [Performance](#-performance))
- 💡 Clean API design with declarative macro support
- 📖 Complete documentation and 21 runnable examples
- 🌟 Comprehensive features (rate limiting, bans, quotas, circuit breaking, fallback)

</details>

<details>
<summary><b>❓ Can this project be used in production?</b></summary>

<br>

**Yes.** The current 0.3.0-rc.x release line has validated its core capabilities in real projects:

- ✅ Core rate limiting and the storage abstraction are stable; the default build has zero external storage dependencies
- ✅ A layered test system of 3298 tests with the llvm-cov ≥90% line-coverage gate (see the [Testing Guide](TESTING.md))
- ✅ CI covers fmt / clippy / three-platform builds / cargo-deny / cargo-audit / CodeQL
- ✅ Performance numbers are reproducible via the repo's criterion benchmarks (`cargo bench --features full`)

> **Note:** check the [Changelog](../CHANGELOG.md) before upgrading; breaking changes are always annotated.

</details>

<details>
<summary><b>❓ Which platforms are supported?</b></summary>

<br>

| Platform | Architectures | Status | Notes |
|------|------|------|------|
| Linux | x86_64 / ARM64 | ✅ Supported | covered by the CI three-platform matrix |
| macOS | x86_64 / ARM64 | ✅ Supported | Apple Silicon covered |
| Windows | x86_64 | ✅ Supported | covered by the CI windows runner |

</details>

<details>
<summary><b>❓ Which programming languages are supported?</b></summary>

<br>

**Rust** is natively supported with full API access; online docs at [docs.rs/limiteron](https://docs.rs/limiteron). FFI access from other languages is a long-term idea; there are no official bindings today.

</details>

---

## 📦 Installation and Setup

<details>
<summary><b>❓ How do I install Limiteron?</b></summary>

<br>

**For Rust projects:**

```toml
[dependencies]
limiteron = { version = "0.3", features = ["macros"] }
```

Or with cargo:

```bash
cargo add limiteron --features macros
```

**Install from source:**

```bash
git clone https://github.com/Kirky-X/limiteron
cd limiteron
cargo build --release
```

**Verify the installation:**

```rust
use limiteron::limiters::TokenBucketLimiter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(10, 1);
    println!("✅ Installation successful!");
    Ok(())
}
```

**See also:** [User Guide](USER_GUIDE.md#installation)

</details>

<details>
<summary><b>❓ What are the system requirements?</b></summary>

<br>

| Component | Requirement | Recommended |
|------|------|------|
| Rust version | 1.97.1+ (pinned by [rust-toolchain.toml](../../rust-toolchain.toml)) | latest stable |
| Memory | 512 MB | 2 GB+ |
| Disk space | 50 MB | 100 MB |
| CPU | 1 core | 4+ cores |

**Optional:**

- PostgreSQL / MySQL / SQLite (persistence via dbnexus)
- Redis (caching and distributed rate limiting via oxcache)
- Docker (containerized deployment)

</details>

<details>
<summary><b>❓ What about compile errors?</b></summary>

<br>

**Common solutions:**

1. **Update the Rust toolchain:**
   ```bash
   rustup update stable
   ```

2. **Clean build artifacts:**
   ```bash
   cargo clean
   cargo build
   ```

3. **Check the Rust version:**
   ```bash
   rustc --version
   # should be 1.97.1 or higher
   ```

4. **Check the feature combination:** the three storage drivers `postgres` / `sqlite` / `mysql` are mutually exclusive; `--all-features` triggers dbnexus compile errors — use explicit feature combinations.

**Still stuck?**
- 📝 See [Troubleshooting](#-troubleshooting)
- 🐛 [Open an issue](https://github.com/Kirky-X/limiteron/issues) with the error details

</details>

<details>
<summary><b>❓ Can I use it in Docker?</b></summary>

<br>

**Yes!** Here is a sample Dockerfile:

```dockerfile
FROM rust:1.97-slim AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/limiteron /usr/local/bin/

CMD ["limiteron"]
```

**Docker Compose:**

```yaml
services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      - RUST_LOG=info
```

**Start the service with docker-compose:**

```bash
docker-compose up -d
```

</details>

---

## 💡 Usage and Features

<details>
<summary><b>❓ How do I get started with basic usage?</b></summary>

<br>

**5-minute quick start:**

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create the limiter
    let limiter = TokenBucketLimiter::new(10, 1);

    // 2. Check rate limits
    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("request {} ✅", i),
            Ok(false) => println!("request {} ❌", i),
            Err(e) => println!("request {} error: {:?}", i, e),
        }
    }

    Ok(())
}
```

**Next steps:**
- 📖 [User Guide](USER_GUIDE.md)
- 💻 [More examples](../../examples/)

</details>

<details>
<summary><b>❓ Which rate-limiting algorithms are supported?</b></summary>

<br>

**Token bucket, leaky bucket, sliding window (deprecated re-export) and sharded sliding window, sliding window log, fixed window, concurrency control, GCRA (the `gcra` feature), HTB hierarchical token bucket, AIMD adaptive concurrency (the `adaptive-limiting` feature)**. For a comparison of types and applicable scenarios per algorithm see the [User Guide](USER_GUIDE.md#1️-rate-limiters); for constructor signatures and feature requirements see [Algorithm details](API_REFERENCE.md#-rate-limiters).

</details>

<details>
<summary><b>❓ Can I use multiple limiters together?</b></summary>

<br>

**Yes!** Compose multiple limiters with a decision chain:

```rust
use limiteron::Governor;
use limiteron::adapters::StorageFactory;
use limiteron::matchers::RequestContext;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create storage
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let storage = factory.create_storage().await?;
    let ban_storage = factory.create_ban_storage().await?;

    // Create the Governor
    let governor = Governor::builder()
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await?;

    // The decision chain checks all limiters in priority order
    let context = RequestContext::new()
        .with_header("X-User-Id", "user123")
        .with_path("/api/v1/users")
        .with_method("GET");

    let decision = governor.check(&context).await?;
    match decision {
        limiteron::error::Decision::Allowed(_) => { /* handle the request */ }
        _ => { /* handle rejection or ban */ }
    }

    Ok(())
}
```

**Benefits:**
- 🔒 Layered protection
- 🎯 Finer control
- 📊 Better security

</details>

<details>
<summary><b>❓ How do I handle errors properly?</b></summary>

<br>

**Recommended pattern:**

```rust
use limiteron::error::LimiteronError;
use limiteron::limiters::TokenBucketLimiter;

async fn process_request(limiter: &TokenBucketLimiter) -> Result<(), LimiteronError> {
    match limiter.allow(1).await {
        Ok(true) => {
            println!("✅ success");
            Ok(())
        }
        Ok(false) => {
            println!("⚠️ rate limited");
            Ok(())
        }
        Err(LimiteronError::RateLimitExceeded(msg)) => {
            println!("⚠️ rate limited: {}", msg);
            Ok(())
        }
        Err(LimiteronError::BanError(msg)) => {
            eprintln!("❌ banned: {}", msg);
            Err(LimiteronError::BanError(msg))
        }
        Err(e) => {
            eprintln!("❌ error: {:?}", e);
            Err(e)
        }
    }
}
```

**Error types:**
- [Error reference](API_REFERENCE.md#-error-handling)

</details>

<details>
<summary><b>❓ Is async/await supported?</b></summary>

<br>

**Fully.** All core APIs are async (built on tokio), and limiters implement `Send + Sync` so they can be shared across tasks.

**Example:**

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(10, 1);

    match limiter.allow(1).await {
        Ok(true) => println!("✅ request allowed"),
        Ok(false) => println!("❌ request rate-limited"),
        Err(e) => println!("❌ error: {:?}", e),
    }

    Ok(())
}
```

**Using the macro:**

```rust
use limiteron::flow_control;

#[flow_control(rate = "10/s")]
async fn api_handler() -> Result<String, limiteron::error::LimiteronError> {
    Ok("Success".to_string())
}
```

</details>

---

## ⚡ Performance

<details>
<summary><b>❓ How fast is it?</b></summary>

<br>

**Token-bucket checks 12M+ ops/s (P99 < 1µs), fixed-window checks 20M+ ops/s, concurrency checks 12M+ ops/s** (measured 2026-01-19; for the full throughput/latency percentile tables see [README Performance](../../README.md#-performance)). Reproduce with `cargo bench --features full`.

</details>

<details>
<summary><b>❓ How do I improve performance?</b></summary>

<br>

**Optimization tips:**

1. **Enable Release mode:**
   ```bash
   cargo build --release
   ```

2. **Leverage the L1 negative cache:**
   ```rust
   // The L1 negative cache is enabled by default and has negative-cache semantics —
   // only "reject/ban" decisions are cached; "allow" decisions never enter the cache,
   // so every request re-executes the rate-limit/ban checks.
   let governor = Governor::builder()
       .with_l1_cache_enabled(true)
       .build()
       .await?;
   ```

3. **Use globally shared instances:**
   ```rust
   use limiteron::limiters::TokenBucketLimiter;
   use std::sync::LazyLock;

   static LIMITER: LazyLock<TokenBucketLimiter> =
       LazyLock::new(|| TokenBucketLimiter::new(10, 1));
   ```

4. **Use the macro:**
   ```rust
   #[flow_control(rate = "100/s")]
   async fn api_handler() -> Result<String, limiteron::error::LimiteronError> {
       Ok("Success".to_string())
   }
   ```

</details>

<details>
<summary><b>❓ What is memory usage like?</b></summary>

<br>

**Memory profile (qualitative):**

| Component | Memory profile | Notes |
|------|---------|------|
| Core limiters | KB-scale per instance | atomic counter state |
| L1 negative cache | bounded by the capacity cap | 1000 entries by default, adjustable via `L1CacheConfig` |
| MemoryStorage | grows with the number of active keys | the limiter manager has LRU eviction (100,000-entry cap) against flooding |
| Persistent backends | determined by connection-pool settings | dbnexus pool parameters are controllable |

**Reducing memory usage:**

```rust
// Reduce memory usage by bounding the L1 negative cache capacity
let governor = Governor::builder()
    .with_l1_cache_config(L1CacheConfig::new(Duration::from_secs(60), 1000))
    .build()
    .await?;
```

**Memory safety:**
- ✅ Rust memory-safety guarantees
- ✅ Limiter-manager LRU eviction prevents unbounded growth
- ✅ Background tasks implement `Drop` against leaks

</details>

---

## 🔒 Security

<details>
<summary><b>❓ Is this library secure?</b></summary>

<br>

**Security is a design throughout.** The six mechanism groups — input defenses, algorithm boundaries, Admin self-protection, data protection, transport defenses, and supply chain — are all traceable in the sources and the [Security document](SECURITY.md#️-security-design-overview); for per-mechanism details see its security design overview.

</details>

<details>
<summary><b>❓ How do I report a security vulnerability?</b></summary>

<br>

**Please report security issues responsibly:** do not open a public issue with vulnerability details; prefer GitHub private vulnerability reporting (repo page → Security → Report a vulnerability). Maintainers commit to acknowledging within 48 hours and giving an initial assessment within 7 days, using coordinated disclosure.

For report-content requirements (affected version, feature combination, reproduction steps, impact assessment) and the full process see the [Security document](SECURITY.md#-vulnerability-reporting-process).

</details>

<details>
<summary><b>❓ How is data stored?</b></summary>

<br>

**Data storage options:**

| Method | Best for | Notes |
|------|---------|------|
| In-memory | development, testing | out of the box; lost on restart |
| SQLite (via dbnexus) | single-node deployment | embedded persistence, no external service |
| PostgreSQL / MySQL (via dbnexus) | production deployment | parameterized queries against injection; TLS recommended |
| Redis (via oxcache / lua-script) | distributed caching and rate limiting | atomic Lua script operations |

**Best practices:**

```rust
// 1. Manage connection strings via environment variables
let dsn = std::env::var("DATABASE_URL")?;

// 2. Set appropriate permissions
// keep database access privileges minimal
```

</details>

<details>
<summary><b>❓ Are there known vulnerabilities?</b></summary>

<br>

**Current status:** ✅ **no known unfixed vulnerabilities**

**How we maintain security:**

1. **Dependency scanning:**
   ```bash
   cargo audit
   cargo deny check
   ```

2. **Regular updates:** CI and the pre-push hook continuously run security audits; known-vulnerable dependencies get minimum-version pinning (such as rustls-webpki, CVE-2025-48369)

3. **Testing:** security test suites (`security_tests`, `admin_security_tests`) and static analysis (CodeQL)

**Stay informed:**
- 🔔 Watch this repo
- 📰 Check the [Security Advisories](https://github.com/Kirky-X/limiteron/security/policy)

</details>

---

## 🔧 Troubleshooting

<details>
<summary><b>❓ Rate limiting is not taking effect</b></summary>

<br>

**Problem:**

```text
All requests pass through
```

**Cause:** a new limiter instance is created for every request.

**Solution:**

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};
use std::sync::Arc;

// Share the limiter instance via Arc
let limiter = Arc::new(TokenBucketLimiter::new(10, 1));

// Use the same instance across many requests
for i in 0..100 {
    let limiter_clone = Arc::clone(&limiter);
    tokio::spawn(async move {
        match limiter_clone.allow(1).await {
            Ok(true) => println!("request {} allowed", i),
            Ok(false) => println!("request {} rate-limited", i),
            Err(e) => println!("request {} error: {:?}", i, e),
        }
    });
}
```

</details>

<details>
<summary><b>❓ Performance is slower than expected</b></summary>

<br>

**Common causes**: not running in release mode, the L1 negative cache not enabled, or limiter instances not shared across requests. For the corresponding optimization commands and code see [Performance tips](#-performance); for the full throughput/latency data see [README Performance](../../README.md#-performance).

</details>

<details>
<summary><b>❓ Memory usage is too high</b></summary>

<br>

**Solution:** lower the L1 negative cache capacity to cut memory usage; for tuning code and each component's memory profile see [Performance](#-performance).

</details>

**More questions?** See the [User Guide troubleshooting](USER_GUIDE.md#-troubleshooting)

---

## 🤝 Contributing

<details>
<summary><b>❓ How can I contribute?</b></summary>

<br>

**Ways to contribute:**

| Code contributions | Non-code contributions |
|---------|-----------|
| 🐛 Fix bugs | 📖 Write tutorials |
| ✨ Add features | 🌍 Translate docs |
| 📝 Improve docs | 💬 Answer questions |
| ✅ Write tests | 🎨 Polish examples |

**Getting started:**

1. 🍴 Fork the repo
2. 🌱 Create a branch
3. ✏️ Make changes
4. ✅ Add tests
5. 📤 Open a PR

For the detailed process see the [Contributing Guide](CONTRIBUTING.md).

</details>

<details>
<summary><b>❓ I found a bug — what now?</b></summary>

<br>

**Before reporting:**

1. ✅ Check [existing issues](https://github.com/Kirky-X/limiteron/issues)
2. ✅ Try the latest version
3. ✅ Check the [troubleshooting guide](USER_GUIDE.md#-troubleshooting)

**A good bug report:**

```markdown
### Description
a clear description of the bug

### Reproduction steps
1. step one
2. step two
3. see the error

### Expected behavior
what should happen

### Actual behavior
what actually happened

### Environment
- OS: Ubuntu 22.04
- Rust version: 1.97.1
- limiteron version: 0.3.0-rc.7

### Additional context
any other relevant information
```

**Submit:** [Open an Issue](https://github.com/Kirky-X/limiteron/issues/new)

</details>

<details>
<summary><b>❓ Where can I get help?</b></summary>

<br>

| Channel | Purpose |
|------|------|
| 🐛 [GitHub Issues](https://github.com/Kirky-X/limiteron/issues) | bug reports and feature requests |
| 💬 [GitHub Discussions](https://github.com/Kirky-X/limiteron/discussions) | Q&A and ideas |
| 📦 [GitHub repo](https://github.com/Kirky-X/limiteron) | browse the source |

</details>

---

## 📄 License

<details>
<summary><b>❓ Which license is used?</b></summary>

<br>

**This project is released under the MIT license** and is free for commercial projects. For the full terms see [LICENSE](../../LICENSE); MIT has been in effect since v0.2.3 (previously Apache-2.0); see the [Changelog](../CHANGELOG.md) for the change record.

</details>

<details>
<summary><b>❓ Can I use it in commercial projects?</b></summary>

<br>

**Yes.** The project uses the MIT license, which permits commercial use and resale; you only need to keep the copyright and license notices. See [LICENSE](../../LICENSE) for the terms.

</details>

---

### 🎯 Other questions?

| Channel | Entry |
|------|------|
| 🐛 Open an Issue | [Issues](https://github.com/Kirky-X/limiteron/issues) |
| 💬 Start a discussion | [Discussions](https://github.com/Kirky-X/limiteron/discussions) |
| 🏠 GitHub repo | [Kirky-X/limiteron](https://github.com/Kirky-X/limiteron) |

---

[📖 User Guide](USER_GUIDE.md) • [📘 API Reference](API_REFERENCE.md) • [🏠 Home](../../README.md)
