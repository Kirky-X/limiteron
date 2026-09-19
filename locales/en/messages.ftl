# limiteron message catalog (en) — canonical English source of truth.
#
# Sections marked "canonical mirror" keep the English patterns identical to the
# canonical strings embedded at construction sites; render localized variants
# via limiteron::i18n::t(key, args). All fallbacks terminate in this catalog.

# ---- LimiteronError wrapper variants (dual-track: Display is canonical
# English, to_localized_string() resolves through these keys) ----
error-config = Configuration error: { $message }
error-limit = Rate limit error: { $message }
error-ban = Ban error: { $message }
error-circuit-breaker = Circuit breaker error: { $message }
error-fallback = Fallback error: { $message }
error-audit-log = Audit log error: { $message }
error-authorization = Authorization error: { $message }
error-io = IO error: { $message }
error-serde = Serialization error: { $message }
error-yaml = YAML parse error: { $message }
error-rate-limit-exceeded = Rate limit exceeded: { $message }
error-quota-exceeded = Quota exceeded: { $message }
error-concurrency-limit-exceeded = Concurrency limit exceeded: { $message }
error-throttled = Throttle queue timeout: { $message }
error-validation = Validation error: { $message }
error-lock = Lock acquisition error: { $message }
error-time = Time error: { $message }
error-dependency = Missing dependency: { $message }
error-other = Unknown error: { $message }

# ---- StorageError variants (LimiteronError::StorageError delegates here) ----
error-storage-connection = Connection error: { $message }
error-storage-query = Query error: { $message }
error-storage-timeout = Timeout error: { $message }
error-storage-not-found = Not found: { $message }
error-storage-authentication = Authentication error: { $message }
error-storage-permission = Permission error: { $message }
error-storage-invalid-config = Invalid configuration: { $message }
error-storage-rate-limit = Rate limit: { $message }
error-storage-validation = Validation error: { $message }

# ---- Already-English error types aligned with catalog keys (T016) ----
i18n-error-invalid-locale = invalid locale '{ $input }': { $reason }
i18n-error-invalid-number = invalid number '{ $input }': { $reason }
i18n-error-date = date error: { $message }
i18n-error-format = formatting error: { $message }
bulkhead-full = bulkhead '{ $name }' is full (max_concurrent exhausted)
bulkhead-circuit-open = bulkhead '{ $name }' circuit breaker is open
admin-config-api-key-required = API key is required when admin API is enabled
admin-config-api-key-too-short = API key must be at least 16 characters, got { $length }

# ---- Rate limit messaging (live keys) ----
rate-limit-exceeded = Rate limit exceeded
access-denied = Access denied
rate-limit-message = Rate limit exceeded: { $count }/{ $limit } requests per { $window }
window-second = second
window-minute = minute
window-hour = hour
window-day = day
decision-rejected-by = Rejected by { $node }: rate limit exceeded
limiter-create-failed = Failed to create limiter #{ $index }: { $reason }

# ---- Webhook URL validation (canonical mirror) ----
webhook-invalid-url = Invalid URL: { $reason }
webhook-https-required = Webhook URL must use HTTPS protocol
webhook-missing-host = URL is missing a host name
webhook-localhost-forbidden = localhost or loopback addresses are forbidden
webhook-loopback-forbidden = Loopback IP addresses are forbidden
webhook-unspecified-forbidden = Unspecified IP addresses are forbidden
webhook-private-forbidden = Private IP addresses are forbidden
webhook-link-local-forbidden = Link-local IP addresses are forbidden
webhook-private-mapped-forbidden = Private IP addresses are forbidden (IPv4-mapped IPv6 bypass attempt)
webhook-loopback-mapped-forbidden = Loopback addresses are forbidden (IPv4-mapped IPv6 bypass attempt)
webhook-link-local-mapped-forbidden = Link-local addresses are forbidden (IPv4-mapped IPv6 bypass attempt)
webhook-unspecified-mapped-forbidden = Unspecified addresses are forbidden (IPv4-mapped IPv6 bypass attempt)
webhook-unique-local-v6-forbidden = Unique local IPv6 addresses are forbidden
webhook-link-local-v6-forbidden = Link-local IPv6 addresses are forbidden

# ---- Ban file loader (canonical mirror) ----
ban-file-metadata-read-failed = Failed to read ban file metadata { $path }: { $reason }
ban-file-too-large = Ban file too large: { $path } ({ $size } bytes, limit { $limit } bytes)
ban-file-read-failed = Failed to read ban file { $path }: { $reason }
ban-file-yaml-parse-failed = Failed to parse ban file YAML { $path }: { $reason }
ban-file-load-task-failed = Ban file load task failed: { $reason }
ban-file-watch-start-failed = Failed to start file watcher: { $reason }
ban-file-watch-register-failed = Failed to register file watch: { $reason }

# ---- Config validation (canonical mirror) ----
config-invalid-proxy-address = Invalid proxy address '{ $address }': { $reason }
window-size-empty = Window size cannot be empty
window-size-missing-number = Invalid window size format: missing number part
window-size-missing-unit = Invalid window size format: missing unit
window-size-invalid-number = Invalid number format: { $input }
window-size-must-be-positive = Window size must be greater than 0
window-size-unsupported-unit = Unsupported unit: { $unit }. Supported units: ms, s, m, h, d
window-size-overflow = Window size overflow: { $number } * { $factor } seconds exceeds u64 range

# ---- CLI (live keys) ----
cli-warning-version-format = version '{ $version }' is not in x.y.z numeric form (semantic versioning recommended)
cli-warning-duplicate-priority = rule '{ $rule_id }' has duplicate priority { $priority } (match order ambiguity)

# ---- SafeErrorMessage abstraction (dual-track, T025) ----
safe-error-config = Configuration error: { $message }
safe-error-storage = Storage error: { $message }
safe-error-limit = Rate limit error: { $message }
safe-error-ban = Ban error: { $message }
safe-error-validation = Validation error: { $message }
safe-error-general = Error: { $message }
config-safe-invalid-format = Invalid configuration format
config-safe-missing-required-field = Missing required field
config-safe-duplicate-rule-id = Duplicate rule ID
config-safe-invalid-storage-type = Invalid storage type
config-safe-invalid-cache-type = Invalid cache type
config-safe-invalid-metrics-type = Invalid metrics type
config-safe-invalid-version = Invalid version
config-safe-rule-not-found = Rule not found
config-safe-invalid-limiter-config = Invalid limiter configuration
config-safe-invalid-matcher-config = Invalid matcher configuration
config-safe-value-out-of-range = Value out of allowed range
config-safe-malformed-pattern = Malformed pattern
config-safe-security-risk = Security risk detected
storage-safe-connection-failed = Connection failed
storage-safe-query-failed = Query failed
storage-safe-timeout = Operation timed out
storage-safe-not-found = Record not found
storage-safe-concurrent-modification = Data was concurrently modified
storage-safe-storage-full = Storage full
storage-safe-invalid-data-format = Invalid data format
limit-safe-rate-limit-exceeded = Request rate exceeded
limit-safe-quota-exceeded = Quota exhausted
limit-safe-concurrency-exceeded = Concurrency limit exceeded
limit-safe-token-bucket-empty = Tokens exhausted
limit-safe-window-full = Time window is full
limit-safe-too-many-requests = Too many requests
ban-safe-user-banned = User is banned
ban-safe-ip-banned = IP address is banned
ban-safe-device-banned = Device is banned
ban-safe-rate-exceeded = Request rate exceeded
ban-safe-spam-detected = Suspicious behavior detected
ban-safe-security-violation = Security check failed
validation-safe-invalid-input = Invalid input
validation-safe-malformed-data = Malformed data
validation-safe-security-check-failed = Security check failed
validation-safe-input-too-long = Input too long
validation-safe-invalid-format = Invalid format
validation-safe-suspicious-pattern = Suspicious pattern detected
general-safe-internal-error = Internal error
general-safe-service-unavailable = Service unavailable
general-safe-invalid-request = Invalid request
general-safe-unauthorized = Unauthorized
general-safe-forbidden = Forbidden
general-safe-rate-limited = Rate limited

# ---- Custom matcher registry validation (T025) ----
matcher-name-empty = Matcher name cannot be empty
matcher-name-too-long = Matcher name exceeds length limit (max { $max } characters)
matcher-name-invalid-chars = Matcher name may only contain letters, digits, underscores and hyphens
header-name-empty = HTTP header name cannot be empty
header-name-too-long = HTTP header name exceeds length limit (max { $max } characters)
header-name-invalid-chars = HTTP header name may only contain letters, digits and hyphens
header-value-too-long = HTTP header value exceeds length limit (max { $max } characters)
matcher-already-exists = Matcher '{ $name }' already exists
matcher-not-found = Matcher '{ $name }' does not exist
matcher-registered = Custom matcher registered: { $name }
matcher-unregistered = Custom matcher unregistered: { $name }
matcher-registry-cleared = All custom matchers cleared
matcher-config-missing = Missing { $field } configuration
matcher-hour-out-of-range = { $field } must be in range 0-23
matcher-time-window-config-loaded = Time window matcher config loaded: { $start }-{ $end }h
matcher-header-config-loaded = HTTP header matcher config loaded: header='{ $header }', allowed values={ $values }, case sensitive={ $case_sensitive }
matcher-allowed-values-too-many = Number of allowed values exceeds limit (max { $max })

# ---- Geo matcher (T025) ----
geo-db-not-found = GeoLite2 database file not found: { $path }. Please download GeoLite2-City.mmdb from the MaxMind website
geo-db-loading = Loading GeoLite2 database: { $path }
geo-db-size-below-typical = GeoLite2 database file is smaller than a typical full database ({ $size } bytes < { $typical } bytes) — Country/test/custom databases are normal; corrupt files will be rejected by format parsing
geo-db-size-above-max = GeoLite2 database file is too large ({ $size } bytes), may not be a standard file
geo-db-incomplete-read = GeoLite2 database file read incomplete, possibly truncated
geo-db-loaded = GeoLite2 database loaded successfully, size: { $size } bytes
geo-db-too-short-header = GeoLite2 database file too short to read file header
geo-db-unexpected-header = Unexpected GeoLite2 database file header: { $header }
geo-db-invalid = Invalid GeoLite2 database file: { $reason }
geo-db-metadata = GeoLite2 database metadata: version={ $version }, build date={ $build_epoch }, node count={ $node_count }
geo-cache-create-failed = Failed to create cache: { $reason }
geo-matcher-created = GeoMatcher created successfully
geo-ip-lookup-failed = IP lookup failed: { $reason }
geo-ip-decode-failed = IP data decode failed: { $reason }
geo-cache-cleared = Cache cleared, removed { $count } entries

# ---- Device matcher (T025) ----
device-matcher-creating = Creating DeviceMatcher
device-matcher-created = DeviceMatcher created successfully
device-custom-rule-invalid-regex-skipped = Invalid regex for custom rule '{ $name }', skipped: { $reason }
device-user-agent-too-long = User-Agent exceeds length limit (max { $max } characters)
device-invalid-regex = Invalid regular expression: { $pattern }
device-custom-rule-added = Custom rule added: { $name }
device-custom-rule-removed = Custom rule removed: { $name }
device-cache-cleared = Cache cleared, removed { $count } entries

# ---- IP range parsing (T025) ----
iprange-invalid-cidr = Invalid CIDR format: { $input }
iprange-invalid-ip = Invalid IP address: { $input }
iprange-invalid-prefix = Invalid prefix: { $input }
iprange-v4-prefix-too-large = IPv4 prefix cannot exceed 32: { $input }
iprange-v6-prefix-too-large = IPv6 prefix cannot exceed 128: { $input }
iprange-invalid-range = Invalid IP range format: { $input }
iprange-invalid-start-ip = Invalid start IP: { $input }
iprange-invalid-end-ip = Invalid end IP: { $input }
iprange-start-greater-than-end = Start IP cannot be greater than end IP: { $start } - { $end }
custom-matcher-not-integrated = Custom matcher '{ $name }' is not integrated with CustomMatcherRegistry; the rule will never match (its limits will not take effect)

# ---- IP / identifier extractors (T025) ----
xff-exceeds-max-hops = X-Forwarded-For contains { $count } IPs, exceeding the maximum limit { $max }
xff-ignored-untrusted-peer = X-Forwarded-For header ignored: direct connection from '{ $addr }' is not in the trusted proxy list (vuln-0003)
xff-ignored-trusted-proxy-disabled = Forwarded header extraction configured but trusted proxy mode not enabled; to prevent IP spoofing, forwarded headers are ignored and the direct connection IP is used (vuln-0003)
api-key-query-param-disabled = For security reasons, API key extraction via query parameters has been disabled

# ---- Governor lifecycle / island mode (T025) ----
governor-island-callback-registered = Island mode callback registered with FallbackManager
governor-request-banned = Request banned: user={ $user }, reason={ $reason }
governor-resource-banned = Resource banned: resource={ $resource }, reason={ $reason }
governor-storage-failure-no-l1 = Storage layer failure and L1 cache not enabled
governor-island-allow-all = Island mode - allowing all requests
governor-island-reject-all = Island mode - rejecting all requests
governor-island-reject-storage-failure = Island mode: storage layer failure, request rejected
governor-island-l1-miss-conservative = Island mode - L1 cache miss, using conservative strategy
governor-island-conservative-quota = Island mode - using conservative quota: { $max }/{ $window }s
governor-storage-failure-cache-miss = Storage layer failure, degraded cache miss
governor-resource-ban-check-unavailable = Parallel check disabled and ban manager not enabled; cannot perform resource ban check
governor-user-banned = User { $user } has been banned
governor-user-unbanned = User { $user } has been unbanned
governor-tenant-ban-applied = Identifier banned per tenant: namespace={ $namespace }, key={ $key }
governor-config-watcher-stopped = Config watcher stopped
governor-manual-config-check = Manual config check
governor-stats-reset = Statistics reset
governor-l1-cache-enabled = L1 cache enabled
governor-l1-cache-disabled = L1 cache disabled
governor-l1-cache-cleared = L1 cache cleared
governor-audit-logger-set = Audit logger set
governor-health-check = Health check
governor-already-shutdown = Governor already shut down; shutdown() is idempotent, returning Ok
governor-shutdown-started = Starting graceful shutdown of Governor
governor-shutdown-complete = Governor graceful shutdown complete

# ---- Fallback manager (T025) ----
fallback-manager-created = Fallback manager created
fallback-strategy-set = Fallback strategy set: component={ $component }, strategy={ $strategy }
fallback-component-op-failed = Component operation failed: component={ $component }, error={ $error }
fallback-strategy-executing = Executing fallback strategy: component={ $component }, strategy={ $strategy }
fallback-fail-open = Fallback strategy: FailOpen - returning fallback error, caller decides whether to allow
fallback-fail-open-error = Service degraded (FailOpen): component failure, caller decides whether to allow
fallback-fail-closed = Fallback strategy: FailClosed - rejecting request
fallback-fail-closed-error = Service degraded, request rejected
fallback-component-failed = Component failure: { $component }
fallback-component-recovered = Component recovered: { $component }
fallback-component-failure-recorded = Component failure recorded: { $component }
fallback-failure-injected = Failure injected: { $component }
fallback-failure-recovered = Failure recovered: { $component }
fallback-island-callback-registered = Island mode notification callback registered
fallback-island-enter-notified = Notified all callbacks: entering island mode
fallback-island-exit-notified = Notified all callbacks: exiting island mode
fallback-first-failure-island = First storage layer failure, triggering island mode
fallback-all-recovered-island-exit = All storage layers recovered, exiting island mode

# ---- Circuit breaker (T025) ----
circuit-created = Circuit breaker created: failure_threshold={ $failure_threshold }, success_threshold={ $success_threshold }, timeout={ $timeout }
circuit-open-rejecting = Circuit breaker open, rejecting request
circuit-open-request-rejected = Circuit breaker open, request rejected
circuit-half-open-limit-reached = Half-open state call limit reached, rejecting request
circuit-half-open-limit-exceeded = Half-open state call limit exceeded
circuit-success-while-open = Success response received while circuit breaker open
circuit-half-open-probe-failed = Half-open probe failed (state drifted to Closed in the meantime), re-opening circuit
circuit-failure-while-open = Failure response received while circuit breaker open
circuit-slow-call-rate-exceeded = Slow call rate exceeded threshold: { $rate }% >= { $threshold }%, opening circuit
circuit-state-changed-open = Circuit breaker state changed: { $old_state } -> Open (failure_count={ $failure_count })
circuit-state-changed-half-open = Circuit breaker state changed: { $old_state } -> HalfOpen
circuit-state-changed-closed = Circuit breaker state changed: { $old_state } -> Closed
circuit-reset = Circuit breaker reset

# ---- L1 cache island mode (T025) ----
l1-cache-island-entered = L1 cache entered island mode: strategy={ $strategy }
l1-cache-island-exited = L1 cache exited island mode

# ---- Admin API (T025) ----
admin-api-disabled = Admin API disabled
admin-api-started = Admin API server started: http://{ $address }
admin-api-key-operator-fallback = API key has no operator mapping, falling back to default 'admin-api'; configure an explicit mapping via AdminApiConfig::with_api_key_operator to prevent operator identity spoofing
admin-rbac-denied = RBAC denied: role={ $role } has no access to { $method } { $path }

# ---- Audit log (T025) ----
audit-batch-task-panic = Audit log batch task panicked: { $reason }
audit-entry = Audit log: { $json }
audit-entry-missing-signature = Log entry is missing a signature
audit-entry-parse-failed = Failed to parse audit log entry: { $reason }
audit-file-write-failed = Failed to write audit log file: { $path }: { $reason }
audit-logger-created = Audit logger created: enabled={ $enabled }, signing_enabled={ $signing_enabled }
audit-logger-stopped = Audit logger stopped
audit-send-ban-event-failed = Failed to send ban operation event: { $reason }
audit-send-config-change-failed = Failed to send config change event: { $reason }
audit-send-decision-event-failed = Failed to send decision event: { $reason }
audit-send-error-event-failed = Failed to send error event: { $reason }
audit-send-system-event-failed = Failed to send system event: { $reason }
audit-serialize-failed = Failed to serialize audit log: { $reason }
audit-signature-verification-failed = Signature verification failed, log may have been tampered with
audit-tampered-entry-discarded = Audit log signature verification failed, discarding tampered entry: { $reason }
audit-write-task-ended = Audit log write task ended

# ---- Authorization (T025) ----
authz-operator-not-authorized = Operator '{ $operator }' is not authorized to perform this operation
authz-operator-not-authorized-for-operation = Operator '{ $operator }' is not authorized to perform operation '{ $operation }'
authz-unknown-operation = Unknown operation type: '{ $operation }'

# ---- Ban manager / file loader (T025) ----
ban-check-error-fail-open = Ban check failed, treating as not banned (fail-open): { $error }
ban-check-storage-error-fail-open = Ban check storage error, treating as not banned (fail-open): target={ $target }, error={ $error }
ban-check-timeout-fail-open = Ban check timed out ({ $timeout }), treating as not banned (fail-open): target={ $target }
ban-file-changed-reloading = Ban file changed, triggering reload: { $path }
ban-file-entry-load-failed = Failed to load ban from file: target={ $target }, error={ $error }
ban-file-existing-lookup-failed-create-new = Failed to query existing ban, treating as new: target={ $target }, error={ $error }
ban-file-load-complete = Ban file load complete: { $count } entries
ban-file-load-partial-failure = Ban file load had failures: { $success } succeeded, { $failure } failed
ban-file-reload-complete = Ban file reload complete: { $success } succeeded, { $failure } failed
ban-file-reload-failed = Ban file reload failed: { $reason }
ban-manager-no-authorization-provider = BanManager created without authorization_provider; all manual ban operations will skip fine-grained authorization checks (admin API key authentication only)
ban-manual-skip-authorization = Manual ban skipping authorization check (no authorization_provider configured): operator={ $operator }, target={ $target }
ban-reason-empty = Ban reason cannot be empty
ban-reason-invalid-chars = Ban reason contains illegal characters
ban-reason-too-long = Ban reason too long, maximum length is { $max } characters

# ---- Cache storage (T025) ----
cache-ban-index-cas-retries-exhausted = ban index CAS retries exhausted (concurrency conflicts too high)
cache-ban-record-cas-retries-exhausted = ban record CAS retries exhausted (concurrency conflicts too high)
cache-ban-times-overflow-u32 = ban_times out of u32 range: { $reason }
cache-quota-counter-parse-failed = quota counter parse failed

# ---- Concurrency limiter (T025) ----
concurrency-permit-acquire-timeout = Timed out acquiring permit
concurrency-permits-overflow-u32 = Permit count out of u32 range
concurrency-semaphore-closed = Semaphore closed

# ---- Custom matcher registry integration (T025) ----
custom-matcher-eval-failed-no-match = Custom matcher '{ $name }' evaluation failed, treated as no match: { $error }
custom-matcher-not-registered = Custom matcher '{ $name }' is not registered in the registry; the rule will never match (its limits will not take effect)
custom-matcher-pending-contract-violation = Custom matcher returned Pending (violates the "first poll is Ready" contract), treated as no match
custom-matcher-resolved-from-registry = Custom matcher '{ $name }' resolved from registry and active

# ---- Limiter factory validation (T025) ----
limiter-capacity-must-be-positive = Token bucket capacity must be greater than 0
limiter-capacity-too-large = Token bucket capacity too large, maximum is { $max }
limiter-custom-requires-registry = Custom limiter type must be handled by CustomLimiterRegistry
limiter-max-concurrent-must-be-positive = Concurrency limit must be greater than 0
limiter-max-concurrent-too-large = Concurrency limit too large, maximum is { $max }
limiter-max-requests-must-be-positive = { $limiter_type } max requests must be greater than 0
limiter-max-requests-too-large = { $limiter_type } max requests too large, maximum is { $max }
limiter-quota-requires-controller = Quota limiter type must be handled by QuotaController
limiter-refill-rate-must-be-positive = Token bucket refill rate must be greater than 0
limiter-refill-rate-too-large = Token bucket refill rate too large, maximum is { $max }

# ---- Quota alerts (T025) ----
quota-alert-concurrency-limit = Alert concurrency limit of { $max } reached, skipping this alert: user_id={ $user_id }, resource={ $resource }, threshold={ $threshold }%
quota-alert-triggered = Quota alert triggered: user_id={ $user_id }, resource={ $resource }, quota_type={ $quota_type }, threshold={ $threshold }%, current_usage={ $current_usage }, limit={ $limit }, triggered_at={ $triggered_at }
quota-alert-webhook-disabled = Webhook feature not enabled; enable the 'webhook' feature
quota-alert-webhook-error-status = Webhook returned error status code: { $status }
quota-alert-webhook-send-failed = Failed to send webhook alert: { $reason }
quota-init-conflict-retries-exhausted = Concurrent quota initialization conflict retries exhausted, please retry the request

# ---- Telemetry (T025) ----
telemetry-noop-metrics-gather = Metrics is in no-op state: the `monitoring` feature is not enabled; this and subsequent gather() calls return an empty string and metric data is discarded. Enable the `monitoring` feature in Cargo.toml to collect real metrics.

# ---- Validation (T025) ----
validation-country-code-empty = Country code cannot be empty
validation-country-code-length = Country code must be 2 letters (ISO 3166-1 alpha-2), got { $length } characters: { $code }
validation-country-code-uppercase = Country code must be uppercase letters: { $code }

# ---- Telemetry alerts (T025) ----
alert-sent-critical = Critical alert sent: { $level }
alert-sent-info = Info alert sent: { $level }
alert-sent-warning = Warning alert sent: { $level }
