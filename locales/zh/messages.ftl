# limiteron 消息目录（zh）— 与 locales/en/messages.ftl 键齐（守卫测试保证）。
#
# 标注 canonical mirror 的段：英文规范串在构造点直出英文，本目录提供对应的
# 中文渲染面（经 limiteron::i18n::t(key, args) 取用）。所有回退终结于 en。

# ---- LimiteronError 包装变体（双轨：Display 为英文规范串，
# to_localized_string() 经这些键解析） ----
error-config = 配置错误: { $message }
error-limit = 限流错误: { $message }
error-ban = 封禁错误: { $message }
error-circuit-breaker = 熔断器错误: { $message }
error-fallback = 降级错误: { $message }
error-audit-log = 审计日志错误: { $message }
error-authorization = 授权错误: { $message }
error-io = IO错误: { $message }
error-serde = 序列化错误: { $message }
error-yaml = YAML解析错误: { $message }
error-rate-limit-exceeded = 速率限制超出: { $message }
error-quota-exceeded = 配额超出: { $message }
error-concurrency-limit-exceeded = 并发限制超出: { $message }
error-throttled = 排队超时: { $message }
error-validation = 验证错误: { $message }
error-lock = 锁获取错误: { $message }
error-time = 时间错误: { $message }
error-dependency = 依赖缺失: { $message }
error-other = 未知错误: { $message }

# ---- StorageError 变体（LimiteronError::StorageError 委托至此） ----
error-storage-connection = 连接错误: { $message }
error-storage-query = 查询错误: { $message }
error-storage-timeout = 超时错误: { $message }
error-storage-not-found = 未找到: { $message }
error-storage-authentication = 认证错误: { $message }
error-storage-permission = 权限错误: { $message }
error-storage-invalid-config = 无效配置: { $message }
error-storage-rate-limit = 速率限制: { $message }
error-storage-validation = 验证错误: { $message }

# ---- 已英文错误类型补键对齐（T016） ----
i18n-error-invalid-locale = 无效的 locale '{ $input }': { $reason }
i18n-error-invalid-number = 无效的数字 '{ $input }': { $reason }
i18n-error-date = 日期错误: { $message }
i18n-error-format = 格式化错误: { $message }
bulkhead-full = 舱壁 '{ $name }' 已满（max_concurrent 已耗尽）
bulkhead-circuit-open = 舱壁 '{ $name }' 熔断器已打开
admin-config-api-key-required = 启用 admin API 时必须提供 API key
admin-config-api-key-too-short = API key 必须至少 16 个字符，当前 { $length }

# ---- 限流消息（活跃键） ----
rate-limit-exceeded = 超出速率限制
access-denied = 访问被拒绝
rate-limit-message = 已超出限流：每个{ $window } { $count }/{ $limit } 个请求
window-second = 秒
window-minute = 分钟
window-hour = 小时
window-day = 天
decision-rejected-by = 已被 { $node } 拒绝：超出速率限制
limiter-create-failed = 创建第 { $index } 个限流器失败: { $reason }

# ---- Webhook URL 校验（canonical mirror） ----
webhook-invalid-url = 无效的 URL: { $reason }
webhook-https-required = Webhook URL 必须使用 HTTPS 协议
webhook-missing-host = URL 缺少主机名
webhook-localhost-forbidden = 禁止使用 localhost 或回环地址
webhook-loopback-forbidden = 禁止使用回环 IP 地址
webhook-unspecified-forbidden = 禁止使用未指定 IP 地址
webhook-private-forbidden = 禁止使用私有 IP 地址
webhook-link-local-forbidden = 禁止使用链路本地地址
webhook-private-mapped-forbidden = 禁止使用私有 IP 地址（IPv4-mapped IPv6 绕过尝试）
webhook-loopback-mapped-forbidden = 禁止使用回环地址（IPv4-mapped IPv6 绕过尝试）
webhook-link-local-mapped-forbidden = 禁止使用链路本地地址（IPv4-mapped IPv6 绕过尝试）
webhook-unspecified-mapped-forbidden = 禁止使用未指定地址（IPv4-mapped IPv6 绕过尝试）
webhook-unique-local-v6-forbidden = 禁止使用唯一本地 IPv6 地址
webhook-link-local-v6-forbidden = 禁止使用链路本地 IPv6 地址

# ---- 封禁文件加载（canonical mirror） ----
ban-file-metadata-read-failed = 读取封禁文件元数据失败 { $path }: { $reason }
ban-file-too-large = 封禁文件过大: { $path }（{ $size } bytes, 上限 { $limit } bytes）
ban-file-read-failed = 读取封禁文件失败 { $path }: { $reason }
ban-file-yaml-parse-failed = 解析封禁文件 YAML 失败 { $path }: { $reason }
ban-file-load-task-failed = 封禁文件加载任务失败: { $reason }
ban-file-watch-start-failed = 启动文件监听失败: { $reason }
ban-file-watch-register-failed = 注册文件监听失败: { $reason }

# ---- 配置校验（canonical mirror） ----
config-invalid-proxy-address = 无效的代理地址 '{ $address }': { $reason }
window-size-empty = 窗口大小不能为空
window-size-missing-number = 窗口大小格式错误：缺少数字部分
window-size-missing-unit = 窗口大小格式错误：缺少单位
window-size-invalid-number = 无效的数字格式: { $input }
window-size-must-be-positive = 窗口大小必须大于0
window-size-unsupported-unit = 不支持的单位: { $unit }。支持的单位: ms, s, m, h, d
window-size-overflow = 窗口大小溢出: { $number } * { $factor } 秒超出 u64 范围

# ---- CLI（活跃键） ----
cli-warning-version-format = 版本 '{ $version }' 不是 x.y.z 数字形态（语义化版本建议）
cli-warning-duplicate-priority = 规则 '{ $rule_id }' 的 priority { $priority } 与其他规则重复（匹配顺序歧义）

# ---- 错误消息抽象（SafeErrorMessage 双轨，T025） ----
safe-error-config = 配置错误: { $message }
safe-error-storage = 存储错误: { $message }
safe-error-limit = 限流错误: { $message }
safe-error-ban = 封禁错误: { $message }
safe-error-validation = 验证错误: { $message }
safe-error-general = 错误: { $message }
config-safe-invalid-format = 配置格式无效
config-safe-missing-required-field = 缺少必需字段
config-safe-duplicate-rule-id = 规则ID重复
config-safe-invalid-storage-type = 无效的存储类型
config-safe-invalid-cache-type = 无效的缓存类型
config-safe-invalid-metrics-type = 无效的指标类型
config-safe-invalid-version = 版本号无效
config-safe-rule-not-found = 规则不存在
config-safe-invalid-limiter-config = 限流器配置无效
config-safe-invalid-matcher-config = 匹配器配置无效
config-safe-value-out-of-range = 值超出允许范围
config-safe-malformed-pattern = 模式格式错误
config-safe-security-risk = 检测到安全风险
storage-safe-connection-failed = 连接失败
storage-safe-query-failed = 查询失败
storage-safe-timeout = 操作超时
storage-safe-not-found = 记录不存在
storage-safe-concurrent-modification = 数据被并发修改
storage-safe-storage-full = 存储空间不足
storage-safe-invalid-data-format = 数据格式无效
limit-safe-rate-limit-exceeded = 请求频率超出限制
limit-safe-quota-exceeded = 配额已用尽
limit-safe-concurrency-exceeded = 并发请求数超出限制
limit-safe-token-bucket-empty = 令牌已用尽
limit-safe-window-full = 时间窗口已满
limit-safe-too-many-requests = 请求过于频繁
ban-safe-user-banned = 用户已被封禁
ban-safe-ip-banned = IP地址已被封禁
ban-safe-device-banned = 设备已被封禁
ban-safe-rate-exceeded = 请求频率超出限制
ban-safe-spam-detected = 检测到可疑行为
ban-safe-security-violation = 安全检查未通过
validation-safe-invalid-input = 输入无效
validation-safe-malformed-data = 数据格式错误
validation-safe-security-check-failed = 安全检查失败
validation-safe-input-too-long = 输入过长
validation-safe-invalid-format = 格式无效
validation-safe-suspicious-pattern = 检测到可疑模式
general-safe-internal-error = 内部错误
general-safe-service-unavailable = 服务不可用
general-safe-invalid-request = 请求无效
general-safe-unauthorized = 未授权
general-safe-forbidden = 禁止访问
general-safe-rate-limited = 请求被限流

# ---- 自定义匹配器注册表校验（T025） ----
matcher-name-empty = 匹配器名称不能为空
matcher-name-too-long = 匹配器名称长度超过限制（最大 { $max } 字符）
matcher-name-invalid-chars = 匹配器名称只能包含字母、数字、下划线和连字符
header-name-empty = HTTP头名称不能为空
header-name-too-long = HTTP头名称长度超过限制（最大 { $max } 字符）
header-name-invalid-chars = HTTP头名称只能包含字母、数字和连字符
header-value-too-long = HTTP头值长度超过限制（最大 { $max } 字符）
matcher-already-exists = 匹配器 '{ $name }' 已存在
matcher-not-found = 匹配器 '{ $name }' 不存在
matcher-registered = 注册自定义匹配器: { $name }
matcher-unregistered = 注销自定义匹配器: { $name }
matcher-registry-cleared = 清空所有自定义匹配器
matcher-config-missing = 缺少 { $field } 配置
matcher-hour-out-of-range = { $field } 必须在 0-23 范围内
matcher-time-window-config-loaded = 加载时间窗口匹配器配置: { $start }-{ $end }小时
matcher-header-config-loaded = 加载HTTP头匹配器配置: 头='{ $header }', 允许值={ $values }, 区分大小写={ $case_sensitive }
matcher-allowed-values-too-many = 允许的值数量超过限制（最大 { $max }）

# ---- 地理匹配器（T025） ----
geo-db-not-found = GeoLite2数据库文件不存在: { $path }。请从MaxMind官网下载GeoLite2-City.mmdb文件
geo-db-loading = 加载GeoLite2数据库: { $path }
geo-db-size-below-typical = GeoLite2数据库文件小于典型全量库大小（{ $size } bytes < { $typical } bytes）——Country/测试/自定义库属正常，损坏文件将由格式解析阶段拒绝
geo-db-size-above-max = GeoLite2数据库文件过大（{ $size } bytes），可能不是标准文件
geo-db-incomplete-read = GeoLite2数据库文件读取不完整，可能被截断
geo-db-loaded = GeoLite2数据库加载成功，大小: { $size } bytes
geo-db-too-short-header = GeoLite2数据库文件过短，无法读取文件头
geo-db-unexpected-header = GeoLite2数据库文件头格式异常: { $header }
geo-db-invalid = 无效的GeoLite2数据库文件: { $reason }
geo-db-metadata = GeoLite2数据库元数据: 版本={ $version }, 构建日期={ $build_epoch }, 记录数={ $node_count }
geo-cache-create-failed = 创建缓存失败: { $reason }
geo-matcher-created = GeoMatcher创建成功
geo-ip-lookup-failed = IP查询失败: { $reason }
geo-ip-decode-failed = IP数据解析失败: { $reason }
geo-cache-cleared = 缓存已清空，移除 { $count } 条记录

# ---- 设备匹配器（T025） ----
device-matcher-creating = 创建DeviceMatcher
device-matcher-created = DeviceMatcher创建成功
device-custom-rule-invalid-regex-skipped = 自定义规则 '{ $name }' 的正则无效，已跳过: { $reason }
device-user-agent-too-long = User-Agent 长度超过限制（最大 { $max } 字符）
device-invalid-regex = 无效的正则表达式: { $pattern }
device-custom-rule-added = 添加自定义规则: { $name }
device-custom-rule-removed = 移除自定义规则: { $name }
device-cache-cleared = 缓存已清空，移除 { $count } 条记录

# ---- IP 范围解析（T025） ----
iprange-invalid-cidr = 无效的CIDR格式: { $input }
iprange-invalid-ip = 无效的IP地址: { $input }
iprange-invalid-prefix = 无效的前缀: { $input }
iprange-v4-prefix-too-large = IPv4前缀不能超过32: { $input }
iprange-v6-prefix-too-large = IPv6前缀不能超过128: { $input }
iprange-invalid-range = 无效的IP范围格式: { $input }
iprange-invalid-start-ip = 无效的起始IP: { $input }
iprange-invalid-end-ip = 无效的结束IP: { $input }
iprange-start-greater-than-end = 起始IP不能大于结束IP: { $start } - { $end }
custom-matcher-not-integrated = 自定义匹配器 '{ $name }' 未集成 CustomMatcherRegistry，该规则将恒不匹配（其限制不会生效）

# ---- IP / 标识符提取器（T025） ----
xff-exceeds-max-hops = X-Forwarded-For 包含 { $count } 个 IP,超过最大限制 { $max }
xff-ignored-untrusted-peer = X-Forwarded-For 头被忽略：直接连接来自 '{ $addr }'，不在可信代理列表中（vuln-0003）
xff-ignored-trusted-proxy-disabled = 已配置转发头提取但未启用可信代理模式；为防止 IP 伪造，忽略转发头并使用直接连接 IP（vuln-0003）
api-key-query-param-disabled = 出于安全考虑，通过查询参数提取API Key已被禁用

# ---- Governor 生命周期 / 孤岛模式（T025） ----
governor-island-callback-registered = 已注册孤岛模式回调到 FallbackManager
governor-request-banned = 请求被封禁: 用户={ $user }, 原因={ $reason }
governor-resource-banned = 资源被封禁: 资源={ $resource }, 原因={ $reason }
governor-storage-failure-no-l1 = 存储层故障且 L1 缓存未启用
governor-island-allow-all = 孤岛模式 - 允许所有请求通过
governor-island-reject-all = 孤岛模式 - 拒绝所有请求
governor-island-reject-storage-failure = 孤岛模式：存储层故障，拒绝请求
governor-island-l1-miss-conservative = 孤岛模式 - L1 缓存未命中，使用保守策略
governor-island-conservative-quota = 孤岛模式 - 使用保守配额: { $max }/{ $window }s
governor-storage-failure-cache-miss = 存储层故障，降级缓存未命中
governor-resource-ban-check-unavailable = 并行检查已禁用且未启用封禁管理器，无法执行资源封禁检查
governor-user-banned = 用户 { $user } 已被封禁
governor-user-unbanned = 用户 { $user } 已解封
governor-tenant-ban-applied = 标识符已按租户封禁: namespace={ $namespace }, key={ $key }
governor-config-watcher-stopped = 停止配置监视器
governor-manual-config-check = 手动配置检查
governor-stats-reset = 重置统计信息
governor-l1-cache-enabled = L1 缓存已启用
governor-l1-cache-disabled = L1 缓存已禁用
governor-l1-cache-cleared = L1 缓存已清空
governor-audit-logger-set = 审计日志记录器已设置
governor-health-check = 健康检查
governor-already-shutdown = Governor 已关闭，shutdown() 幂等返回 Ok
governor-shutdown-started = 开始优雅关闭 Governor
governor-shutdown-complete = Governor 优雅关闭完成

# ---- 降级策略管理器（T025） ----
fallback-manager-created = 创建降级策略管理器
fallback-strategy-set = 设置降级策略: component={ $component }, strategy={ $strategy }
fallback-component-op-failed = 组件操作失败: component={ $component }, error={ $error }
fallback-strategy-executing = 执行降级策略: component={ $component }, strategy={ $strategy }
fallback-fail-open = 降级策略: FailOpen - 返回降级错误，由调用方决定放行
fallback-fail-open-error = 服务降级（FailOpen）：组件故障，是否放行由调用方决定
fallback-fail-closed = 降级策略: FailClosed - 拒绝请求
fallback-fail-closed-error = 服务降级，拒绝请求
fallback-component-failed = 组件故障: { $component }
fallback-component-recovered = 组件恢复: { $component }
fallback-component-failure-recorded = 组件故障记录: { $component }
fallback-failure-injected = 注入故障: { $component }
fallback-failure-recovered = 恢复故障: { $component }
fallback-island-callback-registered = 注册孤岛模式通知回调
fallback-island-enter-notified = 已通知所有回调：进入孤岛模式
fallback-island-exit-notified = 已通知所有回调：退出孤岛模式
fallback-first-failure-island = 存储层首次故障，触发孤岛模式
fallback-all-recovered-island-exit = 所有存储层恢复，退出孤岛模式

# ---- 熔断器（T025） ----
circuit-created = 创建熔断器: failure_threshold={ $failure_threshold }, success_threshold={ $success_threshold }, timeout={ $timeout }
circuit-open-rejecting = 熔断器打开，拒绝请求
circuit-open-request-rejected = 熔断器打开，请求被拒绝
circuit-half-open-limit-reached = 半开状态调用次数已达上限，拒绝请求
circuit-half-open-limit-exceeded = 半开状态调用次数已达上限
circuit-success-while-open = 熔断器打开状态下收到成功响应
circuit-half-open-probe-failed = 半开探针失败（期间状态已漂移至 Closed），重新熔断
circuit-failure-while-open = 熔断器打开状态下收到失败响应
circuit-slow-call-rate-exceeded = 慢调用率超过阈值: { $rate }% >= { $threshold }%，触发熔断
circuit-state-changed-open = 熔断器状态变更: { $old_state } -> Open (failure_count={ $failure_count })
circuit-state-changed-half-open = 熔断器状态变更: { $old_state } -> HalfOpen
circuit-state-changed-closed = 熔断器状态变更: { $old_state } -> Closed
circuit-reset = 重置熔断器

# ---- L1 缓存孤岛模式（T025） ----
l1-cache-island-entered = L1 缓存进入孤岛模式: strategy={ $strategy }
l1-cache-island-exited = L1 缓存退出孤岛模式

# ---- Admin API（T025） ----
admin-api-disabled = 管理API已禁用
admin-api-started = 管理API服务器已启动: http://{ $address }
admin-api-key-operator-fallback = API key 未配置 operator 映射，回退到默认 'admin-api'；建议通过 AdminApiConfig::with_api_key_operator 配置显式映射以防止 operator 身份伪造
admin-rbac-denied = RBAC 拒绝：role={ $role } 无权访问 { $method } { $path }

# ---- 审计日志（T025） ----
audit-batch-task-panic = 审计日志批处理任务异常: { $reason }
audit-entry = 审计日志: { $json }
audit-entry-missing-signature = 日志条目缺少签名
audit-entry-parse-failed = 解析审计日志条目失败: { $reason }
audit-file-write-failed = 写入审计日志文件失败: { $path }: { $reason }
audit-logger-created = 创建审计日志记录器: enabled={ $enabled }, signing_enabled={ $signing_enabled }
audit-logger-stopped = 停止审计日志记录器
audit-send-ban-event-failed = 发送封禁操作事件失败: { $reason }
audit-send-config-change-failed = 发送配置变更事件失败: { $reason }
audit-send-decision-event-failed = 发送决策事件失败: { $reason }
audit-send-error-event-failed = 发送错误事件失败: { $reason }
audit-send-system-event-failed = 发送系统事件失败: { $reason }
audit-serialize-failed = 序列化审计日志失败: { $reason }
audit-signature-verification-failed = 签名验证失败，日志可能被篡改
audit-tampered-entry-discarded = 审计日志签名验证失败，丢弃篡改条目: { $reason }
audit-write-task-ended = 审计日志写入任务结束

# ---- 授权（T025） ----
authz-operator-not-authorized = 操作者 '{ $operator }' 未被授权执行此操作
authz-operator-not-authorized-for-operation = 操作者 '{ $operator }' 未被授权执行操作 '{ $operation }'
authz-unknown-operation = 未知的操作类型: '{ $operation }'

# ---- 封禁管理器 / 文件加载（T025） ----
ban-check-error-fail-open = 封禁检查出错，按未封禁处理（fail-open）: { $error }
ban-check-storage-error-fail-open = 封禁检查存储错误，按未封禁处理（fail-open）: target={ $target }, error={ $error }
ban-check-timeout-fail-open = 封禁检查超时（{ $timeout }），按未封禁处理（fail-open）: target={ $target }
ban-file-changed-reloading = 封禁文件变更，触发重载: { $path }
ban-file-entry-load-failed = 文件加载封禁失败: target={ $target }, error={ $error }
ban-file-existing-lookup-failed-create-new = 查询既有封禁失败，按新建处理: target={ $target }, error={ $error }
ban-file-load-complete = 文件封禁加载完成: { $count } 条
ban-file-load-partial-failure = 文件封禁加载存在失败: 成功 { $success } 条, 失败 { $failure } 条
ban-file-reload-complete = 封禁文件重载完成: 成功 { $success } 条, 失败 { $failure } 条
ban-file-reload-failed = 封禁文件重载失败: { $reason }
ban-manager-no-authorization-provider = BanManager 创建时未配置 authorization_provider，所有手动封禁操作将跳过细粒度授权检查（仅依赖 admin API key 认证）
ban-manual-skip-authorization = 手动封禁跳过授权检查（未配置 authorization_provider）: operator={ $operator }, target={ $target }
ban-reason-empty = 封禁原因不能为空
ban-reason-invalid-chars = 封禁原因包含非法字符
ban-reason-too-long = 封禁原因过长，最大长度为 { $max } 字符

# ---- 缓存存储（T025） ----
cache-ban-index-cas-retries-exhausted = ban index CAS 重试耗尽（并发冲突过高）
cache-ban-record-cas-retries-exhausted = ban record CAS 重试耗尽（并发冲突过高）
cache-ban-times-overflow-u32 = ban_times 超出 u32 范围: { $reason }
cache-quota-counter-parse-failed = quota counter 解析失败

# ---- 并发限制器（T025） ----
concurrency-permit-acquire-timeout = 获取许可超时
concurrency-permits-overflow-u32 = 许可数量超出 u32 范围
concurrency-semaphore-closed = 信号量已关闭

# ---- 自定义匹配器注册表集成（T025） ----
custom-matcher-eval-failed-no-match = 自定义匹配器 '{ $name }' 求值失败，按不匹配处理: { $error }
custom-matcher-not-registered = 自定义匹配器 '{ $name }' 未在注册表中注册，该规则将恒不匹配（其限制不会生效）
custom-matcher-pending-contract-violation = 自定义匹配器返回 Pending（违反「首次 poll 即 Ready」契约），按不匹配处理
custom-matcher-resolved-from-registry = 自定义匹配器 '{ $name }' 已从注册表解析并生效

# ---- 限流器工厂校验（T025） ----
limiter-capacity-must-be-positive = 令牌桶容量必须大于0
limiter-capacity-too-large = 令牌桶容量过大，最大值为{ $max }
limiter-custom-requires-registry = Custom 限流器类型需要由CustomLimiterRegistry处理
limiter-max-concurrent-must-be-positive = 并发限制数必须大于0
limiter-max-concurrent-too-large = 并发限制数过大，最大值为{ $max }
limiter-max-requests-must-be-positive = { $limiter_type }最大请求数必须大于0
limiter-max-requests-too-large = { $limiter_type }最大请求数过大，最大值为{ $max }
limiter-quota-requires-controller = Quota 限流器类型需要由QuotaController处理
limiter-refill-rate-must-be-positive = 令牌桶补充速率必须大于0
limiter-refill-rate-too-large = 令牌桶补充速率过大，最大值为{ $max }

# ---- 配额告警（T025） ----
quota-alert-concurrency-limit = 告警并发上限已达 { $max }，跳过本次告警: user_id={ $user_id }, resource={ $resource }, threshold={ $threshold }%
quota-alert-triggered = 配额告警触发: user_id={ $user_id }, resource={ $resource }, quota_type={ $quota_type }, threshold={ $threshold }%, current_usage={ $current_usage }, limit={ $limit }, triggered_at={ $triggered_at }
quota-alert-webhook-disabled = Webhook 功能未启用，请启用 'webhook' feature
quota-alert-webhook-error-status = Webhook 返回错误状态码: { $status }
quota-alert-webhook-send-failed = 发送 Webhook 告警失败: { $reason }
quota-init-conflict-retries-exhausted = 并发配额初始化冲突重试耗尽，请重试请求

# ---- 遥测（T025） ----
telemetry-noop-metrics-gather = Metrics 处于空对象状态：`monitoring` feature 未启用，本次及后续 gather() 返回空串，指标数据被丢弃。请在 Cargo.toml 启用 `monitoring` feature 以收集真实指标。

# ---- 校验（T025） ----
validation-country-code-empty = 国家代码不能为空
validation-country-code-length = 国家代码必须是 2 字母（ISO 3166-1 alpha-2），got { $length } 字符: { $code }
validation-country-code-uppercase = 国家代码必须是大写字母: { $code }

# ---- 遥测告警（T025） ----
alert-sent-critical = 发送严重告警: { $level }
alert-sent-info = 发送信息告警: { $level }
alert-sent-warning = 发送警告告警: { $level }
