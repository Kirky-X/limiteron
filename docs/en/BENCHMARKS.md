# Benchmark and Performance Baseline Methodology

## Running

```bash
cargo bench --features full              # all four suites (throughput/latency/memory/regression)
cargo bench --features full --bench latency -- "custom_matcher_latency"   # single suite / single benchmark filter
```

- `[[bench]] harness = false`: criterion 0.8 requires disabling the libtest harness; without it `cargo bench`
  falls into an empty libtest run ("running 0 tests"), and baselines never really materialized before the 2026-09-30 fix.
- Fast methodology (for local iteration): `--sample-size 20 --warm-up-time 0.3 --measurement-time 1`;
  gate/release methodology use criterion defaults (100 samples / 5s). Absolute values from the two are not directly comparable.

## Baselines and Refresh

- Baseline details (environment, methodology, reference numbers, reduction-loop records) are currently tracked in the local `reviews/perf-baseline.md`.
  Note that `reviews/` is excluded by `.gitignore` (not committed), and **no numeric baselines are published in the repo**: what is externally reproducible is this file's
  commands and methodology; concrete numbers must be measured on your own machine (criterion reports land in `target/criterion/`); the [historical comparison](#historical-comparison)
  at the end of this file is v0.1.0-era data and is not a current baseline.
- Refresh timing: after algorithm implementation changes, hot-path code changes, or major dependency upgrades; re-measure on the same machine with the same methodology,
  append results to the same table and note the commit.
- Built-in criterion comparison: on non-first runs it automatically prints `change: [+x% ...]` versus the previous run (p-value significance);
  specify a baseline explicitly with `cargo bench -- --baseline <id>` (ids are in the target/criterion reports).
- Cross-machine / cross-time comparisons must use the same machine and methodology; absolute values depend on CPU frequency/load/virtualization — judge trends
  by same-machine relative changes.

## Coverage Matrix

- throughput: six-limiter throughput comparison, scaling curves, cache and mixed workloads.
- latency: limiter latency distributions, custom matchers (HeaderMatcher Exact/Prefix hit/miss ×
  1/10/100 values, MethodMatcher, RegexPathMatcher anchored/non-anchored × short/long paths), full decision-chain flow.
- memory: memory footprint by instance count and by entry count, leak detection.

## Historical Comparison

> **Note**: the tables below are measured data from 2026-01-19 (the v0.1.0 era), with a **methodology predating the criterion benchmark integration** —
> the missing `harness = false` meant no baseline actually materialized until 2026-09-30. The numbers below are kept as a historical comparison; for the current reproducible baseline,
> use the [Running](#running) commands in this file together with same-machine re-measurements.

<table>
<tr>
<td width="50%" valign="top">

**Throughput**

| Limiter | Measured | Target | Achieved |
|--------|------|------|------|
| TokenBucket | **12M+ ops/s** | 500K ops/s | ✅ 24x |
| FixedWindow | **20M+ ops/s** | 300K ops/s | ✅ 66x |
| ConcurrencyLimiter | **12M+ ops/s** | 200K ops/s | ✅ 60x |

</td>
<td width="50%" valign="top">

**Latency**

| Percentile | TokenBucket | FixedWindow |
|------|-------------|-------------|
| P50 | < 100ns | < 100ns |
| P95 | < 200ns | < 150ns |
| P99 | < 1µs | < 500ns |

</td>
</tr>
</table>

<details>
<summary><b>📈 Detailed benchmark data</b></summary>

<br>

Historical measured data (methodology predates the unified benchmark-infrastructure methodology, order-of-magnitude reference only; the current methodology is the fresh-instance allow path, see the methodology notes in `benches/throughput.rs`):

```text
TokenBucket: 12,088,759 ops/s
FixedWindow: 19,920,188 ops/s
ConcurrencyLimiter: 11,891,237 ops/s
Concurrency tests: data consistency 100%, rate-limit correctness 1000/1000
```

</details>

**Benchmark infrastructure**: the repo ships four criterion benchmark suites (all require the `full` feature) for reproduction and regression detection:

| Benchmark | File | Content |
|------|------|------|
| Throughput | `benches/throughput.rs` | single-thread/concurrent throughput and scaling curves |
| Latency | `benches/latency.rs` | P50/P90/P99/P99.9 latency measurement and operation comparison |
| Memory | `benches/memory.rs` | memory footprint by key scale, data-structure comparison, leak detection |
| Regression | `benches/regression.rs` | historical baseline storage and automatic comparison alerts |
