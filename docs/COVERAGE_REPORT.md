# 📊 Limiteron 覆盖率报告

> ## 📊 数据口径说明
>
> **本报告为历史快照，数据已过时，请勿作为当前质量依据。**
>
> - 本报告基于 **v0.1.0 时期**（生成于 2026-03-20）的 cargo-tarpaulin 统计，覆盖率口径为 **53.56%**。此后 v0.2.x 重组了模块结构、测试规模扩大数倍，这些数字已无法反映现状。
> - **当前覆盖率门禁为 cargo-llvm-cov 行覆盖率 ≥ 80%**，口径与命令以 [CI 配置](../.github/workflows/ci.yml) 为准：
>   `cargo llvm-cov --workspace --no-default-features --features full --lib --fail-under-lines 80`
> - 当前测试规模基线见 [测试场景固化](TEST_SCENARIOS.md) 与 [测试指南](TESTING.md)。
>
> 以下内容仅作历史追溯参考，本文档不提供也不应引用任何"当前覆盖率"数字。

生成日期: 2026-03-20

对应版本: v0.1.0（历史基线）

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [总体覆盖率](#总体覆盖率)
- [模块覆盖率详情](#模块覆盖率详情)
- [测试统计](#测试统计)
- [改进建议](#改进建议)
- [覆盖率趋势](#覆盖率趋势)
- [报告生成](#报告生成)

</details>

---

## 总体覆盖率

| 指标 | 数值 |
|------|------|
| **总体覆盖率** | 53.56% |
| **覆盖行数** | 1664 / 3107 |
| **未覆盖行数** | 1443 |

## 模块覆盖率详情

### 高覆盖率模块 (>80%)

| 模块 | 覆盖率 | 覆盖/总数 |
|------|--------|----------|
| validation.rs | 100% | 62/62 |
| stats_manager.rs | 100% | 67/67 |
| dbnexus_entities/ban_record.rs | 100% | 2/2 |
| dbnexus_entities/quota_record.rs | 100% | 2/2 |
| dbnexus_entities/rate_limit.rs | 100% | 2/2 |
| limiters.rs | 87% | 211/242 |
| l1_cache.rs | 85% | 138/163 |

### 中等覆盖率模块 (50-80%)

| 模块 | 覆盖率 | 覆盖/总数 |
|------|--------|----------|
| authorization.rs | 89% | 24/27 |
| matchers/custom.rs | 57% | 132/231 |
| matchers/mod.rs | 52% | 270/523 |
| governor.rs | 68% | 137/202 |
| factory/mod.rs | 60% | 36/60 |
| decision_chain.rs | 79% | 123/155 |
| config.rs | 79% | 235/298 |
| rule_builder.rs | 64% | 49/77 |
| storage_trait.rs | 53% | 18/34 |
| error.rs | 54% | 7/13 |

### 低覆盖率模块 (<50%)

| 模块 | 覆盖率 | 覆盖/总数 |
|------|--------|----------|
| adapters/dbnexus_ban_storage.rs | 0% | 0/160 |
| adapters/dbnexus_quota_storage.rs | 0% | 0/96 |
| adapters/dbnexus_storage.rs | 0% | 0/55 |
| adapters/storage_factory.rs | 38% | 26/68 |
| circuit_breaker.rs | 0% | 0/32 |
| config_loader.rs | 7% | 9/127 |
| dbnexus_entities/mod.rs | 0% | 0/7 |
| error_abstraction.rs | 41% | 62/151 |
| fallback.rs | 0% | 0/35 |
| limiter_manager.rs | 33% | 11/33 |
| log_redaction.rs | 32% | 35/108 |
| matchers/geo.rs | 0% | 0/54 |
| oxcache_lua.rs | 0% | 0/2 |
| telemetry.rs | 0% | 0/7 |

## 测试统计

| 测试类型 | 数量 | 状态 |
|----------|------|------|
| 单元测试 | 303 | ✅ 通过 |
| 集成测试 | 161 | ✅ 通过 |
| 文档测试 | 106 | ✅ 通过 |
| **总计** | **570** | **✅ 全部通过** |

## 改进建议

> 📌 以下建议为 v0.1.0 时期记录，仅作历史参考；其中的多数缺口（熔断器、降级、配置加载等）已在后续版本补齐测试。当前缺口请以最新 llvm-cov 报告为准。

### 需要补充测试的模块

1. **circuit_breaker.rs** (0%)
   - 添加熔断器状态转换测试
   - 添加半开状态测试

2. **fallback.rs** (0%)
   - 添加降级策略测试
   - 添加组件故障模拟测试

3. **config_loader.rs** (7%)
   - 添加配置文件加载测试
   - 添加环境变量覆盖测试

4. **adapters/*.rs** (0-38%)
   - 添加 dbnexus 存储适配器测试（需要数据库）
   - 添加工厂创建测试

### 特性相关模块

以下模块需要启用特定特性才能测试：

- `audit-log`: audit_log.rs
- `ban-manager`: ban_manager.rs, authorization.rs
- `cache-service`: cache_service.rs
- `circuit-breaker`: circuit_breaker.rs
- `geo-matching`: matchers/geo.rs
- `quota-control`: quota_controller.rs

## 覆盖率趋势

| 日期 | 覆盖率 | 变化 |
|------|--------|------|
| 2026-03-19 (基线) | 17.8% | - |
| 2026-03-20 (当时) | 53.56% | +35.76% |

## 报告生成

历史报告由 cargo-tarpaulin 生成：

```bash
# 生成 HTML 报告（注意：postgres/sqlite/mysql 互斥，不可 --all-features）
cargo tarpaulin --out Html --features minimal

# 生成 JSON 报告
cargo tarpaulin --out Json --features minimal
```

当前口径请使用 cargo-llvm-cov（与 CI/pre-push 门禁一致，完整命令见本报告开头的数据口径说明与 [测试指南](TESTING.md#-测试覆盖率)；生成 lcov 文件时追加 `--lcov --output-path lcov.info`）。
