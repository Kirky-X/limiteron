# 🔒 Limiteron Security

This document describes Limiteron's version support policy, vulnerability reporting process, security design, and best practices for secure usage. The security design and mechanisms all come from the project's actual implementation (see the [Architecture](ARCHITECTURE.md) and the sources); security fixes per version are recorded under the "security" category in the [Changelog](../CHANGELOG.md).

## 📋 Table of Contents

<details open>
<summary>Click to expand</summary>

- [📌 Supported Versions and Maintenance Policy](#-supported-versions-and-maintenance-policy)
- [🚨 Vulnerability Reporting Process](#-vulnerability-reporting-process)
- [🛡️ Security Design Overview](#️-security-design-overview)
- [✅ Security Best Practices](#-security-best-practices)

</details>

---

## 📌 Supported Versions and Maintenance Policy

This project is in the 0.x pre-release stage and **maintains only the current minor version line** (`0.3.x`, including rc pre-releases within that line).

- **Current line**: receives defect and security fixes, delivered with the latest release of that line.
- **Older minor lines**: enter EOL as soon as the first release of the next minor line ships; no fixes of any kind are provided, and security patches are not backported.
- **No LTS**: before 1.0 there is no cross-minor API compatibility promise; minor upgrades may include breaking changes — read the [CHANGELOG](../CHANGELOG.md) before upgrading.
- **Visible on crates.io ≠ supported**: old versions stay downloadable (unless yanked for severe defects) but are out of the support scope.

| Version line | Status | Notes |
|--------|------|------|
| `0.3.x` (current) | ✅ Supported | the only line receiving fixes |
| `≤ 0.2.x` | ❌ EOL | superseded by the current line, unmaintained — please upgrade |

> Security-related fixes in historical versions (such as the rustls-webpki version pinning for CVE-2025-48369) are not backported to EOL lines.

---

## 🚨 Vulnerability Reporting Process

1. **Reporting channel**: prefer the GitHub private vulnerability reporting channel **GitHub Security Advisories** (repo page → Security → Report a vulnerability) for private disclosure; the second fallback, when that is unavailable, is emailing Kirky-X@outlook.com. General issues may also be reported in [Issues](https://github.com/Kirky-X/limiteron/issues), but please **do not** include directly exploitable details, PoC payloads, or information affecting production environments.
2. **Report content**: include the affected version (the version in `Cargo.toml`), the feature combination involved, reproduction steps, and an impact assessment (e.g., rate-limit bypass, denial of service).
3. **Response and fixes**: maintainers will assess severity and schedule a fix after confirmation; fixes are developed on a private branch first and shipped with the next release.
4. **Disclosure convention**: coordinated disclosure. After the fix ships, it is recorded in the "security" category of the corresponding version in the [CHANGELOG](../CHANGELOG.md) (see the vuln-0001 ~ vuln-0004 fix series of 0.2.8 for reference), and the reporter is credited in discussions (with their consent).
5. **Non-security issues**: general usage questions go to [Issues](https://github.com/Kirky-X/limiteron/issues) or [Discussions](https://github.com/Kirky-X/limiteron/discussions); the security process is not needed.

### ⏱️ Response Commitments

- Acknowledgement within **48 hours**
- Initial assessment (accept / decline / need more info) within **7 days**
- Before the fix ships, the reporter gets a preview-validation opportunity of the patch
- After the fix ships, the reporter is credited in the CHANGELOG and the advisory (anonymously if preferred)

---

## 🛡️ Security Design Overview

The mechanisms below all have corresponding implementations in the sources and the architecture document.

### Memory Safety at the Language Layer

- All core code is written in Rust; ownership and borrow checking eliminate dangling pointers, buffer overruns, and other memory issues at compile time; unsafe code must document its safety invariants (see the contributing guide).
- Storage access goes uniformly through DBNexus / sea-orm parameterized queries, avoiding SQL injection via string concatenation.

### Boundary Handling in Rate-Limiting Algorithms

- **Token bucket** (`src/limiters/token_bucket.rs`): time deltas and token refill use `saturating_sub` / `saturating_add` and are capped with `.min(capacity)`, eliminating quota computation errors from integer overflow/underflow.
- **Quota windows** (`src/quota/`): built-in clock rollback protection — when NTP correction or container clock drift makes `now < window_start`, the window start is used, preventing rollback time from piercing the window-reset logic.
- **Clock abstraction** (`src/clock.rs`): the `Clock` trait unifies the time source and supports `MockClock` injection under the `test-clock` feature for deterministic testing of time-sensitive security logic.

### Input Validation and Injection Protection

- **Identifier sanitization** (`sanitize_tenant_id` / `sanitize_environment` in `src/tenant/config.rs`): escapes `:` in identifiers, preventing multi-tenant namespace prefix injection and cross-tenant prefix collisions.
- **Tenant identity origin** (`src/tenant/resolver.rs`): tenant identity is controlled by garrison (the SSO gateway) — the tenant ID is taken from a request header, and **that header may only be injected by garrison after authentication; the gateway must strip/overwrite the same header sent by end clients**; otherwise restrict acceptable tenant IDs with `HeaderTenantResolver::with_allowed_tenants` (non-matching values do not resolve to a tenant, preventing callers from choosing tenant namespaces and bypassing the banned namespace). A warning is logged once when the header is first resolved without a configured allowlist (the library has no authenticated identity to bind, so it can only leave an explicit trace). A warning is also logged once when the allowlist is configured empty: an empty allowlist funnels all tenant-header traffic into the default namespace (tenant-level isolation is defeated), which is a misconfiguration and never takes effect silently. Header values missing the allowlist fall back to the default namespace — spoofed traffic joins the global bucket but cannot obtain a self-chosen namespace; if switched to isolated namespaces, global bans no longer cover that traffic (a ban bypass), a trade-off decided by the caller. Scenarios bypassing garrison with direct connections should derive tenants from an authenticated principal (JWT claim / mTLS certificate / session); the library provides the `TenantResolver` extension point and `DefaultTenantResolver`.
- **Input validation** (`src/validation.rs`): format validation for identifiers such as IP addresses, user IDs, and MAC addresses.
- **Admin API self rate limiting** (`src/admin/routes.rs`): per-path, per-client bucketed rate limiting (the bucket window length is stored per entry; one group's counters are not cleared by another group) plus a bucketed memory cap (`RATE_BUCKET_MAX_ENTRIES=10000`) with expired-window sweeps, preventing OOM DoS from attackers rotating source IPs for unbounded memory growth; Mutex poisoning recovers instead of panicking. `/healthz` `/readyz` are entirely rate-limit-free (rate-limited K8s probes trigger Pod restarts); `/metrics` is authentication-free but subject to rate limiting. Group default quotas come from the single source of truth `config::default_rate_limit_for` (ban=100/min, quota=50/min, metrics=600/min, default=200/min): when configuration covers only some groups, uncovered groups fall back to the same quotas and are never loosened due to missing entries.
- **Control-plane credential strength** (`src/admin/routes.rs`): `routes::create_router` and `AdminServer::start` / `into_router` run `AdminApiConfig::validate()` with the same methodology — a primary `api_key` shorter than 16 characters must not act as a valid credential (calling the route constructor directly can no longer bypass this validation). Keys registered in `api_key_roles` are live credentials alongside the primary `api_key` and are held to the same 16-character floor: an over-short role key is not a valid credential (401) and is explicitly logged (only the role and length are reported, never the key itself). The public helper `AdminApiConfig::is_valid_api_key` uses the same methodology as the routes (the primary `api_key` must first pass strength validation to count as valid; role-mapping keys must satisfy the same floor), and credential comparison always uses constant-time comparison.

### SSRF Protection

- **Webhook URL validation** (`src/webhook_validator.rs`): rejects private, loopback, link-local, and unspecified addresses, and explicitly inspects the embedded IPv4 of IPv4-mapped IPv6 (such as `::ffff:10.0.0.1`) to close bypass paths. The forbidden ranges also cover **special-purpose ranges** — `0.0.0.0/8`, `100.64.0.0/10` (carrier-grade NAT, the default range of tunneling tools like Tailscale that can reach services on the host), `192.0.0.0/24`, `198.18.0.0/15`, `224.0.0.0/4` and above reserved ranges (multicast / broadcast); on the v6 side it covers `ff00::/8` multicast, `fec0::/10` site-local, IPv4-compatible spellings `::a.b.c.d`, and the NAT64 prefix `64:ff9b::/96` (judged by the embedded IPv4 rather than blacklisting whole ranges, to avoid hurting pure NAT64 deployments). Literal paths and the outbound resolver **share one judgment** (`forbidden_target_reason`), so the two can never drift apart. Validation repeats for **every redirect hop** (the outbound client uses a custom `redirect` policy and stops following on an intranet hit), preventing the "public address 302 → intranet" bypass of the initial URL check; the redirect chain also has a hop cap (reqwest's `Policy::custom` carries no chain-length limit by itself), terminating with an error when exceeded — hops are counted on reqwest's one-based scale (`attempt.previous().len()` includes the initial URL), so exactly `MAX_WEBHOOK_REDIRECTS` hops are allowed.
- **The outbound client ignores environment proxies** (`src/webhook_validator.rs`): an environment proxy (`ALL_PROXY` / `HTTP_PROXY` / `HTTPS_PROXY`) makes the **proxy** resolve the target hostname — locally it only resolves to the proxy host, and the target address escapes the SSRF resolver's judgment from then on. Hence outbound clients always `.no_proxy()`, and all name resolution goes through the SSRF-safe resolver.
- **Transport policy for redirect targets is decided by the caller** (`WebhookTransportPolicy`): event dispatch and quota alerts use different policies (the latter's payloads contain sensitive data and its redirect targets must always be HTTPS); policies are passed in via factory parameters and clients are cached per policy bucket — inferring from the build profile would apply one caller's policy to the other (plaintext downgrade or needless rejection).
- **Domain face (DNS resolution)**: beyond literal validation, the outbound client installs the SSRF-safe resolver — rejecting intranet/loopback/link-local targets at **the very resolution used for connecting** (the resolution result is the connection target; there is no "validate first, connect later" resolution-drift window; DNS rebinding is covered too). **The send path no longer repeats a DNS precheck**: the precheck and the connection are two resolutions, and a transient precheck failure would condemn a delivery that would otherwise succeed, so the domain-face judgment remains only at connection time (authoritative) and on the **registration path** (`validate_webhook_url_async`, which answers "is this endpoint reachable right now" and therefore must distinguish permanent rejection from transient failure). The registration-path precheck has a time cap (`WEBHOOK_DNS_PRECHECK_TIMEOUT`, 2s < the request's own 5s timeout): a hung resolver is reported as a transient failure and never occupies the caller indefinitely — the webhook fan-out of event dispatch is a sequential await, so one hang would stall the entire dispatch task and eventually backlog the broadcast channel and drop events.
- **Classified reporting of webhook validation results** (`WebhookUrlError` in `src/error/mod.rs`): validation failures are classified as "permanent rejection" (`Rejected`: wrong scheme / intranet forbidden range hit / all resolution results inside forbidden ranges) and "transient failure" (`DnsTransient`: DNS resolution failure or precheck timeout), the latter retryable. The registration-path `EventDispatcher::try_add_webhook_url` returns the classification to the caller (`add_webhook_url` remains a thin wrapper that only logs warnings), preventing a single resolution jitter from turning a valid endpoint into an unrecoverable loss.
- **Webhook URLs are never logged in full** (`logging::redaction::redact_url`): a webhook target's path/query may carry tokens (such as signed callback URLs); logs keep only the scheme and host — the redirect target is produced by the remote server, and echoing it in full writes remotely-controlled content into log files.

### Read-Only Admin Web UI (`admin-ui`)

- **Unauthenticated by default, loopback-bound by default**: the UI is designed for "viewing snapshots on the local machine", defaulting to `127.0.0.1` with no authentication; non-loopback bindings emit an explicit warning and require a fronting reverse proxy with authentication. The `/snapshot` ban list is desensitized (targets masked per the `logging::redaction` methodology, reasons removed).
- **Optional access control** (`WebUiAccess` in `src/admin/web.rs`): `WebUiAccess::strict()` enables a Host allowlist (only IP literals, `localhost`, and explicitly configured domains pass; other DNS names get 421, narrowing the DNS rebinding surface; the Host is taken from the `Host` header, falling back to the URI authority when missing — HTTP/2 has only `:authority`, and if both are missing the request is rejected); `WebUiAccess::with_api_key(...)` requires `Authorization: Bearer <key>` (scheme case-insensitive; missing/mismatched returns 401), the key must be non-empty and ≥ 16 characters (same methodology as `AdminApiConfig`; violating keys are rejected at construction), and failed authentication is rate-limited per client IP (reaching the cap within the window returns 429). The static page `/` contains no data and is exempt from credential checks (Host checks still cover all routes). Both are opt-in; default behavior matches existing deployments.
- **Credentials never leak**: `WebUiAccess` hand-writes its `Debug` impl (the api_key appears as `<redacted; N chars>`); a derived `Debug` would print the key in plaintext.
- **Known boundary**: by default (permissive Host + no authentication) the DNS rebinding surface is not closed at the routing layer; a fronting reverse proxy must enforce the Host allowlist. **Production reminder**: for non-local deployments explicitly enable `WebUiAccess::strict()` (or enforce the Host allowlist with a fronting reverse proxy); do not run long-term in the default permissive mode.

### Administrative Operation Authorization

- **Fine-grained authorization for manual bans/unbans** (`src/ban/types.rs`): `BanManager`'s `authorization_provider` is optional; when absent, construction warns once and each manual ban/unban warns (skipping the authorization chain must leave a trace). `BanManagerBuilder::with_required_authorization(true)` makes a build without an injected authorization provider fail outright (fail closed), eliminating the deployment state of "believing fine-grained authorization exists when it does not"; the control plane additionally has coarse-grained API key → role RBAC as a fallback.
- **`with_required_authorization` usage notes** (default `false`, enable as needed): once enabled it only enforces "an `authorization_provider` must be injected at build time"; the provider's authorization semantics (role/action mapping, whether denial fails closed) are the implementor's responsibility. The switch does not replace the control plane's API key → role RBAC, nor does it cover service-surface rate-limit decisions; after enabling, a build without a provider fails explicitly at build time rather than silently running without fine-grained authorization.
- **Scope of `with_required_authorization`**: fail closed takes effect only inside `BanManagerBuilder::build()`. `BanManager::new` / `BanManager::with_dependencies` / `BanManager::with_dependencies_and_auth` bypass this builder and cannot enable the constraint — without an injected authorization provider they still build successfully (warning only and skipping authorization checks per call). Deployments requiring fail closed must construct via `BanManager::builder()`.

### Sensitive Data Protection

- **Key masking** (`redact_key` in `src/limiters/manager.rs`): panic messages and logs never print full identifiers; short keys expose only the character count, long keys expose the first 8 characters + total length, truncation respects character boundaries to avoid UTF-8 panics.
- **Log redaction**: the `log-redaction` feature provides the masking engine (the engine is carried by inklog's `DataMasker` / `MaskRuleRegistry`, with rules and priorities defined uniquely on the inklog side); this feature is not in the `full` preset and must be enabled explicitly in dependency declarations, which unlocks the advanced masking APIs (`RedactionConfig` / `contains_sensitive_info` / `redact_advanced` / `redact_http_content`). The `audit-log` feature uses the `secrecy` crate to protect sensitive data in the audit chain. Basic masking functions (`redact_basic` / `redact_user_id` / `redact_ip` / `redact_email`) and sensitive-field-name detection always slice by **character** (not byte), so multi-byte UTF-8 (Chinese / emoji) input never triggers str-slice panics; the ≤4-character whole-value masking threshold of `redact_advanced` is likewise counted in **characters** (not bytes), so multi-byte short values are never missed.
- **Identity and forwarding headers** (`src/admin/routes.rs`, `src/middleware/`): Admin API operator identity binds to the API token actually submitted with the request; `X-Forwarded-For` is trusted only on direct connections from trusted proxies, preventing IP forgery.

### Resource and Availability Protection

- **Ban file loading** (`src/ban/file_loader.rs`): YAML ban files are capped at 2MB against YAML bombs; hot reload has a 500ms debounce against high-frequency file-change DoS.
- **Limiter LRU eviction** (`src/limiters/manager.rs`): `LimiterManager` evicts least-recently-used entries at a capacity (`MAX_LIMITER_ENTRIES=100_000`), preventing malicious keys from exhausting memory; cleanup uses CAS-based rate limiting to avoid blocking the request path.
- **Tasks and backpressure**: alert notification spawns use `Semaphore(8)` backpressure; `BanManager` / `EventDispatcher` implement `Drop` to prevent background task leaks; expired-ban cleanup avoids lock-held blocking operations, eliminating deadlock risk. The webhook send path of `EventDispatcher` performs an SSRF precheck before each send, capped at 2s — a slow resolver cannot occupy the single dispatch task indefinitely.

### Supply Chain and Dependency Security

### Config change audit trail

When the `audit-log` feature is enabled and an `AuditLogger` is configured, every successful hot config update (`POST /api/v1/config`) writes an `AuditEvent::ConfigChange` entry into the HMAC-signed audit chain. Event fields:

| Field | Meaning |
| --- | --- |
| `old_version` / `new_version` | Config versions before and after the swap |
| `changes` | Summary of rule-count changes |
| `operator` | Authenticated operator resolved from the API key mapping (`api_key_operators`), or `null` for unmapped keys |

A failed audit write is reported but does not roll back the applied config — the audit chain is an observability surface, not part of the admission decision. Verify chain integrity with `AuditLogger::verify_chain`.

- CI includes **cargo-deny security auditing** (the Security Audit job in `.github/workflows/ci.yml`, aligned with the RustSec Advisory DB) and **CodeQL static analysis** (`.github/workflows/codeql.yml`).
- Known-vulnerable dependencies get minimum-version pinning, such as `rustls-webpki` (CVE-2025-48369); dependencies are uniformly feature-gated (`optional = true`), and the default build (`default = []`) introduces no external storage/HTTP dependencies, minimizing the attack surface.

---

## ✅ Security Best Practices

When deploying and using Limiteron, follow:

1. **Minimize the feature surface**: enable only the features actually needed (the default `default = []` contains core rate limiting only), reducing code and dependencies compiled into the binary.
2. **Choose the storage backend explicitly**: `postgres` and `sqlite` are mutually exclusive — pick one per deployment shape; prefer PostgreSQL with TLS (`runtime-tokio-rustls`) in production.
3. **Protect the Admin API**: never expose the Admin REST API directly to the public internet; use network isolation / reverse-proxy access control; enable per-path rate limiting as needed and configure the `api_key_operators` mapping for multi-key deployments.
4. **Configure trusted proxies correctly**: trust `X-Forwarded-For` only when the fronting reverse proxy is confirmed trusted, otherwise client IPs can be forged to bypass IP-based rate limiting and bans.
5. **Validate external input**: enable URL validation for webhook callback targets (intranet addresses are rejected); when using YAML ban files, load only files from controlled sources (the 2MB cap + reload debounce are built in, but file permissions should be narrowed to read-only).
6. **Enable redaction and audit**: internet-facing services should explicitly enable `log-redaction` and `audit-log` (`log-redaction` is not in the `full` preset and must be enabled separately in dependency declarations) to keep identifiers and sensitive data out of logs.
7. **Keep dependencies updated**: track RustSec advisories (cargo-deny is enabled in CI) and upgrade to the latest versions carrying security fixes; when upgrading `kit` and other integration features, watch for breaking-change notes in the CHANGELOG.
8. **Read the changelog before upgrading**: security fixes are recorded uniformly under the "security" category of each version in the [CHANGELOG](../CHANGELOG.md); prioritize them when upgrading.
