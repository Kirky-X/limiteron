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
