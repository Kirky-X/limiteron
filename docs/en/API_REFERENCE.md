# 📘 Limiteron API Reference

This document fully describes Limiteron's public APIs, including rate limiters, ban management, quota control, the circuit breaker, the Governor, matchers, storage backends, the Admin REST API, configuration loading, and error types. All signatures match the `src/` sources (for the `0.3.x` release line). For usage methods and scenario examples see the [User Guide](USER_GUIDE.md).

[🏠 Home](../../README.md) • [📖 User Guide](USER_GUIDE.md) • [❓ FAQ](FAQ.md)

---

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [🧭 Overview](#-overview)
- [🚦 Rate Limiters](#-rate-limiters)
  - [Limiter trait](#limiter-trait)
  - [TokenBucketLimiter](#tokenbucketlimiter)
  - [GcraLimiter](#gcralimiter)
  - [Other rate-limiting algorithms](#other-rate-limiting-algorithms)
  - [Deprecated re-exports](#deprecated-re-exports)
- [🚪 Ban Management](#-ban-management)
  - [BanManager](#banmanager)
  - [BanTarget and BanSource](#bantarget-and-bansource)
  - [BanFileLoader](#banfileloader)
- [📊 Quota Control](#-quota-control)
- [🔌 Circuit Breaker](#-circuit-breaker)
- [🔁 Retry](#-retry)
- [🎛️ Governor](#️-governor)
- [🔍 Matchers](#-matchers)
- [💾 Storage Backends](#-storage-backends)
  - [MemoryStorage](#memorystorage)
  - [StorageFactory](#storagefactory)
- [🌐 Admin REST API](#-admin-rest-api)
  - [POST /api/v1/ban](#post-apiv1ban)
  - [DELETE /api/v1/ban/{target}](#delete-apiv1bantarget)
- [⚙️ Configuration Loading](#️-configuration-loading)
- [📨 Events and Webhooks](#-events-and-webhooks)
  - [EventDispatcher::add_webhook_url()](#eventdispatcheradd_webhook_url)
  - [EventDispatcher::try_add_webhook_url()](#eventdispatchertry_add_webhook_url)
- [🚨 Error Handling](#-error-handling)
- [📐 Type Definitions](#-type-definitions)
- [💡 Usage Examples](#-usage-examples)

</details>

---

## 🧭 Overview

| Design principle | Description |
|---------|------|
| Simple | core types are consolidated into a few modules; the `prelude` imports common types in one line |
| Safe | type-safe; default features are empty with zero external storage dependencies |
| Composable | the `Limiter` trait unifies algorithm interfaces; decision chains cascade by priority |
| Well documented | all public APIs carry doc comments, available online on docs.rs |

APIs gated by features are annotated with the required feature in the corresponding section. With `default = []`, only core rate limiting is available.

---

## 🚦 Rate Limiters

The rate-limiter module lives at `limiteron::limiters`; everything implements the unified `Limiter` trait.

### Limiter trait

```rust
#[async_trait]
pub trait Limiter: Send + Sync {
    /// Consume the given cost and return whether it is allowed
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError>;

    /// Non-consuming precheck: returns standard rate-limit header data, never mutates limiter state
    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError>;

    /// Non-consuming remaining-quota query
    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError>;

    /// Check whether allowed (accepts a key argument, used by macro-generated code)
    async fn check(&self, key: &str) -> Result<(), LimiteronError>;
}
```

The `RateLimitSnapshot` returned by `peek` / `remaining` maps to IETF `RateLimit-*` header data:

| Field | Type | Description |
|------|------|------|
| `limit` | `u64` | window/bucket capacity cap (`RateLimit-Limit`) |
| `remaining` | `u64` | current remaining quota (`RateLimit-Remaining`) |
| `reset_secs` | `u64` | seconds until quota reset (`RateLimit-Reset`) |

---

### TokenBucketLimiter

The token-bucket limiter.

#### `TokenBucketLimiter::new()`

```rust
pub fn new(capacity: u64, refill_rate: u64) -> Self
```

| Parameter | Type | Description |
|------|------|------|
| `capacity` | `u64` | bucket capacity (max tokens) |
| `refill_rate` | `u64` | tokens refilled per second |

#### `TokenBucketLimiter::allow()`

```rust
pub async fn allow(&self, cost: u64) -> Result<bool, LimiteronError>
```

Returns `Ok(true)` to allow and `Ok(false)` when rate-limited; a cost of 0 or above `MAX_COST` returns `ConfigError`.

**Example:**

```rust
use limiteron::limiters::TokenBucketLimiter;

let limiter = TokenBucketLimiter::new(10, 1); // 10 tokens, 1 refilled per second

match limiter.allow(1).await {
    Ok(true) => println!("✅ request allowed"),
    Ok(false) => println!("❌ request rate-limited"),
    Err(e) => println!("❌ error: {:?}", e),
}
```

---

### GcraLimiter

The GCRA (Generic Cell Rate Algorithm) limiter, requiring the `gcra` feature. Located at `limiteron::limiters::GcraLimiter`.

#### `GcraLimiter::new()`

```rust
pub fn new(capacity: u64, refill_interval_us: u64) -> Self
```

| Parameter | Type | Description |
|------|------|------|
| `capacity` | `u64` | bucket capacity (max tokens) |
| `refill_interval_us` | `u64` | refill interval per token (microseconds) |

#### `GcraLimiter::with_rate()`

```rust
pub fn with_rate(capacity: u64, requests_per_second: u64) -> Self
```

| Parameter | Type | Description |
|------|------|------|
| `capacity` | `u64` | bucket capacity (max tokens) |
| `requests_per_second` | `u64` | requests allowed per second |

#### `GcraLimiter::check()`

```rust
pub fn check(&self, cost: u64) -> GcraCheckResult
```

Returns a detailed synchronous check result:

| Field | Type | Description |
|------|------|------|
| `allowed` | `bool` | whether allowed |
| `remaining` | `u64` | remaining quota |
| `retry_after_us` | `u64` | suggested wait in microseconds when rejected |

**Example:**

```rust
use limiteron::limiters::GcraLimiter;

// capacity 10, 100 requests per second
let limiter = GcraLimiter::with_rate(10, 100);
let result = limiter.check(1);
if result.allowed {
    println!("✅ allowed, remaining: {}", result.remaining);
} else {
    println!("❌ rejected, wait {} microseconds", result.retry_after_us);
}
```

`GcraLimiter` also implements the `Limiter` trait and can be consumed asynchronously via `allow(cost).await`.

---

### Other rate-limiting algorithms

| Type | Constructor | Required feature | Description |
|------|------|---------|------|
| `LeakyBucketLimiter` | `new(capacity: u64, leak_rate: u64) -> Result<_>` | none | leaky-bucket water-level accounting (the dual of the token bucket), standard `Limiter` interface; the snapshot reports remaining bucket space and time to drain |
| `SlidingWindowLogLimiter` | `new(max_requests: u64, window: Duration) -> Result<_>` | none | exact sliding-window log without boundary spikes; memory grows linearly with requests in the window (100K-entry cap) |
| `FixedWindowLimiter` | `new(window_size: Duration, max_requests: u64)` | none | fixed-window counting |
| `ShardedSlidingWindowLimiter` | `new(window_size: Duration, max_requests: u64)` | none | sharded sliding window, friendly to high concurrency |
| `ConcurrencyLimiter` | `new(max_concurrent: u64)` | none | concurrency-permit control |
| `HierarchicalTokenBucket` | `new(root_capacity: u64, root_refill_rate: u64)` | none | HTB hierarchical token bucket; `allow(class_path: &[&str], cost)` consumes along a class path |
| `AdaptiveConcurrencyLimiter` | `new(config: AdaptiveConcurrencyConfig)` | `adaptive-limiting` | AIMD adaptive concurrency, window adjusted by latency/error-rate feedback |
| `QuotaLimiter` | see `limiters::quota_limiter` | `quota-control` | quota-based limiter, used by the macro and `LimiterManager` |

### Deprecated re-exports

> Since **v0.2.1**, `SlidingWindowLimiter` is no longer flat-exported via `limiteron::limiters`. Prefer `limiteron::limiters::ShardedSlidingWindowLimiter`, which offers better concurrent performance.
>
> It remains accessible via the full path `limiteron::limiters::sliding_window::SlidingWindowLimiter` (the module is marked `#[allow(deprecated)]`), but new code should not use it.

---

## 🚪 Ban Management

Requires the `ban-manager` feature. The module lives at `limiteron::ban`.

### BanManager

The ban manager, providing ban CRUD, exponential-backoff duration calculation, and the automatic-unban task.

#### `BanManager::new()` / `BanManager::builder()`

```rust
pub async fn new() -> Result<Self, LimiteronError>       // default in-memory storage
pub fn builder() -> BanManagerBuilder                     // chain configuration, then call build().await
```

#### `BanManager::with_dependencies()`

```rust
pub async fn with_dependencies(
    storage: Arc<dyn BanStorage>,
    config: BanManagerConfig,
) -> Result<Self, LimiteronError>
```

| Parameter | Type | Description |
|------|------|------|
| `storage` | `Arc<dyn BanStorage>` | ban storage backend |
| `config` | `BanManagerConfig` | ban manager configuration (backoff parameters, auto-unban switch, etc.) |

#### `BanManager::create_ban()`

```rust
pub async fn create_ban(
    &self,
    target: BanTarget,
    reason: String,
    source: BanSource,
    metadata: serde_json::Value,
    duration: Option<StdDuration>,
) -> Result<BanDetail, LimiteronError>
```

| Parameter | Type | Description |
|------|------|------|
| `target` | `BanTarget` | ban target |
| `reason` | `String` | ban reason |
| `source` | `BanSource` | `BanSource::Auto` or `BanSource::Manual { operator }` |
| `metadata` | `serde_json::Value` | additional metadata |
| `duration` | `Option<StdDuration>` | ban duration; `None` means it is computed automatically by the exponential-backoff algorithm |

#### `BanManager::is_banned()`

```rust
pub async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, LimiteronError>
```

`Some(BanRecord)` means banned. `BanRecord` carries `target`, `ban_times`, `duration`, `banned_at`, `expires_at`, `is_manual`, `reason`, and more.

**Example:**

```rust
use limiteron::ban::BanTarget;
use std::time::Duration;

let target = BanTarget::Ip("192.168.1.100".to_string());

// Create a ban
ban_manager.create_ban(
    target.clone(),
    "malicious request".to_string(),
    limiteron::ban::BanSource::Manual { operator: "admin".to_string() },
    serde_json::json!({}),
    Some(Duration::from_secs(3600)),
).await?;

// Query the ban
if let Some(record) = ban_manager.is_banned(&target).await? {
    println!("banned: {}, expires at: {}", record.reason, record.expires_at);
}
```

**Remaining methods at a glance:**

| Method | Description |
|------|------|
| `read_ban(&target)` | read ban details (`BanDetail`) |
| `update_ban(...)` | update a ban record |
| `delete_ban(&target, unbanned_by: String)` | unban (returns whether it succeeded) |
| `list_bans(filter: BanFilter)` | paginated/filtered ban list query |
| `calculate_ban_duration(ban_times)` | compute the ban duration via the backoff algorithm |
| `get_config()` / `update_config()` | read and update runtime configuration |
| `stop_auto_unban_task()` | stop the automatic-unban background task (cancels the signal first with a graceful 5s drain; abort only on timeout) |
| `is_auto_unban_running()` | whether the automatic-unban background task is running |

### BanTarget and BanSource

```rust
#[serde(tag = "type", content = "value")]
pub enum BanTarget {
    Ip(String),
    UserId(String),
    Mac(String),
    Geo { country_code: String }, // uppercase 2-letter ISO 3166-1 alpha-2
    Cidr(String),                 // IPv4/IPv6 range, e.g. "10.0.0.0/8"
}

pub enum BanSource {
    Auto,
    Manual { operator: String },
}
```

serde serialization format:

| Variant | type field | value format |
|------|----------|-----------|
| `Ip(String)` | `"ip"` | IP string |
| `UserId(String)` | `"user"` | user ID |
| `Mac(String)` | `"mac"` | MAC address |
| `Geo { country_code }` | `"geo"` | `{"country_code":"CN"}` |
| `Cidr(String)` | `"cidr"` | range string |

> **Query semantics**: when the query target is `Ip` and the exact match misses, range ban records are matched by longest prefix (`BanTarget::contains_ip` / `prefix_len`).

### BanFileLoader

Bulk-loads ban rules from a YAML file into a `BanManager`, with optional hot reload on file changes. Requires the `ban-manager` feature; hot reload additionally needs the `config-watcher` feature.

```rust
pub struct BanFileLoader {
    path: PathBuf,
    // under the config-watcher feature there is also a watcher task handle
}
```

**YAML file format:**

```yaml
bans:
  - target:
      type: ip              # ip | user | mac | geo | cidr
      value: "192.168.1.1"  # for geo: {country_code: "CN"}
    reason: "malicious request"
    duration_secs: 3600     # optional; null/omitted = use the backoff algorithm
```

| Method | Signature | Description |
|------|------|------|
| `new` | `pub fn new(path: impl Into<PathBuf>) -> Self` | create the loader |
| `load_once` | `pub async fn load_once(&self, manager: &BanManager) -> Result<LoadResult, LimiteronError>` | one-shot load; a single failed entry does not abort the rest, failure details in `LoadResult.errors`; only file read/parse failures return `Err` |
| `start_watching` | `pub async fn start_watching(&self, manager: BanManager) -> Result<(), LimiteronError>` | start hot reload (500ms debounce against DoS), needs `config-watcher` |
| `stop_watching` | `pub async fn stop_watching(&self)` | stop watching; called automatically on `Drop` |

```rust
pub struct LoadResult {
    pub success_count: usize,
    pub failure_count: usize,
    pub errors: Vec<BanLoadError>, // each carries target_desc and error
}
```

> **Security**: built-in YAML-bomb protection with a 2MB file-size cap; exceeding it returns `ConfigError`.

---

## 📊 Quota Control

Requires the `quota-control` feature. The module lives at `limiteron::quota`.

### QuotaController

#### `QuotaController::builder()`

```rust
pub fn builder() -> QuotaControllerBuilder
```

`QuotaControllerBuilder` provides:

| Method | Description |
|------|------|
| `with_storage(storage: Arc<dyn QuotaStorage>)` | set the quota storage backend |
| `with_config(config: QuotaConfig)` | set the quota configuration |
| `build()` | build and return `Result<QuotaController, LimiteronError>` |

#### `QuotaController::with_dependencies()`

```rust
pub fn with_dependencies(storage: Arc<dyn QuotaStorage>, config: QuotaConfig) -> Self
```

> **Note**: there is no parameterized `new(limit, window_size)` constructor; `QuotaController::new()` is a zero-argument default constructor (default in-memory storage and default configuration).

#### `QuotaController::consume()`

```rust
pub async fn consume(
    &self,
    user_id: &str,
    resource: &str,
    cost: u64,
) -> Result<ConsumeResult, LimiteronError>
```

The returned `ConsumeResult`:

| Field | Type | Description |
|------|------|------|
| `allowed` | `bool` | whether further consumption is allowed |
| `remaining` | `u64` | remaining quota |
| `alert_triggered` | `bool` | whether an alert fired (based on usage thresholds) |
| `usage_percent` | `f64` | current usage percentage (0-100) |

> **Note**: `user_id` / `resource` must not contain `:` (the storage key is `{user_id}:{resource}`; a `:` triggers the anti-collision `ValidationError`).

#### `QuotaConfig`

```rust
pub struct QuotaConfig {
    pub quota_type: QuotaType,          // quota type (default Count)
    pub limit: u64,                     // quota cap
    pub window_size: u64,               // window size (seconds)
    pub allow_overdraft: bool,          // whether overdraft is allowed
    pub overdraft_limit_percent: u8,    // overdraft cap (percentage of quota, 0-100)
    pub alert_config: AlertConfig,      // alert configuration
}
```

**Example:**

```rust
use limiteron::quota::{QuotaConfig, QuotaController};

let config = QuotaConfig {
    limit: 10000,
    window_size: 60,
    ..Default::default()
};
let quota = QuotaController::builder().with_config(config).build()?;

let result = quota.consume("user123", "api_call", 1).await?;
println!("remaining {}, usage {:.1}%", result.remaining, result.usage_percent);
```

---

## 🔌 Circuit Breaker

Requires the `circuit-breaker` feature. The module lives at `limiteron::circuit`.

### CircuitBreaker

```rust
pub fn new(config: CircuitBreakerConfig<E>) -> Self
```

`CircuitBreaker<E = LimiteronError>` also implements `Default` (default configuration, instantiable only with the default error type).

> **Note**: `CircuitBreaker` itself has no parameterized `new(failure_threshold, timeout_secs)` signature and no `with_config()` method — build a `CircuitBreakerConfig` and pass it to `new()` for custom configuration, or use `CircuitBreaker::builder()`. (The three-argument constructor lives on the **configuration type**: `CircuitBreakerConfig::new`, table below.)

#### `CircuitBreakerConfig`

```rust
pub struct CircuitBreakerConfig<E = LimiteronError> {
    pub failure_threshold: u64,          // failure threshold (default 5)
    pub success_threshold: u64,          // successes required to recover from half-open (default 3)
    pub timeout: Duration,               // wait duration in the open state (default 30 seconds)
    pub half_open_max_calls: u64,        // max probe calls in the half-open state (default 3)
    pub half_open_max_duration: Duration, // max dwell time of half-open probes (default 30 seconds)
    pub slow_call_duration_threshold: Duration, // slow-call duration threshold (default 500ms)
    pub slow_call_rate_threshold: f64,   // slow-call rate threshold (default 0.5)
    pub error_classifier: Arc<dyn FailureClassifier<E>>, // error classifier
}
```

The generic parameter `E` is the error type of the wrapped call. The default classifier `DefaultFailureClassifier` only implements
`FailureClassifier<LimiteronError>`; custom error types build the configuration via
`CircuitBreakerConfig::with_error_classifier(Arc<dyn FailureClassifier<E>>)`.

#### `CircuitCallError`

Circuit rejection is an explicit variant (shared with the sync `SyncCircuitBreaker`, defined in
`limiteron::error::CircuitCallError`):

- `CircuitCallError::Open`: the circuit is open (cooling down or half-open probe quota full); the closure did not run;
- `CircuitCallError::Inner(e)`: the call was let through but failed on its own; the original error `e` is passed through.

#### 0.3.0 breaking changes and migration

This release aligns the rejection semantics of the async and sync versions, with three breaking points:

1. The `execute` return type `Result<T, LimiteronError>` → `Result<T, CircuitCallError<E>>`.
   The old `match Err(LimiteronError::LimitError(msg)) | Err(LimiteronError::CircuitBreakerError(msg))`
   (circuit rejection) becomes `Err(CircuitCallError::Open)`; the call's own failure errors are now obtained from
   `Err(CircuitCallError::Inner(e))` (no longer sharing the rejection error channel).
2. The classifier trait `ErrorClassifier` was renamed `FailureClassifier<E>`: `impl ErrorClassifier for X`
   becomes `impl FailureClassifier<LimiteronError> for X` (method signatures unchanged).
3. The default implementation `DefaultErrorClassifier` was renamed `DefaultFailureClassifier`.

**Common methods:**

| Method | Description |
|------|------|
| `execute(op).await` | run the operation, returning `Result<T, CircuitCallError<E>>` (`op` returns `Result<T, E>`) |
| `get_state().await` | query the current state (`CircuitState`: Closed / Open / HalfOpen) |
| `config()` | read the effective configuration |
| `CircuitBreaker::builder()` | chained builder (`CircuitBreakerBuilder`), no need to assemble the config struct first |
| `CircuitBreakerConfig::new(failure_threshold: u64, success_threshold: u64, timeout: Duration)` | three-argument config constructor (covers only the three core thresholds; other fields take `Default`), available only for the default error type `LimiteronError` |
| `CircuitBreakerConfig::with_error_classifier(classifier: Arc<dyn FailureClassifier<E>>)` | config constructor with a custom classifier (the generic-error-type entry; other fields take `Default`) |

**Example:**

```rust
use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};

// Default configuration
let breaker = CircuitBreaker::new(CircuitBreakerConfig::default());
// equivalent to CircuitBreaker::default()

let state = breaker.get_state().await;
println!("current state: {:?}", state);
```

---

## 🔁 Retry

Requires the `retry` feature. The module lives at `limiteron::retry` and complements the circuit breaker: the breaker protects the downstream while retries absorb transient jitter.

### `RetryPolicy`

All fields are private, configured via chained methods (`derive(Debug, Clone)`; retry hooks are method arguments rather than policy fields — hook accounting state is captured by the caller's own closures):

| Method | Signature | Default |
|------|------|--------|
| `new` | `fn new(max_retries: u32, initial_delay: Duration) -> Self` | others take `Default` (factor 2.0 / max_delay 60s / no jitter) |
| `with_factor` | `fn with_factor(self, factor: f64) -> Self` | 2.0 |
| `with_max_delay` | `fn with_max_delay(self, max_delay: Duration) -> Self` | 60 seconds |
| `with_jitter` | `fn with_jitter(self, jitter: f64) -> Self` | 0.0 (range 0.0–1.0) |
| `with_budget_ratio` | `fn with_budget_ratio(self, ratio: f64) -> Self` | `None` (budget disabled) |
| `max_retries` | `fn max_retries(&self) -> u32` | 3 |

The `Default` configuration: `max_retries = 3`, `initial_delay = 500ms`, `factor = 2.0`, `max_delay = 60s`, `jitter = 0.0`, `budget_ratio = None`.
`max_retries` excludes the first call (0 = no retry, single execution); the execution entries run the operation at most `max_retries + 1` times,
waiting `min(initial × factor^(n-1), max_delay)` plus jitter before the n-th retry.
`budget_ratio` caps retries as a ratio of "total calls"; over-budget retries give up immediately (retry-storm protection:
when the downstream fails, retry traffic stays within total requests × budget_ratio).

### Execution entries

| Method | Signature | Feature requirement |
|------|------|----------|
| `execute` | `async fn execute<F, Fut, T, E, P>(&self, op: F, is_retryable: P) -> Result<T, E>`, with `F: FnMut() -> Fut`, `Fut: Future<Output = Result<T, E>>`, `P: Fn(&E) -> bool` | none |
| `execute_notify` | `async fn execute_notify<F, Fut, T, E, P, N>(&self, op: F, is_retryable: P, on_retry: N) -> Result<T, E>`, with `N: Fn(u32, &E)` | none |
| `execute_with` | `async fn execute_with<F, Fut, T, E, D>(&self, op: F, decide: D) -> Result<T, E>`, with `D: Fn(u32, &E) -> RetryDecision` | none |
| `execute_with_breaker` | `async fn execute_with_breaker<F, Fut, T, E, P>(&self, breaker: &CircuitBreaker, op: F, is_retryable: P) -> Result<T, E>` | `circuit-breaker` |

Semantic notes:

- `op` is an async operation factory invoked fresh on every retry; the return value is the last error when retries are exhausted, or the first non-retryable error.
- `execute_notify`'s `on_retry(attempt, err)` fires only for retries that **actually happen** (before that backoff wait; `attempt` starts at 1);
  non-retryable errors and retry exhaustion do not fire it — errors are reported via the return value. `execute` is `execute_notify` wrapped with an empty hook.
- `execute_with` **has no separate `is_retryable` classifier**: whether to retry is decided entirely by the decision callback (return `Stop` for non-retryable errors);
  once the hard loop constraints (count/budget) are exhausted, the classifier is no longer consulted — the same methodology as `execute`.
- `execute_with_breaker`: when the breaker is already open (or in half-open cooldown) the operation runs once and the error is returned as-is,
  injecting no retry traffic into a downstream already judged faulty.

### `RetryDecision`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    RetryAfter(Duration), // wait as instructed by the upstream, then retry
    Retry,                // wait on the policy's default backoff step, then retry
    Stop,                 // give up retrying immediately, return the triggering error as-is
}
```

- `RetryAfter(d)` is for errors carrying an authoritative wait duration (the rate-limit response's `Retry-After`, GCRA's
  `GcraCheckResult::retry_after_us` (the `gcra` feature), etc.); it is accounted **exactly like `Retry`** against the retry budget.
- The actual wait sleeps exactly the upstream-instructed value — **no jitter, no `max_delay` cap** (capping would make the consumer retry before the upstream-specified
  moment, violating the instruction contract: the rate-limit response would reject again with the same wait); the reasonableness of the instructed value is the decision callback's
  responsibility (e.g. impose your own cap on an HTTP `Retry-After`); the retry count remains hard-capped by `max_retries`, so total time is bounded.
- The jitter chain's `prev_delay` is accounted on the policy's own step (not the upstream-instructed value); the decorrelated jitter's `prev × 3`
  upper bound stays anchored to the policy's backoff sequence and neither collapses nor inflates due to a single upstream instruction.

### `delay_for_attempt` (free function)

```rust
pub fn delay_for_attempt(
    attempt: u32,
    initial: Duration,
    factor: f64,
    max_delay: Duration,
    jitter: f64,
) -> Duration
```

A pure function: `base = initial × factor^(attempt-1)` (`attempt` starts at 1), plus proportional jitter
`[1, 1+jitter]` (a linear-congruential pseudo-random, only for spreading retry spikes, **not for security purposes**), capped by `max_delay` (the cap is an absolute
upper bound: jitter is applied before capping, otherwise jitter could break `max_delay`). Consumers running their own retry loops use it to fetch
the delay per attempt number, sharing the same formula as `RetryPolicy::execute`'s internal backoff; when `RetryPolicy` enables `jitter`, it additionally
applies a decorrelated constraint on top (for `attempt > 1`, the delay's upper bound is also limited to the previous delay × 3).

**Example:**

```rust
use limiteron::retry::{RetryDecision, RetryPolicy};
use std::time::Duration;

let policy = RetryPolicy::new(3, Duration::from_millis(200))
    .with_factor(2.0)
    .with_max_delay(Duration::from_secs(5))
    .with_jitter(0.2)
    .with_budget_ratio(0.1); // retry traffic stays within 10% of total requests

// Wait as instructed when the upstream provides Retry-After; decide retryability for other errors yourself
let result = policy
    .execute_with(
        || fetch_upstream(),
        |attempt, err| match err {
            UpstreamError::RateLimited { retry_after } if attempt < 3 => {
                RetryDecision::RetryAfter(Duration::from_secs(retry_after))
            }
            UpstreamError::Transient(_) => RetryDecision::Retry,
            _ => RetryDecision::Stop,
        },
    )
    .await?;
```

> Choosing between `execute` / `execute_notify` and `execute_with`: use the `is_retryable` classifier with `execute` when error-classification
> information is limited; use `execute_with` when you need to read error-carried wait durations per attempt or make attempt-level judgments.

---

## 🎛️ Governor

The main controller providing end-to-end flow control: identifier extraction, rule matching, cascaded decision-chain execution, the negative cache, and statistics. The module lives at `limiteron::governor`.

### Construction

```rust
// Zero-argument construction: default in-memory storage, out of the box
pub async fn new() -> Result<Self, LimiteronError>

// Full-argument construction (metrics/tracer parameters appended per feature)
pub async fn with_storage(
    config: FlowControlConfig,
    storage: Arc<dyn Storage>,
    ban_storage: Arc<dyn BanStorage>,
    #[cfg(feature = "prometheus")] metrics: Option<Arc<Metrics>>,
    #[cfg(feature = "telemetry")] tracer: Option<Arc<Tracer>>,
) -> Result<Self, LimiteronError>

// builder pattern (recommended)
pub fn builder() -> GovernorBuilder
```

`GovernorBuilder` chained methods:

| Method | Description |
|------|------|
| `with_config(config: FlowControlConfig)` | inject the configuration |
| `with_storage(storage: Arc<dyn Storage>)` | inject the storage backend |
| `with_ban_storage(ban_storage: Arc<dyn BanStorage>)` | inject the ban storage |
| `with_metrics(metrics: Arc<Metrics>)` | inject the metrics collector (the `prometheus` feature) |
| `with_tracer(tracer: Arc<Tracer>)` | inject the tracer (the `telemetry` feature) |
| `with_audit_logger(logger: Arc<AuditLogger>)` | inject the audit logger (the `audit-log` feature) |
| `with_shutdown_snapshot_dir(dir)` | directory for statistics snapshots written on shutdown (JSON; not written by default) |
| `with_l1_cache_enabled(enabled: bool)` | toggle the L1 negative cache |
| `with_l1_cache_config(config: L1CacheConfig)` | customize the L1 negative cache (TTL and capacity) |
| `build().await` | build the Governor |

> **Note**: the Prometheus export of `Metrics` is provided by `start_prometheus_server` (the `prometheus` feature); calling it without the feature returns an error, whose message text was renamed with the feature from `monitoring feature is disabled` to `prometheus feature is disabled` (change disclosure in the [CHANGELOG](../CHANGELOG.md) Unreleased section).

**Example:**

```rust
use limiteron::Governor;
use limiteron::adapters::StorageFactory;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let storage = factory.create_storage().await?;
    let ban_storage = factory.create_ban_storage().await?;

    let governor = Governor::builder()
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await?;
    Ok(())
}
```

### `Governor::check()`

```rust
pub async fn check(&self, context: &RequestContext) -> Result<Decision, LimiteronError>
```

| Parameter | Type | Description |
|------|------|------|
| `context` | `&RequestContext` | request context (in `limiteron::matchers`) |

Returns a `Decision`: `Allowed(RateLimitMetadata)` / `Rejected(RejectionMetadata)` / `Banned(BanInfo)`.

### Lifecycle and Health Checks

| Method | Signature | Description |
|------|------|------|
| `shutdown` | `pub async fn shutdown(&self) -> Result<(), LimiteronError>` | graceful shutdown: stops the config hot-reload watcher, cancels background task tokens, optionally writes statistics snapshots, stops the auto-unban and audit-write tasks, and clears the L1 cache (idempotent; connection pools close with the last `Arc` reference release by the underlying layer) |
| `register_config_watcher_token` | `pub fn register_config_watcher_token(&self, token: tokio_util::sync::CancellationToken)` | register the config hot-reload watcher token (the return value of `watch_and_reload`); shutdown cancels it uniformly |
| `shutdown_token` | `pub fn shutdown_token(&self) -> &tokio_util::sync::CancellationToken` | get a reference to the shutdown token for async tasks listening for the shutdown signal |
| `is_shutdown` | `pub fn is_shutdown(&self) -> bool` | whether already shut down |
| `health_check` | `pub async fn health_check(&self) -> Result<(), LimiteronError>` | perform a real health check (storage, ban storage, and other dependencies) |
| `health_status` | `pub async fn health_status(&self) -> HealthStatus` | read the status snapshot of the most recent health check |

```rust
pub struct HealthStatus {
    pub storage_healthy: bool,
    pub ban_storage_healthy: bool,
    pub cache_healthy: bool,        // the L1 cache
    pub background_tasks_alive: bool,
}
```

**Example:**

```rust
use tokio_util::sync::CancellationToken;

// shutdown_token() returns a reference; clone it before moving into the async task
let token = governor.shutdown_token().clone();
tokio::spawn(async move {
    token.cancelled().await;
    println!("Governor is shutting down");
});

// Health check
governor.health_check().await?;
let status = governor.health_status().await;
if !status.storage_healthy {
    println!("⚠️ storage unavailable");
}
```

---

## 🔍 Matchers

The module lives at `limiteron::matchers`, responsible for identifier extraction and rule matching.

### Identifier

```rust
pub enum Identifier {
    UserId(String),
    Ip(String),
    Mac(String),
    ApiKey(String),
    DeviceId(String),
}
```

### RequestContext

The request context; fields are public and directly constructible, or built via chained methods:

```rust
pub struct RequestContext {
    pub user_id: Option<String>,
    pub ip: Option<String>,
    pub mac: Option<String>,
    pub device_id: Option<String>,
    pub api_key: Option<String>,
    pub headers: HashMap<String, String>,
    pub path: String,
    pub method: String,
    pub client_ip: Option<String>,
    pub query_params: HashMap<String, String>,
}

impl RequestContext {
    pub fn new() -> Self;
    pub fn with_header(mut self, key: &str, value: &str) -> Self;
    pub fn with_client_ip(mut self, ip: &str) -> Self;
    pub fn with_query_param(mut self, key: &str, value: &str) -> Self;
    pub fn with_path(mut self, path: &str) -> Self;
    pub fn with_method(mut self, method: &str) -> Self;
    pub fn get_header(&self, key: &str) -> Option<&String>;
}
```

### Extractors

Built-in identifier extractors uniformly implement the `IdentifierExtractor` trait:

| Extractor | Construction | Description |
|--------|------|------|
| `UserIdExtractor` | `new(header_name, query_param_name, default_user_id)` / `from_header(name)` / `builder()` | extract the user ID from an HTTP header or query parameter |
| `IpExtractor` | `new(header_names: Vec<String>, validate: bool)` / `builder()` | extract the IP from an HTTP header list by priority, with optional format validation and trusted-proxy configuration |
| `MacExtractor` | `new(...)` / `builder()` | MAC address extraction |
| `ApiKeyExtractor` | `from_header(name)` etc. | API Key extraction |
| `DeviceIdExtractor` | `new(...)` / `builder()` | device ID extraction |
| `CompositeExtractor` | `new(extractors, fallback_to_default)` / `builder()` | compose multiple extractors, tried in order |

**Example:**

```rust
use limiteron::matchers::IpExtractor;

// Create with the new method
let extractor = IpExtractor::new(
    vec!["X-Forwarded-For".to_string(), "X-Real-IP".to_string()],
    true,
);

// Or use the builder pattern
let extractor = IpExtractor::builder()
    .header_name("X-Forwarded-For")
    .header_name("X-Real-IP")
    .validate(true)
    .build();
```

### Rule Matching

| Type | Description |
|------|------|
| `RuleMatcher` | rule matching engine (`new(rules: Vec<Rule>)`) |
| `Matcher` | built-in match conditions (User / Ip etc., see `limiteron::config::Matcher`) |
| `CustomMatcher` / `CustomMatcherRegistry` | custom matcher trait and registry |
| `HeaderMatcher` | built-in Header matcher with `HeaderMatchMode` prefix/equality dual modes (the `match_mode` field of `with_match_mode` / `load_config`) |
| `MethodMatcher` | built-in request-method matcher with ASCII case normalization of method names (`new(methods: Vec<String>)`) |
| `TimeWindowMatcher` | built-in time-window matcher |
| `RegexPathMatcher` (the `regex-matching` feature) | path regex matcher, `new(pattern: &str, case_insensitive: bool)` |
| `GeoMatcher` (the `geo-matching` feature) | geo location condition matching |
| `DeviceMatcher` (the `device-matching` feature) | User-Agent parsing and device identification |

---

### Lua scripts (the `lua-script` feature)

`oxcache_lua` provides 7 Redis Lua script constants registered via `OxcacheLuaManager` (KEYS/ARGV conventions and return values in each const's docs in `src/oxcache_lua.rs`):

| Script | Semantics |
|------|------|
| `SLIDING_WINDOW_SCRIPT` | sliding window (ZSET, counting 1 per request) |
| `SLIDING_WINDOW_LOG_SCRIPT` | sliding window log (cost-weighted, with incremental ledger maintenance, amortized O(1); the 100K max_requests cap shares its origin with the in-process one) |
| `FIXED_WINDOW_SCRIPT` | fixed window (String + TTL) |
| `TOKEN_BUCKET_SCRIPT` | token bucket (strict balance judgment) |
| `BURST_TOKEN_BUCKET_SCRIPT` | token bucket with burst overdraft (borrow-to-allow with negative-balance accounting, burst_allowance ≤ capacity, TTL clamped at 24h) |
| `QUOTA_CONSUME_SCRIPT` / `QUOTA_RESET_SCRIPT` | quota consumption (with overdraft) and reset |

Execution entries: `execute_lua_script` / `load_script` + `execute_cached_script` (via oxcache `eval_lua`/`EVALSHA`).

---

## 💾 Storage Backends

The module lives at `limiteron::storage` and `limiteron::adapters`.

### Core Traits

| Trait | Responsibility |
|-------|------|
| `Storage` | rate-limiting data storage (KV operations for token buckets, counters, etc.) |
| `BanStorage` | ban record storage |
| `QuotaStorage` | quota data storage |

All three are injected into the Governor and components as `Arc<dyn Trait>`.

### MemoryStorage

The in-memory storage implementation, implementing `Storage` / `BanStorage` / `QuotaStorage` at once. Always available (no feature gating), suitable for single-instance development, testing, and rapid prototyping.

```rust
pub fn new() -> Self
pub fn create_storage() -> Arc<dyn Storage>   // convenience constructor (replacing the removed StorageCreate trait)
```

> **Note**: v0.2.1 removed `RedisStorage` and the `redis-storage` feature; caching is managed uniformly via oxcache (enable the `cache-redis` feature for the Redis cache backend; `cache-storage` is its compatibility alias).

**Example:**

```rust
use limiteron::storage::MemoryStorage;
use limiteron::Governor;

let storage = MemoryStorage::create_storage();
let governor = Governor::builder()
    .with_storage(storage)
    .build()
    .await?;
```

### StorageFactory

The factory creating persistent storage backends via dbnexus; requires one of `postgres` / `sqlite` / `mysql` (mutually exclusive).

```rust
pub struct StorageFactory { /* ... */ }

impl StorageFactory {
    pub fn from_dsn(dsn: impl Into<String>) -> Self;
    pub async fn initialize(&mut self, config: Option<StorageFactoryConfig>) -> Result<(), StorageError>;
    pub async fn create_schema(&self) -> Result<(), StorageError>;
    pub async fn create_storage(&self) -> Result<Arc<dyn Storage>, StorageError>;
    pub async fn create_ban_storage(&self) -> Result<Arc<dyn BanStorage>, StorageError>;
    pub async fn create_quota_storage(&self) -> Result<Arc<dyn QuotaStorage>, StorageError>;
}
```

**Example:**

```rust
use limiteron::adapters::StorageFactory;

let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
factory.initialize(None).await?;
// The table-creation dialect is chosen automatically by the connection backend (PostgreSQL / SQLite; MySQL does not support auto table creation and errors explicitly)
factory.create_schema().await?;
let storage = factory.create_storage().await?;
```

---

## 🌐 Admin REST API

With the `admin-api` feature enabled, Limiteron provides REST endpoints to manage bans, quotas, and status. Except the probe endpoints, all require `Authorization: Bearer <api_key>` header authentication (constant-time comparison against timing attacks; multi-key deployments can configure the `api_key_operators` mapping and the admin/viewer role matrix).

**Endpoint overview:**

| Method and path | Description | Auth |
|-----------|------|------|
| `GET /healthz` / `GET /readyz` | K8s probe endpoints | unauthenticated |
| `GET /metrics` | Prometheus metrics | unauthenticated (subject to an independent rate limit) |
| `GET /api/v1/status` | runtime status | Bearer |
| `GET /api/v1/status/circuit-breaker` | circuit breaker status | Bearer |
| `GET /api/v1/introspect` | rule/decision-chain/quota/ban/circuit introspection JSON | Bearer |
| `POST /api/v1/ban` | create a ban (needs `ban-manager`) | Bearer |
| `DELETE /api/v1/ban/{target}` | remove a ban (needs `ban-manager`; returns 400 for invalid targets, validated with the same methodology as creation) | Bearer |
| `PUT /api/v1/quota/{user_id}` | reset quota usage per user (`new_limit=0`; tenant-dimension quotas use `QuotaController::reset_quota_for_tenant`; returns 400 when `user_id`/`resource` contain `':'`) | Bearer |
| `POST /api/v1/config` | atomic hot config update | Bearer |
| `POST /api/v1/check/batch` | batch decision checks | Bearer |
| `POST /api/v1/tokens/prefetch` | batch token prefetch (`BatchTokenPrefetcher`) | Bearer |

> Admin endpoints ship path- and client-bucketed rate-limit self-protection (bucketed memory cap `RATE_BUCKET_MAX_ENTRIES=10000`).

**OpenAPI artifact (the `openapi` feature)**: for the complete contract see the committed artifact [`docs/openapi.json`](../openapi.json) (OpenAPI 3.0.3, 12 routes and schemas). The artifact is generated from the in-memory builder as the single source of truth, and the `tests/openapi_drift.rs` anti-drift guard guarantees byte-for-byte identity between the two — a contract change turns the light red; after rewriting with `UPDATE_OPENAPI=1 cargo test --features openapi --test openapi_drift`, the diff must be reviewed manually. For the protocol-agnostic operation surface see the `AdminService` trait (`src/admin/service.rs`): axum handlers and the thin client share one contract, with a unique error-category-to-status-code mapping (NotConfigured→503 / Invalid→400 / NotFound→404 / Forbidden→403 / Internal→500).

> **Error responses never leak internal information**: the response body of `Internal` errors is the fixed text `internal server error`; the original error text (which may contain storage-driver table/column/constraint names or SQL fragments) lands only in server-side logs. Likewise, the per-item `error` field of `POST /api/v1/check/batch` carries only stable categories (`validation` / `storage` / `config` / `authorization` / `throttled` / `ban` / `circuit_breaker` / `internal`); raw errors land only in logs.

**Thin client (the `admin-client` feature)**: `AdminClient::new(base_url, api_key)` provides typed methods mirroring `AdminService` one-to-one (`status` / `introspect` / `circuit_breaker_status` / `create_ban` / `delete_ban` / `reset_quota` / `apply_config` / `check_batch` / `healthz`). hyper http1 creates a connection per request, fitting low-frequency control-plane scenarios; a **temporary verification surface** — once the SDK generator (sdforge-R11) lands, a generated client takes over.

### Read-only admin Web UI (the `admin-ui` feature)

> ⚠️ **Unauthenticated by default**: this UI is designed for "snapshot viewing on the local machine / trusted intranet" and **provides no authentication by default**. It binds to `127.0.0.1:9091` by default (local machine only); switching to a non-loopback address exposes the admin snapshot (rules, statistics, health, circuit status) to every host on the segment — **you must front it with an authenticating reverse proxy (and terminate TLS there)**. On startup the server emits a `tracing::warn!` log for non-loopback bindings, leaving an explicit trace.
>
> **Optional access control (`WebUiAccess`)**: the routing layer carries two opt-in controls, off by default (existing behavior unchanged):
> - **Host validation** (`WebUiAccess::strict()`): allows IP literals, `localhost`, and domains in `allowed_hosts`; other DNS names get `421 Misdirected Request` (the narrowing surface for DNS rebinding). Strict mode breaks "access by domain name" deployments (including reverse proxies preserving the Host); such deployments must list the domain in `allowed_hosts`. The Host comes from the `Host` header, falling back to the URI authority when missing (HTTP/2 has only `:authority`); if both are missing the request is rejected.
> - **API key authentication** (`WebUiAccess::with_api_key(...)`): data endpoints (`/snapshot`, `/circuit-breaker`) require `Authorization: Bearer <key>` (scheme case-insensitive, RFC 7235 §2.1); missing or mismatched returns `401`; the key must be non-empty and ≥ 16 characters (same methodology as `AdminApiConfig`; `with_api_key` rejects violating keys at construction). The static page `/` contains no data and is exempt from credential checks (Host validation still covers all routes); failed authentication is rate-limited per client IP, returning `429` (with `Retry-After`) when failures within the window reach the cap.
>
> **Browser usage**: with an API key enabled, the page provides a key input entry (stored in `sessionStorage`, never in the URL, browser history, or reverse-proxy logs) and requests automatically carry `Authorization: Bearer <key>`; when absent or incorrect, the page renders an explicit "API key required" state.
>
> **Ban list desensitization**: `/snapshot`'s `active_bans` array carries only the aggregate surface — ban targets (IP / user ID / MAC / range) are masked per the `logging::redaction` methodology and ban reason text is removed entirely; ban count, manual flag, and expiry are kept. The Bearer-authenticated `/api/v1/introspect` returns the full ban list.

```rust
use limiteron::admin::{WebUiAccess, WebUiConfig, WebUiServer};

let config = WebUiConfig {
    host: "127.0.0.1".to_string(), // default; non-loopback requires a fronting reverse proxy
    port: 9091,
};
// Optional: strict Host validation + API key authentication
let server = WebUiServer::new(state, config).with_access(
    WebUiAccess::strict()
        .with_allowed_hosts(["admin.example.com"])
        .with_api_key("web-ui-key-16chars!!"),
);
server.start().await?;
```

**Mechanical read-only guards** (`src/admin/web.rs`; adding any write endpoint turns the light red):

1. The route table `ROUTE_TABLE` is the single source of truth for the path list; the Router is table-driven and registered only via `get()`;
2. Guard tests GET-probe every path in the table (missing registrations turn red) + send POST/PUT/DELETE to every path asserting 405;
3. Data handlers accept only `Arc<dyn ReadOnlySnapshotSource>` — the read-only projection trait of `AdminService` (only the three methods status/introspect/circuit-breaker); no write method exists at the type level.

The page is an embedded single page (no build chain, vanilla HTML+JS) that fetches `/snapshot` and `/circuit-breaker` to render aggregate statistics, component health, rules and decision chains, and circuit-breaker status, auto-refreshing every 5s.

### POST /api/v1/ban

Creates a ban record. Supports the five target types `ip` / `user` / `mac` / `geo` / `cidr`. Requires the `ban-manager` feature.

**Request body:**

```rust
pub struct CreateBanRequest {
    pub target: BanTarget,           // serde: {"type":"...","value":...}
    pub reason: String,
    pub operator: Option<String>,    // default "admin-api"
    pub duration_secs: Option<u64>,  // None = computed automatically by the backoff algorithm
}
```

**Response status codes:**

| Status | Meaning |
|--------|------|
| `201 Created` | ban created, returning `{id, ban_times, expires_at, is_manual}` |
| `400 Bad Request` | JSON syntax error or `ValidationError` (e.g. invalid IP, lowercase country code) |
| `401 Unauthorized` | missing or wrong `Authorization` header |
| `403 Forbidden` | `AuthorizationError` (authorization denied) |
| `422 Unprocessable Entity` | valid JSON but missing required fields (e.g. `reason`) |
| `503 Service Unavailable` | ban_manager not configured |
| `500 Internal Server Error` | other internal errors |

**Example:**

```bash
# IP ban
curl -X POST http://localhost:8080/api/v1/ban \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"target":{"type":"ip","value":"192.168.1.100"},"reason":"malicious request"}'

# Geo regional ban
curl -X POST http://localhost:8080/api/v1/ban \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"target":{"type":"geo","value":{"country_code":"CN"}},"reason":"regional ban","duration_secs":3600}'
```

**Successful response:**

```json
{
  "success": true,
  "message": "OK",
  "data": {
    "id": "ban-uuid",
    "ban_times": 1,
    "expires_at": 1783290000,
    "is_manual": true
  }
}
```

### DELETE /api/v1/ban/{target}

Removes a ban. Specify the target type explicitly via the `?type=` query parameter; when absent it is inferred automatically (valid IP → `ip`, otherwise → `user`). Requires the `ban-manager` feature.

**Query parameters:**

| Parameter | Values | Description |
|------|-----|------|
| `type` | `ip` / `user` / `mac` / `geo` / `cidr` | explicit target type; inferred automatically when absent |

**Request body (optional):**

```rust
pub struct UnbanRequest {
    pub reason: Option<String>,
    pub operator: Option<String>,
}
```

**Response status codes:**

| Status | Meaning |
|--------|------|
| `200 OK` | unbanned |
| `400 Bad Request` | unsupported `type` value, or invalid target format (validated with the same methodology as creation) |
| `401 Unauthorized` | missing or wrong `Authorization` header |
| `403 Forbidden` | `AuthorizationError` (authorization denied) |
| `404 Not Found` | target not banned |
| `503 Service Unavailable` | ban_manager not configured |
| `500 Internal Server Error` | other internal errors |

**Example:**

```bash
# Unban a Geo target (must specify type=geo explicitly)
curl -X DELETE "http://localhost:8080/api/v1/ban/CN?type=geo" \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"reason":"unban","operator":"admin-alice"}'

# Unban an IP (type may be omitted, inferred automatically)
curl -X DELETE "http://localhost:8080/api/v1/ban/192.168.1.100" \
  -H "Authorization: Bearer your-api-key"

# Unban a MAC (must specify type=mac explicitly)
curl -X DELETE "http://localhost:8080/api/v1/ban/00:1a:2b:3c:4d:5e?type=mac" \
  -H "Authorization: Bearer your-api-key"
```

---

## ⚙️ Configuration Loading

The module lives at `limiteron::config`.

### `ConfigLoader::load_from_file()`

```rust
pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, LimiteronError>
```

Loads the configuration from a TOML config file.

### `ConfigLoader::load_from_file_with_env()`

```rust
pub fn load_from_file_with_env<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, LimiteronError>
```

Loads the configuration from a TOML config file, with environment-variable overrides (prefix `LIMITERON_`).

| Environment variable | Overridden setting | Description |
|---------|-----------|------|
| `LIMITERON_GLOBAL_STORAGE` | `global.storage` | storage type: `memory` / `postgresql` / `redis` |
| `LIMITERON_GLOBAL_CACHE` | `global.cache` | cache type: `memory` / `redis` / `none` (via oxcache) |
| `LIMITERON_GLOBAL_METRICS` | `global.metrics` | metrics type: `prometheus` / `statsd` / `none` |

**Example:**

```rust
use limiteron::ConfigLoader;

// Set the environment variable override first
std::env::set_var("LIMITERON_GLOBAL_STORAGE", "postgres");

// Load the configuration (environment variables override same-named items in the config file)
let config = ConfigLoader::load_from_file_with_env("config.toml")?;
```

### Programmatic building

`limiteron::config::ConfigBuilder` provides programmatic building (`with_storage(StorageType)` / `with_cache(CacheBackend)` / `with_metrics(MetricsBackend)` / `with_trusted_proxies(TrustedProxyConfig)` / `with_rule(closure)` / `build()`). Note that `build()` requires at least one rule, and every rule must contain at least one matcher and one limiter; validation failure returns `Err(String)`. The rule builder `RuleBuilder` provides `id` / `name` / `priority` / `user_matcher` / `ip_matcher` / `token_bucket` / `fixed_window` / `sliding_window` / `concurrency_limit` and more.

---

## 📨 Events and Webhooks

The event system needs the `event-system` feature; webhook egress additionally needs the `webhook` feature. `EventDispatcher` distributes events to registered handlers and webhook targets.

### EventDispatcher::add_webhook_url()

```rust
pub async fn add_webhook_url(&self, url: String)
```

Registers a webhook target. When the `webhook` feature is enabled, an SSRF precheck runs first (scheme / intranet address ranges / domain resolution results); on failure the registration is rejected and logged as a warning; **the failure reason is not returned to the caller**.

### EventDispatcher::try_add_webhook_url()

```rust
pub async fn try_add_webhook_url(&self, url: String) -> Result<(), WebhookUrlError>
```

Synonymous with `add_webhook_url`, but returns the failure reason to the caller and does not register the URL on failure. Callers can thereby distinguish "permanent rejection" from "retryable transient failure":

```rust
match dispatcher.try_add_webhook_url(url).await {
    Ok(()) => {}
    Err(WebhookUrlError::Rejected(msg)) => eprintln!("❌ target rejected: {msg}"),
    Err(WebhookUrlError::DnsTransient(msg)) => eprintln!("⚠️ resolution temporarily failed, retryable: {msg}"),
}
```

When the `webhook` feature is not enabled, no validation runs and it always returns `Ok(())`.

---

## 🚨 Error Handling

### The `LimiteronError` enum

```rust
pub enum LimiteronError {
    ConfigError(String),
    StorageError(#[from] StorageError),
    LimitError(String),
    BanError(String),
    CircuitBreakerError(String),
    FallbackError(String),
    AuditLogError(String),
    AuthorizationError(String),
    IoError(#[from] std::io::Error),
    SerdeError(#[from] serde_json::Error),
    YamlError(#[from] serde_yaml_ng::Error),
    RateLimitExceeded(String),
    QuotaExceeded(String),
    ConcurrencyLimitExceeded(String),
    Throttled(String),           // macro throttle queue timeout
    ValidationError(String),
    LockError(String),
    TimeError(String),
    DependencyError(String),
    ParamMismatch(String),
    Other(String),
}
```

Type alias: `pub type LimiteronResult<T> = std::result::Result<T, LimiteronError>;`

### The `WebhookUrlError` enum

Lives in `limiteron::error`, distinguishing permanent rejection from transient failure in webhook URL validation:

```rust
pub enum WebhookUrlError {
    Rejected(String),      // permanent rejection: wrong scheme / intranet forbidden range hit / all resolution results inside forbidden ranges
    DnsTransient(String),  // transient failure: DNS resolution failure or precheck timeout (2s cap), retryable
}
```

The DNS precheck carries a 2s time cap: a hung resolver is reported as a transient failure and never hangs the event-dispatch send path.

### Error-handling patterns

```rust
// Pattern matching: distinguish rate limiting, bans, and other errors
match limiter.allow(1).await {
    Ok(true) => println!("✅ request allowed"),
    Ok(false) => println!("❌ request rate-limited"),
    Err(LimiteronError::LimitError(msg)) => {
        eprintln!("❌ rate-limit error: {}", msg);
    }
    Err(LimiteronError::BanError(msg)) => {
        eprintln!("❌ ban error: {}", msg);
    }
    Err(e) => {
        eprintln!("❌ error: {:?}", e);
    }
}
```

```rust
// The ? operator: propagate upward
async fn process_request() -> Result<(), LimiteronError> {
    let limiter = TokenBucketLimiter::new(10, 1);
    limiter.allow(1).await?;
    Ok(())
}
```

---

## 📐 Type Definitions

### Decision

The decision result, in `limiteron::error`:

```rust
pub enum Decision {
    Allowed(RateLimitMetadata),
    Rejected(RejectionMetadata),
    Banned(BanInfo),
}
```

| Carried type | Fields | Description |
|---------|------|------|
| `RateLimitMetadata` | `limit` / `remaining` / `reset_at` / `retry_after: Option<u64>` / `policy` | rate-limit metadata of an allow decision |
| `RejectionMetadata` | `reason` / `retry_after` / `limit` / `reset_at` | details of a reject decision |
| `BanInfo` | `reason()` / `banned_until()` / `ban_times()` | ban information (read via accessors) |

### FlowControlConfig

```rust
pub struct FlowControlConfig {
    pub version: String,
    pub global: GlobalConfig,
    pub rules: Vec<Rule>,
}

pub struct GlobalConfig {
    pub storage: StorageType,                 // Memory / PostgreSQL / Redis
    pub cache: CacheBackend,                  // Memory / Redis / None
    pub metrics: MetricsBackend,              // Prometheus / Statsd / None
    pub trusted_proxies: TrustedProxyConfig,  // trusted proxy configuration
}

pub struct TrustedProxyConfig {
    pub enabled: bool,          // whether trusted-proxy mode is enabled
    pub proxies: Vec<String>,   // trusted proxy IP list (CIDR supported)
    // plus an X-Forwarded-For max-hop limit (default 10)
}
```

### Other common types

| Type | Description |
|------|------|
| `LimiteronResult<T>` | alias of `Result<T, LimiteronError>` |
| `RateLimitSnapshot` | standard rate-limit header data returned by `peek` / `remaining` |
| `ConsumeResult` | quota consumption result (allowed / remaining / alert_triggered / usage_percent) |
| `LoadResult` | BanFileLoader load result |
| `CircuitState` | circuit breaker state: Closed / Open / HalfOpen |
| `ChainStats` | decision-chain statistics (total checks, per-node allow/reject counts) |
| `HealthStatus` | Governor health status snapshot |
| `Identifier` | identifier enum (UserId / Ip / Mac / ApiKey / DeviceId) |

---

## 💡 Usage Examples

### Example 1: Basic rate limiting

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(10, 1);

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

### Example 2: Ban management

```rust
use limiteron::adapters::StorageFactory;
use limiteron::ban::{BanManager, BanManagerConfig, BanSource, BanTarget};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let ban_storage = factory.create_ban_storage().await?;
    let ban_manager = BanManager::with_dependencies(ban_storage, BanManagerConfig::default()).await?;

    let ip_target = BanTarget::Ip("192.168.1.100".to_string());
    ban_manager.create_ban(
        ip_target.clone(),
        "malicious request".to_string(),
        BanSource::Manual { operator: "admin".to_string() },
        serde_json::json!({"severity": "high"}),
        Some(Duration::from_secs(3600)),
    ).await?;

    if let Some(record) = ban_manager.is_banned(&ip_target).await? {
        println!("❌ IP banned: {}", record.reason);
        println!("expires at: {}", record.expires_at);
    }
    Ok(())
}
```

### Example 3: Governor decision

```rust
use limiteron::adapters::StorageFactory;
use limiteron::error::Decision;
use limiteron::matchers::RequestContext;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let storage = factory.create_storage().await?;
    let ban_storage = factory.create_ban_storage().await?;

    let governor = limiteron::Governor::builder()
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await?;

    let context = RequestContext::new()
        .with_header("X-User-Id", "user123")
        .with_path("/api/v1/users")
        .with_method("GET");

    let decision = governor.check(&context).await?;
    match decision {
        Decision::Allowed(meta) => println!("✅ request allowed, remaining {}", meta.remaining),
        Decision::Rejected(meta) => println!("❌ request rejected: {}", meta.reason),
        Decision::Banned(info) => println!("❌ request banned: {}", info.reason()),
    }
    Ok(())
}
```

### Example 4: Declarative macro

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::LimiteronError> {
    Ok(format!("processed request for user {}", user_id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = api_handler("user123").await?;
    println!("{}", result);
    Ok(())
}
```

More runnable examples in [examples/](../../examples/) (21 examples covering all core capabilities).

---

[🏠 Home](../../README.md) • [📖 User Guide](USER_GUIDE.md) • [❓ FAQ](FAQ.md)
