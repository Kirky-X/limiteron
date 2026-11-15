# 基准测试与性能基线规程

## 运行

```bash
cargo bench --features full              # 全部四组（throughput/latency/memory/regression）
cargo bench --features full --bench latency -- "custom_matcher_latency"   # 单组/单基准过滤
```

- `[[bench]] harness = false`：criterion 0.8 要求关闭 libtest harness；缺失时 `cargo bench`
  落入 libtest 空跑（"running 0 tests"），2026-09-30 修复前基线从未真实产出过。
- 快速口径（本地迭代用）：`--sample-size 20 --warm-up-time 0.3 --measurement-time 1`；
  门禁/发布口径用 criterion 默认（100 样本/5s）。两者绝对值不可直接对比。

## 基线与刷新

- 基线明细（环境、口径、参考数值、削减闭环记录）现行记录在本地 `reviews/perf-baseline.md`。
  注意 `reviews/` 被 `.gitignore` 排除（不入库），**仓库内不发布数值基线**：对外可复现的是本文件的
  命令与口径，具体数字需同机自行跑出（criterion 报告落在 `target/criterion/`）；README 性能章的表格
  属 v0.1.0 时期历史对照，不作当前基线。
- 刷新时机：算法实现变更、热路径代码变更、或依赖大版本升级后；同机同口径复测，
  结果追加同表并注明提交。
- criterion 内建对比：非首跑时自动输出与前次的 `change: [+x% ...]`（p 值显著性）；
  显式指定基线用 `cargo bench -- --baseline <id>`（id 见 target/criterion 报告）。
- 跨机器/跨时间对比必须同机同口径；绝对值受 CPU 频率/负载/虚拟化影响，趋势判断
  以同机相对变化为准。

## 覆盖矩阵

- throughput：六限流器吞吐对比、扩展曲线、缓存与混合负载。
- latency：限流器延迟分布、自定义匹配器（HeaderMatcher Exact/Prefix hit/miss ×
  1/10/100 值、MethodMatcher、RegexPathMatcher 锚定/非锚定×短/长路径）、决策链全流程。
- memory：按实例数与按条目数的内存占用、泄漏检测。
