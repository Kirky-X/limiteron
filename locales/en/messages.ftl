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
