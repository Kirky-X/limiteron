//! 自定义匹配器和限流器扩展示例
//!
//! 本示例展示了如何：
//! 1. 实现自定义匹配器（CustomMatcher）
//! 2. 实现自定义限流器（CustomLimiter）
//! 3. 使用注册表管理自定义组件
//! 4. 从配置文件加载自定义匹配器和限流器
//!
//! 运行示例：
//! ```bash
//! cargo run --example custom_extensions
//! ```

use async_trait::async_trait;
use chrono::{Datelike, Timelike};
use limiteron::{
    CustomLimiter, CustomLimiterRegistry, CustomMatcher, CustomMatcherRegistry, FlowGuardError,
    LimiterStats, RequestContext,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

// ============================================================================
// 自定义匹配器示例
// ============================================================================

/// 周末匹配器
///
/// 只在周末（周六、周日）匹配请求。
#[derive(Debug, Clone)]
struct WeekendMatcher;

#[async_trait]
impl CustomMatcher for WeekendMatcher {
    fn name(&self) -> &str {
        "weekend"
    }

    async fn matches(&self, _context: &RequestContext) -> Result<bool, FlowGuardError> {
        let now = chrono::Utc::now();
        let weekday = now.weekday();

        // 周六(6)或周日(7)
        let is_weekend = weekday == chrono::Weekday::Sat || weekday == chrono::Weekday::Sun;

        info!("周末匹配器: 当前是 {:?}, 结果: {}", weekday, is_weekend);
        Ok(is_weekend)
    }

    fn load_config(&mut self, _config: serde_json::Value) -> Result<(), FlowGuardError> {
        info!("加载周末匹配器配置（无需配置）");
        Ok(())
    }
}

/// 用户等级匹配器
///
/// 根据用户等级匹配请求。
#[derive(Debug, Clone)]
struct UserLevelMatcher {
    /// 允许的用户等级
    allowed_levels: Vec<String>,
}

impl UserLevelMatcher {
    fn new(allowed_levels: Vec<String>) -> Self {
        Self { allowed_levels }
    }
}

#[async_trait]
impl CustomMatcher for UserLevelMatcher {
    fn name(&self) -> &str {
        "user_level"
    }

    async fn matches(&self, context: &RequestContext) -> Result<bool, FlowGuardError> {
        let user_level = match context.get_header("X-User-Level") {
            Some(level) => level,
            None => {
                warn!("缺少用户等级头");
                return Ok(false);
            }
        };

        let matches = self.allowed_levels.contains(&user_level.to_string());
        info!("用户等级匹配器: 用户等级={}, 结果: {}", user_level, matches);
        Ok(matches)
    }

    fn load_config(&mut self, config: serde_json::Value) -> Result<(), FlowGuardError> {
        if let Some(levels) = config["allowed_levels"].as_array() {
            self.allowed_levels = levels
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect();
        }
        info!("加载用户等级匹配器配置: 允许等级={:?}", self.allowed_levels);
        Ok(())
    }
}

// ============================================================================
// 自定义限流器示例
// ============================================================================

/// 加权令牌桶限流器
///
/// 不同请求消耗不同数量的令牌。
#[derive(Debug)]
struct WeightedTokenBucketLimiter {
    capacity: u64,
    refill_rate: u64,
    tokens: Arc<AtomicU64>,
    last_refill: Arc<AtomicU64>,
    stats: Arc<std::sync::Mutex<LimiterStats>>,
}

impl WeightedTokenBucketLimiter {
    fn new(capacity: u64, refill_rate: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        Self {
            capacity,
            refill_rate,
            tokens: Arc::new(AtomicU64::new(capacity)),
            last_refill: Arc::new(AtomicU64::new(now)),
            stats: Arc::new(std::sync::Mutex::new(LimiterStats::new())),
        }
    }

    fn refill_tokens(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        loop {
            let last = self.last_refill.load(Ordering::SeqCst);
            let elapsed_nanos = now.saturating_sub(last);

            if elapsed_nanos < 1_000_000 {
                break;
            }

            let elapsed_seconds = elapsed_nanos as f64 / 1_000_000_000.0;
            let tokens_to_add = (elapsed_seconds * self.refill_rate as f64) as u64;

            if tokens_to_add == 0 {
                break;
            }

            if self
                .last_refill
                .compare_exchange(last, now, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                loop {
                    let current = self.tokens.load(Ordering::SeqCst);
                    let new_tokens = current.saturating_add(tokens_to_add).min(self.capacity);

                    if self
                        .tokens
                        .compare_exchange(current, new_tokens, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        break;
                    }
                }
                break;
            }
        }
    }
}

#[async_trait]
impl CustomLimiter for WeightedTokenBucketLimiter {
    fn name(&self) -> &str {
        "weighted_token_bucket"
    }

    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        self.refill_tokens();

        loop {
            let current = self.tokens.load(Ordering::SeqCst);

            if current < cost {
                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.total_requests += 1;
                    stats.rejected_requests += 1;
                }
                warn!("加权令牌桶限流拒绝: 当前={}, 成本={}", current, cost);
                return Ok(false);
            }

            if self
                .tokens
                .compare_exchange(current, current - cost, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.total_requests += 1;
                    stats.allowed_requests += 1;
                }
                info!("加权令牌桶限流允许: 当前={}, 成本={}", current - cost, cost);
                return Ok(true);
            }
        }
    }

    fn load_config(&mut self, config: serde_json::Value) -> Result<(), FlowGuardError> {
        let capacity = config["capacity"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 capacity 配置".to_string()))?;

        let refill_rate = config["refill_rate"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 refill_rate 配置".to_string()))?;

        self.capacity = capacity;
        self.refill_rate = refill_rate;
        self.tokens.store(capacity, Ordering::SeqCst);

        info!(
            "加载加权令牌桶限流器配置: 容量={}, 补充速率={}",
            self.capacity, self.refill_rate
        );
        Ok(())
    }

    fn stats(&self) -> LimiterStats {
        self.stats.lock().unwrap().clone()
    }
}

/// 阶梯限流器
///
/// 根据时间段使用不同的限流策略。
#[derive(Debug)]
struct TieredLimiter {
    /// 白天限流器（9:00-18:00）
    day_limiter: Arc<limiteron::TokenBucketLimiter>,
    /// 夜间限流器（18:00-9:00）
    night_limiter: Arc<limiteron::TokenBucketLimiter>,
    stats: Arc<std::sync::Mutex<LimiterStats>>,
}

impl TieredLimiter {
    fn new(day_capacity: u64, night_capacity: u64) -> Self {
        Self {
            day_limiter: Arc::new(limiteron::TokenBucketLimiter::new(day_capacity, 10)),
            night_limiter: Arc::new(limiteron::TokenBucketLimiter::new(night_capacity, 10)),
            stats: Arc::new(std::sync::Mutex::new(LimiterStats::new())),
        }
    }

    fn is_daytime(&self) -> bool {
        let now = chrono::Utc::now();
        let hour = now.hour();
        hour >= 9 && hour < 18
    }
}

#[async_trait]
impl CustomLimiter for TieredLimiter {
    fn name(&self) -> &str {
        "tiered"
    }

    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        let limiter = if self.is_daytime() {
            info!("使用白天限流策略");
            &self.day_limiter
        } else {
            info!("使用夜间限流策略");
            &self.night_limiter
        };

        let allowed = limiter.allow(cost).await?;

        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_requests += 1;
            if allowed {
                stats.allowed_requests += 1;
            } else {
                stats.rejected_requests += 1;
            }
        }

        Ok(allowed)
    }

    fn load_config(&mut self, config: serde_json::Value) -> Result<(), FlowGuardError> {
        let day_capacity = config["day_capacity"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 day_capacity 配置".to_string()))?;

        let night_capacity = config["night_capacity"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 night_capacity 配置".to_string()))?;

        self.day_limiter = Arc::new(limiteron::TokenBucketLimiter::new(day_capacity, 10));
        self.night_limiter = Arc::new(limiteron::TokenBucketLimiter::new(night_capacity, 10));

        info!(
            "加载阶梯限流器配置: 白天容量={}, 夜间容量={}",
            day_capacity, night_capacity
        );
        Ok(())
    }

    fn stats(&self) -> LimiterStats {
        self.stats.lock().unwrap().clone()
    }
}

// ============================================================================
// 主函数
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    println!("=== 自定义匹配器和限流器扩展示例 ===\n");

    // ================================================================
    // 示例1: 使用内置的自定义匹配器
    // ================================================================
    info!("--- 示例1: 使用内置的自定义匹配器 ---");

    let matcher_registry = CustomMatcherRegistry::new();

    // 注册时间窗口匹配器
    let time_window_matcher = limiteron::TimeWindowMatcher::new(0, 23); // 全天匹配
    matcher_registry
        .register("time_window".to_string(), Box::new(time_window_matcher))
        .await?;

    // 注册HTTP头匹配器
    let header_matcher = limiteron::HeaderMatcher::new(
        "X-API-Key",
        vec!["secret123".to_string(), "secret456".to_string()],
    );
    matcher_registry
        .register("header".to_string(), Box::new(header_matcher))
        .await?;

    info!("注册的匹配器: {:?}", matcher_registry.list().await);

    // 测试时间窗口匹配器
    let context = RequestContext::new();
    let matches = matcher_registry.match_with("time_window", &context).await?;
    info!("时间窗口匹配结果: {}", matches);

    // 测试HTTP头匹配器
    let context = RequestContext::new().with_header("X-API-Key", "secret123");
    let matches = matcher_registry.match_with("header", &context).await?;
    info!("HTTP头匹配结果: {}", matches);

    // ================================================================
    // 示例2: 使用自定义匹配器
    // ================================================================
    info!("\n--- 示例2: 使用自定义匹配器 ---");

    let custom_matcher_registry = CustomMatcherRegistry::new();

    // 注册周末匹配器
    let weekend_matcher = WeekendMatcher;
    custom_matcher_registry
        .register("weekend".to_string(), Box::new(weekend_matcher))
        .await?;

    // 注册用户等级匹配器
    let user_level_matcher =
        UserLevelMatcher::new(vec!["gold".to_string(), "platinum".to_string()]);
    custom_matcher_registry
        .register("user_level".to_string(), Box::new(user_level_matcher))
        .await?;

    info!(
        "注册的自定义匹配器: {:?}",
        custom_matcher_registry.list().await
    );

    // 测试周末匹配器
    let context = RequestContext::new();
    let matches = custom_matcher_registry
        .match_with("weekend", &context)
        .await?;
    info!("周末匹配结果: {}", matches);

    // 测试用户等级匹配器
    let context = RequestContext::new().with_header("X-User-Level", "gold");
    let matches = custom_matcher_registry
        .match_with("user_level", &context)
        .await?;
    info!("用户等级匹配结果: {}", matches);

    // ================================================================
    // 示例3: 使用内置的自定义限流器
    // ================================================================
    info!("\n--- 示例3: 使用内置的自定义限流器 ---");

    let limiter_registry = CustomLimiterRegistry::new();

    // 注册漏桶限流器
    let leaky_bucket = limiteron::LeakyBucketLimiter::new(100, 10);
    limiter_registry
        .register("leaky_bucket".to_string(), Box::new(leaky_bucket))
        .await?;

    // 注册令牌桶限流器
    let token_bucket = limiteron::TokenBucketLimiter::new(100, 10);
    limiter_registry
        .register("token_bucket".to_string(), Box::new(token_bucket))
        .await?;

    info!("注册的限流器: {:?}", limiter_registry.list().await);

    // 测试漏桶限流器
    for i in 1..=10 {
        let allowed = limiter_registry.allow("leaky_bucket", 1).await?;
        info!("漏桶限流请求 #{}: {}", i, allowed);
    }

    // 获取统计信息
    let stats = limiter_registry.get_stats("leaky_bucket").await?;
    info!("漏桶限流统计: {:?}", stats);

    // ================================================================
    // 示例4: 使用自定义限流器
    // ================================================================
    info!("\n--- 示例4: 使用自定义限流器 ---");

    let custom_limiter_registry = CustomLimiterRegistry::new();

    // 注册加权令牌桶限流器
    let weighted_limiter = WeightedTokenBucketLimiter::new(100, 10);
    custom_limiter_registry
        .register(
            "weighted_token_bucket".to_string(),
            Box::new(weighted_limiter),
        )
        .await?;

    // 注册阶梯限流器
    let tiered_limiter = TieredLimiter::new(100, 50);
    custom_limiter_registry
        .register("tiered".to_string(), Box::new(tiered_limiter))
        .await?;

    info!(
        "注册的自定义限流器: {:?}",
        custom_limiter_registry.list().await
    );

    // 测试加权令牌桶限流器
    for i in 1..=5 {
        let cost = match i {
            1 | 2 => 1, // 低成本请求
            3 | 4 => 5, // 中等成本请求
            _ => 10,    // 高成本请求
        };
        let allowed = custom_limiter_registry
            .allow("weighted_token_bucket", cost)
            .await?;
        info!("加权令牌桶限流请求 #{} (成本={}): {}", i, cost, allowed);
    }

    // 获取统计信息
    let stats = custom_limiter_registry
        .get_stats("weighted_token_bucket")
        .await?;
    info!("加权令牌桶限流统计: {:?}", stats);

    // ================================================================
    // 示例5: 从配置文件加载自定义匹配器和限流器
    // ================================================================
    info!("\n--- 示例5: 从配置文件加载 ---");

    let config_yaml = r#"
version: "1.0"
global:
  storage: "memory"
  cache: "memory"
  metrics: "prometheus"
rules:
  - id: "custom_rule"
    name: "Custom Rule"
    priority: 100
    matchers:
      - type: Custom
        name: "time_window"
        config:
          start_hour: 9
          end_hour: 18
    limiters:
      - type: Custom
        name: "leaky_bucket"
        config:
          capacity: 100
          leak_rate: 10
    action:
      on_exceed: "reject"
"#;

    let config: limiteron::FlowControlConfig = serde_yaml::from_str(config_yaml)?;
    info!("配置版本: {}", config.version);
    info!("规则数量: {}", config.rules.len());

    for rule in &config.rules {
        info!("规则: {}", rule.name);
        for matcher in &rule.matchers {
            if let limiteron::ConfigMatcher::Custom { name, config } = matcher {
                info!("  自定义匹配器: {}, 配置: {}", name, config);
            }
        }
        for limiter in &rule.limiters {
            if let limiteron::LimiterConfig::Custom { name, config } = limiter {
                info!("  自定义限流器: {}, 配置: {}", name, config);
            }
        }
    }

    // ================================================================
    // 示例6: 动态更新配置
    // ================================================================
    info!("\n--- 示例6: 动态更新配置 ---");

    let mut time_window_matcher = limiteron::TimeWindowMatcher::new(9, 18);
    info!(
        "初始时间窗口: {}-{}小时",
        time_window_matcher.start_hour(),
        time_window_matcher.end_hour()
    );

    // 更新配置
    let new_config = serde_json::json!({
        "start_hour": 10,
        "end_hour": 20
    });
    time_window_matcher.load_config(new_config)?;
    info!(
        "更新后时间窗口: {}-{}小时",
        time_window_matcher.start_hour(),
        time_window_matcher.end_hour()
    );

    let mut leaky_bucket = limiteron::LeakyBucketLimiter::new(100, 10);
    info!(
        "初始漏桶: 容量={}, 流出速率={}",
        leaky_bucket.capacity(),
        leaky_bucket.leak_rate()
    );

    // 更新配置
    let new_config = serde_json::json!({
        "capacity": 200,
        "leak_rate": 20
    });
    leaky_bucket.load_config(new_config)?;
    info!(
        "更新后漏桶: 容量={}, 流出速率={}",
        leaky_bucket.capacity(),
        leaky_bucket.leak_rate()
    );

    // ================================================================
    // 示例7: 并发测试
    // ================================================================
    info!("\n--- 示例7: 并发测试 ---");

    let concurrent_registry = Arc::new(CustomLimiterRegistry::new());
    let leaky_bucket = limiteron::LeakyBucketLimiter::new(100, 10);
    concurrent_registry
        .register("concurrent".to_string(), Box::new(leaky_bucket))
        .await?;

    let mut handles = vec![];
    for i in 0..10 {
        let registry_clone = Arc::clone(&concurrent_registry);
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                let allowed = registry_clone.allow("concurrent", 1).await.unwrap();
                info!("任务 {} 请求 #{}: {}", i, j, allowed);
            }
        }));
    }

    for handle in handles {
        handle.await?;
    }

    let stats = concurrent_registry.get_stats("concurrent").await?;
    info!("并发测试统计: {:?}", stats);

    info!("\n=== 示例完成 ===");

    Ok(())
}
