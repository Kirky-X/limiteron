# Limiteron 架构文档

### 完整 API 文档

[🏠 首页](../README.md) • [📚 变更日志](CHANGELOG.md) • [📘 API 参考](API_REFERENCE.md) • [🧪 测试指南](TESTING.md)

---

## 整体架构

```mermaid
graph TB
    subgraph "入口层"
        GW[Gateway / Load Balancer]
        MW[Tower Middleware<br/>RateLimitLayer]
    end

    subgraph "核心控制层"
        GOV[Governor<br/>主控制器]
        DC[DecisionChain<br/>策略决策引擎]
    end

    subgraph "限流算法层"
        TB[TokenBucket]
        FW[FixedWindow]
        SW[SlidingWindow]
        GCRA[GCRA]
        CC[Concurrency]
        QL[QuotaLimiter]
    end

    subgraph "自适应流量优化"
        ACB[AdaptiveCircuitBreaker<br/>自适应熔断器]
        AW[AdaptiveWindowController<br/>自适应滑动窗口]
        PM[PredictionModel<br/>预测评分模型]
        CF[CascadingFallbackChain<br/>级联降级]
        RB[RetryBudget<br/>重试预算]
        ADM[AdmissionController<br/>SLO 准入控制]
        PQ[PriorityResolver<br/>优先级解析]
        TP[TrafficProfiler<br/>流量画像]
    end

    subgraph "辅助层"
        BAN[BanManager]
        QUOTA[QuotaController]
        CB[CircuitBreaker]
        FB[FallbackManager]
        MATCH[Matchers<br/>标识符提取]
        STORE[Storage<br/>存储后端]
        CACHE[CacheService<br/>缓存服务]
        TEL[Telemetry<br/>指标/追踪]
        EVT[EventEmitter<br/>事件系统]
    end

    GW --> MW --> GOV
    GOV --> DC
    DC --> TB & FW & SW & GCRA & CC & QL
    DC --> BAN & QUOTA & CB & FB
    GOV --> MATCH
    GOV --> STORE & CACHE
    GOV --> TEL & EVT

    CB --> ACB
    ACB --> AW & PM
    FB --> CF & RB
    GOV --> ADM & PQ & TP
    TP --> EVT
```

## 自适应流量优化模块架构

### 模块依赖关系

```mermaid
graph LR
    subgraph "自适应熔断器 (adaptive-circuit-breaker)"
        AW[adaptive_window<br/>AdaptiveWindowController]
        PM[prediction_model<br/>CommunicationRating]
        MT[metrics/<br/>内置指标]
    end

    subgraph "级联降级 (cascading-fallback)"
        CF[cascading<br/>CascadingFallbackChain]
    end

    subgraph "重试预算 (retry-budget)"
        RB[retry_budget<br/>TokenBucketRetryBudget]
        JT[jitter<br/>JitterStrategy]
    end

    subgraph "SLO 准入控制 (admission-control)"
        ADM[admission/mod<br/>AdmissionController]
        BN[bouncer<br/>BouncerAdmissionController]
        HG[histogram<br/>双缓冲直方图]
        SV[starvation<br/>StarvationAvoidance]
    end

    subgraph "优先级队列 (priority-queue)"
        PR[priority/resolver<br/>DefaultPriorityResolver]
    end

    subgraph "流量画像 (traffic-profiling)"
        PF[profiling/mod<br/>TrafficProfiler]
        DT[detector<br/>StatisticalAnomalyDetector]
        PA[pattern<br/>PeriodicPatternAnalyzer]
    end

    AW --> CB[circuit/types<br/>CircuitBreaker]
    PM --> CB
    MT --> PM
    CF --> FB[fallback/mod<br/>FallbackManager]
    RB --> FB
    JT --> FB
    BN --> ADM
    HG --> BN
    SV --> ADM
    DT --> PF
    PA --> PF
    PF --> EVT[events<br/>EventEmitter]
```

### 数据流

```mermaid
sequenceDiagram
    participant Client
    participant Governor
    participant Admission as AdmissionController
    participant Priority as PriorityResolver
    participant Circuit as CircuitBreaker
    participant AW as AdaptiveWindow
    participant PM as PredictionModel
    participant Limiter as RateLimiter
    participant Profiler as TrafficProfiler
    participant Fallback as FallbackManager
    participant Cascading as CascadingFallbackChain

    Client->>Governor: check(request)
    Governor->>Admission: evaluate(request_type)
    Admission-->>Governor: Accept/Reject

    alt Accepted
        Governor->>Priority: resolve(context)
        Priority-->>Governor: RequestPriority

        Governor->>Circuit: execute(closure)
        Circuit->>AW: current_size()
        AW-->>Circuit: dynamic_window_size

        alt Prediction Mode
            Circuit->>PM: calculate(stats)
            PM-->>Circuit: rating
            Note over Circuit: rating ≥ threshold → 恢复
        end

        Circuit->>Limiter: allow(key)
        Limiter-->>Circuit: decision

        alt Failure
            Circuit->>Fallback: handle(component)
            Fallback->>Cascading: escalate(component)
            Cascading-->>Fallback: FallbackLevel
        end

        Governor->>Profiler: record()
        Profiler-->>Governor: Option<AnomalyType>
    end

    Governor-->>Client: Decision
```

## 新增模块详细说明

### 自适应滑动窗口 (`src/circuit/adaptive_window.rs`)

**Feature**: `adaptive-circuit-breaker`（依赖 `circuit-breaker`）

**职责**：根据实时请求速率动态调整熔断器滑动窗口大小。

**核心算法**：
1. 指数平滑：`λ_smoothed = (1-γ)·λ_prev + γ·λ_raw`
2. 目标窗口：`W_target = clip(α·λ_smoothed, W_min, W_max)`
3. 滞后判断：`|W_target - W_current| / W_current > ε` 时才更新

**并发模型**：`AtomicU64`（请求计数）+ `parking_lot::RwLock`（平滑速率/窗口大小）

### 预测型熔断器 (`src/circuit/prediction_model.rs`, `src/circuit/metrics/`)

**Feature**: `adaptive-circuit-breaker`

**职责**：基于加权性能指标计算通信评分，驱动熔断器智能恢复。

**评分公式**：`rating = Σ(c_pos·m_pos) + Σ(c_neg·(1-m_neg))` ∈ [0, 1]

**内置指标**：
| 指标 | 文件 | 方向 | 计算 |
|------|------|------|------|
| FailureRateMetric | `metrics/failure_rate.rs` | Negative | failures / total |
| SlowCallRateMetric | `metrics/slow_call_rate.rs` | Negative | slow_calls / total |
| PermittedCallRateMetric | `metrics/permitted_call_rate.rs` | Positive | permitted / total |
| ConsecutiveFailureStreakMetric | `metrics/consecutive_streak.rs` | Negative | streak / threshold |

### 级联降级 (`src/fallback/cascading.rs`)

**Feature**: `cascading-fallback`（依赖 `fallback`）

**降级级别**：Normal → L1CacheOnly → StaticResponse → FailOpen → FailClosed

**并发模型**：`HashMap<ComponentType, FallbackLevel>` + `parking_lot::RwLock`

### 重试预算 (`src/fallback/retry_budget.rs`) + Jitter (`src/fallback/jitter.rs`)

**Feature**: `retry-budget`（依赖 `fallback`, `dep:rand`）

**重试预算**：令牌桶算法，`AtomicU64` 存储 + `parking_lot::Mutex` 保护补充。

**Jitter 策略**：
| 策略 | 公式 |
|------|------|
| FullJitter | `random(0, base_delay)` |
| EqualJitter | `base_delay/2 + random(0, base_delay/2)` |
| DecorrelatedJitter | `min(max_delay, random(base_delay, prev_delay·3))` |

### SLO 准入控制 (`src/admission/`)

**Feature**: `admission-control`

**核心组件**：
- `BouncerAdmissionController`：基于 EWT/ERT 公式的准入决策
- 双缓冲直方图（`histogram.rs`）：active (VecDeque) 写入 + frozen (Vec) 排序读取
- 饥饿避免（`starvation.rs`）：时间窗口内拒绝率监控 + 降级触发

### 优先级队列 (`src/priority/`)

**Feature**: `priority-queue`

**核心组件**：
- `DefaultPriorityResolver`：基于 RequestContext 的 path/user_id/tenant 规则匹配
- `PriorityConfig`：Critical/High/Normal/Low 比例配置 + 饥饿避免策略

### 流量画像 (`src/profiling/`)

**Feature**: `traffic-profiling`（依赖 `event-system`）

**核心组件**：
- `StatisticalAnomalyDetector`：Z-score (|z|>threshold) + IQR 双重检测
- `PeriodicPatternAnalyzer`：自相关函数 (ACF) 检测周期性 + 线性回归趋势检测
- `DefaultTrafficProfiler`：组合 detector + pattern analyzer

## Feature 依赖总览

```mermaid
graph BT
    ACB[adaptive-circuit-breaker] --> CB[circuit-breaker]
    RB[retry-budget] --> FB[fallback]
    CF[cascading-fallback] --> FB
    TP[traffic-profiling] --> ES[event-system]
    FULL[full preset] --> ACB & RB & CF & ADM[admission-control] & PQ[priority-queue] & TP
```

| Feature | 依赖 | 新增模块 |
|---------|------|----------|
| `adaptive-circuit-breaker` | `circuit-breaker` | `circuit/adaptive_window`, `circuit/prediction_model`, `circuit/metrics/` |
| `retry-budget` | `fallback`, `dep:rand` | `fallback/retry_budget`, `fallback/jitter` |
| `cascading-fallback` | `fallback` | `fallback/cascading` |
| `admission-control` | — | `admission/` |
| `priority-queue` | — | `priority/` |
| `traffic-profiling` | `event-system` | `profiling/` |
