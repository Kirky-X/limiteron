# Changelog

所有显著变更记录于此。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## Unreleased

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
