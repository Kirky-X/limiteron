# Changelog

所有显著变更记录于此。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## Unreleased

## 0.3.0 - 2026-11-14

0.3 线正式版：多租户贯穿 Governor、CIDR 网段封禁、Admin RBAC、OTLP 追踪导出、MySQL 存储、HTB 分层令牌桶、bulkhead 隔离、AIMD 自适应限流、limiteron-cli 等随 rc.2-rc.10 波次交付的能力随本版定稿。

### Changed

- 全家族依赖随上游转正对齐 minor 精度：`trait-kit` 0.5、`confers` 0.6、`oxcache` 0.5、`dbnexus` 0.6（trait-kit 0.5.0 i18n PluralCategory 重导出等接口归一随版带入）；`sdforge` 维持 0.5.0-rc.11（声明环断环约定保持现钉版）。公共 API 无破坏，编译与测试门禁全绿。
- 消息目录改 sdforge::i18n 宿主注册、ICU 格式化委托 trait-kit——自持 fluent-bundle/icu/writeable 依赖删除。
- 二进制入口收敛 `src/main.rs`：独立 src/bin 入口删除，CLI 子命令化经 sdforge CliBuilder（JSON 输出形状、退出码契约与 `help`/`--version` 行为不变）。

## 0.3.0-rc.10 - 2026-11-01

### Fixed

- `cleartext-logging` 误报面收敛：脱敏测试断言消息不再回显渲染输出，
  admin 配置日志改走 i18n 消息键（不再内插原始配置值）。

## 0.3.0-rc.9 - 2026-10-31

### Changed

- `log-redaction` 掩码引擎换源：`inklog` 依赖删除，改依赖 `trait-kit/masking`
  （`DataMasker` / `MaskRule` / `MaskRuleRegistry` 单一来源下沉 trait-kit）；
  limiteron 对 inklog 的依赖归零，双向可选边成环风险清零。`trait-kit` 地板
  抬至 `0.5.0-rc.11`。守卫测试口径修正：跨仓自研依赖不变式约束「已声明者
  须经 registry 引入」，未声明项跳过（约束来源而非声明义务）。

### Added

- 新增 `integrations/limiteron-inklog` 适配器 crate（不发布 crates.io，照
  `limiteron-dbnexus` 先例）：实现 inklog 自有 `SinkRateLimit` 端口的按 target
  分桶令牌预算——`ERROR` 级直通、sink 写失败归还预算、桶数上限防无界增长；
  补充语义与 `TokenBucketLimiter::new(rate, rate)` 等价（纳秒时间积分携带）。
