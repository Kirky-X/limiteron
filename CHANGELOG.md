# Changelog

所有显著变更记录于此。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## 0.3.1 - 2026-11-14

### Changed

- **sdforge 依赖改为 optional（registry 环依赖解耦）**：`sdforge = "0.5.0-rc.11"` 从常驻依赖改为
  `i18n-host` feature 门控的 opt-in 依赖（`cli` feature 同步显式激活 `dep:sdforge`）。默认构建零
  sdforge 解析面——sdforge 0.5.0 的 publish 解析不再撞本仓 rc.11 钉（2026-11-13/14 实证的双向
  死锁：预发布 req 不匹配稳定版候选，详见 sdforge 仓 release run 43548087259 / 43549926749 /
  43551369643）。**i18n 消息查找在 `i18n-host` 关闭时（含默认构建）走本仓自持模板面**：同键值、
  同单遍 `{ $name }` 替换规则（`sdforge::i18n` `format_template` 权威实现逐字移植：首个同名参数
  命中、缺参占位符原样保留、非占位符大括号与无闭合大括号按原文输出），对全部内嵌键与宿主路径
  逐字等价（双路径 3658/3659 测试全绿）。宿主注册（汇入 `sdforge::i18n` 全局注册表）成为
  opt-in 增强面，`i18n-host` 开启时行为与 0.3.0 完全一致。**下游影响**：显式声明自家 sdforge
  依赖的消费者零影响；依赖 limiteron 传递获得 sdforge 的消费者需自行声明（家族内无此用法）。
- `cli` feature 单开时消息查找走自持面，不再注册进 sdforge 全局注册表（宿主注册随 `i18n-host`）。

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
