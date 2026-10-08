// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 限流器工厂模块
//!
//! 提供统一的限流器创建接口，支持通过配置动态创建各种限流器。
//!
//! # 特性
//!
//! - **统一创建接口** - 通过配置动态创建限流器
//! - **类型安全** - 编译时类型检查
//! - **扩展性强** - 易于添加新的限流器类型
//! - **错误处理** - 完善的错误信息和类型

use super::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter,
    TokenBucketLimiter,
};
use crate::config::LimiterConfig;
use crate::config::parse_window_size;
use crate::constants::{
    MAX_CONCURRENT_REQUESTS, MAX_SLIDING_LOG_REQUESTS, MAX_TOKEN_BUCKET_CAPACITY,
    MAX_TOKEN_BUCKET_REFILL_RATE, MAX_WINDOW_REQUESTS,
};
use crate::error::LimiteronError;
use crate::i18n::t;
use std::sync::Arc;

/// 限流器工厂
///
/// 提供统一的限流器创建接口，支持从配置创建各种限流器。
///
/// # 示例
///
/// ```rust
/// use limiteron::limiters::factory::LimiterFactory;
/// use limiteron::config::LimiterConfig;
///
/// // 创建令牌桶限流器
/// let config = LimiterConfig::TokenBucket {
///     capacity: 1000,
///     refill_rate: 100,
/// };
/// let limiter = LimiterFactory::create(&config).unwrap();
/// ```
pub struct LimiterFactory;

impl LimiterFactory {
    /// 从配置创建限流器
    ///
    /// # 参数
    /// - `config`: 限流器配置
    ///
    /// # 返回
    /// - `Ok(Arc<dyn Limiter>)`: 创建成功的限流器
    /// - `Err(LimiteronError)`: 创建失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::limiters::factory::LimiterFactory;
    /// use limiteron::config::LimiterConfig;
    ///
    /// let config = LimiterConfig::TokenBucket {
    ///     capacity: 1000,
    ///     refill_rate: 100,
    /// };
    /// let limiter = LimiterFactory::create(&config).unwrap();
    /// ```
    pub fn create(config: &LimiterConfig) -> Result<Arc<dyn Limiter>, LimiteronError> {
        match config {
            LimiterConfig::TokenBucket {
                capacity,
                refill_rate,
            } => Ok(Arc::new(TokenBucketLimiter::new(*capacity, *refill_rate))),
            LimiterConfig::SlidingWindow {
                window_size,
                max_requests,
            } => {
                let duration = Self::parse_window_size(window_size)?;
                Ok(Arc::new(ShardedSlidingWindowLimiter::new(
                    duration,
                    *max_requests,
                )))
            }
            LimiterConfig::FixedWindow {
                window_size,
                max_requests,
            } => {
                let duration = Self::parse_window_size(window_size)?;
                Ok(Arc::new(FixedWindowLimiter::new(duration, *max_requests)))
            }
            LimiterConfig::Concurrency { max_concurrent } => {
                Ok(Arc::new(ConcurrencyLimiter::new(*max_concurrent)))
            }
            LimiterConfig::LeakyBucket {
                capacity,
                leak_rate,
            } => Ok(Arc::new(super::leaky_bucket::LeakyBucketLimiter::new(
                *capacity, *leak_rate,
            )?)),
            LimiterConfig::SlidingWindowLog {
                window_size,
                max_requests,
            } => {
                let duration = Self::parse_window_size(window_size)?;
                Ok(Arc::new(
                    super::sliding_window_log::SlidingWindowLogLimiter::new(
                        *max_requests,
                        duration,
                    )?,
                ))
            }
            #[cfg(feature = "priority-queue")]
            LimiterConfig::PriorityQueue {
                window_size,
                total_per_window,
                level_weights,
                default_priority,
            } => {
                let window = Self::parse_window_size(window_size)?;
                Ok(Arc::new(super::priority_queue::PriorityQueueLimiter::new(
                    super::priority_queue::PriorityQueueConfig {
                        window,
                        total_per_window: *total_per_window,
                        level_weights: level_weights.clone(),
                        default_priority: default_priority
                            .unwrap_or_else(|| level_weights.len().saturating_sub(1)),
                    },
                )?))
            }
            #[cfg(not(feature = "priority-queue"))]
            LimiterConfig::PriorityQueue { .. } => Err(LimiteronError::LimitError(t(
                "limiter-priority-queue-feature-disabled",
                &[],
            ))),
            #[cfg(feature = "admission-control")]
            LimiterConfig::AdmissionControl {
                max_concurrent,
                max_per_second,
            } => Ok(Arc::new(
                super::admission_control::AdmissionController::new(
                    super::admission_control::AdmissionControlConfig {
                        max_concurrent: *max_concurrent,
                        max_per_second: *max_per_second,
                    },
                ),
            )),
            #[cfg(not(feature = "admission-control"))]
            LimiterConfig::AdmissionControl { .. } => Err(LimiteronError::LimitError(t(
                "limiter-admission-control-feature-disabled",
                &[],
            ))),
            LimiterConfig::Quota {
                quota_type: _,
                limit: _limit,
                window: _window,
                alert_threshold: _,
                overdraft: _,
            } => {
                // Quota 配额由 QuotaController（管理面）或
                // RuleBuilder::build_rule_chains_with_quota_storage（决策链,
                // 可选 storage-backed）处理;本工厂不直构配额限流器
                Err(LimiteronError::LimitError(t(
                    "limiter-quota-requires-controller",
                    &[],
                )))
            }
            LimiterConfig::Custom { .. } => {
                // Custom 类型由CustomLimiterRegistry处理
                Err(LimiteronError::LimitError(t(
                    "limiter-custom-requires-registry",
                    &[],
                )))
            }
        }
    }

    /// 以 Redis 后端路由创建分布式限流器（backend=redis 的入口）
    ///
    /// - `FixedWindow` → `RedisDistributedLimiter`（固定窗口脚本）
    /// - `TokenBucket` → `RedisTokenBucketLimiter`（令牌桶脚本）
    /// - `SlidingWindow` → `RedisSlidingWindowLimiter`（滑动窗口脚本）
    /// - `Quota`/`Custom` 仍由 QuotaController/Registry 处理（返回错误）。
    /// - `LeakyBucket`/`SlidingWindowLog`/`Concurrency` 等**当前仅进程内**：
    ///   落入兜底 `Self::create` 成为本地限流，多实例部署下实际全局放行量
    ///   = 配置值 × 实例数，分布式部署请慎用这些类型。
    ///
    /// `cache` 必须是 oxcache Redis 后端的 Cache 实例；无 Redis 连接配置
    /// 时调用方无法构造该实例，自然返回配置错误——限流语义不会静默
    /// 退化为进程内。
    #[cfg(all(feature = "distributed", feature = "lua-script"))]
    pub fn create_with_redis(
        config: &LimiterConfig,
        cache: oxcache::Cache<String, String>,
    ) -> Result<Arc<dyn Limiter>, LimiteronError> {
        use crate::limiters::distributed::{
            RedisDistributedLimiter, RedisSlidingWindowLimiter, RedisTokenBucketLimiter,
        };
        const DEFAULT_KEY: &str = "_global";
        match config {
            LimiterConfig::TokenBucket {
                capacity,
                refill_rate,
            } => {
                let inner = Arc::new(RedisDistributedLimiter::new(
                    cache, *capacity,
                    1000, // 1ms 基准窗口（令牌桶不使用窗口语义,占位）
                ));
                Ok(Arc::new(RedisTokenBucketLimiter::new(
                    inner,
                    DEFAULT_KEY,
                    *capacity,
                    *refill_rate,
                )))
            }
            LimiterConfig::SlidingWindow {
                window_size,
                max_requests,
            } => {
                let window_ms = Self::parse_window_size(window_size)?.as_millis() as u64;
                let inner = Arc::new(RedisDistributedLimiter::new(
                    cache,
                    *max_requests,
                    window_ms,
                ));
                Ok(Arc::new(RedisSlidingWindowLimiter::new(
                    inner,
                    DEFAULT_KEY,
                    window_ms,
                    *max_requests,
                )))
            }
            LimiterConfig::FixedWindow {
                window_size,
                max_requests,
            } => {
                let window_ms = Self::parse_window_size(window_size)?.as_millis() as u64;
                Ok(Arc::new(RedisDistributedLimiter::new(
                    cache,
                    *max_requests,
                    window_ms,
                )))
            }
            other => {
                // 仅进程内类型（LeakyBucket/SlidingWindowLog/Concurrency 等）
                // 走兜底降级：保留进程内语义（存量行为），但降级必须显性化
                // ——多实例部署下全局放行量 = 配置值 × 实例数，静默降级会让
                // 该偏差不可观测（fail-open 方向）
                tracing::warn!(
                    config = ?other,
                    "redis backend: limiter type has no distributed script, \
                     falling back to in-process limiter"
                );
                Self::create(other)
            }
        }
    }

    /// 批量创建限流器
    ///
    /// # 参数
    /// - `configs`: 限流器配置列表
    ///
    /// # 返回
    /// - `Ok(Vec<Arc<dyn Limiter>>)`: 创建成功的限流器列表
    /// - `Err(LimiteronError)`: 创建失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::limiters::factory::LimiterFactory;
    /// use limiteron::config::LimiterConfig;
    ///
    /// let configs = vec![
    ///     LimiterConfig::TokenBucket { capacity: 1000, refill_rate: 100 },
    ///     LimiterConfig::Concurrency { max_concurrent: 50 },
    /// ];
    /// let limiters = LimiterFactory::create_batch(&configs).unwrap();
    /// ```
    pub fn create_batch(
        configs: &[LimiterConfig],
    ) -> Result<Vec<Arc<dyn Limiter>>, LimiteronError> {
        let mut limiters = Vec::with_capacity(configs.len());

        for (index, config) in configs.iter().enumerate() {
            let limiter = Self::create(config).map_err(|e| {
                // 经 FTL 目录（键 limiter-create-failed，序号 1-based 保留）
                LimiteronError::LimitError(crate::i18n::t(
                    "limiter-create-failed",
                    &[
                        ("index", (index + 1).to_string()),
                        ("reason", e.to_string()),
                    ],
                ))
            })?;
            limiters.push(limiter);
        }

        Ok(limiters)
    }

    /// 解析窗口大小字符串
    ///
    /// # 参数
    /// - `window_size`: 窗口大小字符串（如 "1s", "1m", "1h"）
    ///
    /// # 返回
    /// - `Ok(Duration)`: 解析成功的时间段
    /// - `Err(LimiteronError)`: 解析失败
    ///
    /// # 支持的格式
    ///
    /// - `10s` - 10秒
    /// - `5m` - 5分钟
    /// - `2h` - 2小时
    /// - `1d` - 1天
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::limiters::factory::LimiterFactory;
    /// use std::time::Duration;
    ///
    /// let duration = LimiterFactory::parse_window_size("5m").unwrap();
    /// assert_eq!(duration, Duration::from_secs(300));
    /// ```
    pub fn parse_window_size(window_size: &str) -> Result<std::time::Duration, LimiteronError> {
        parse_window_size(window_size).map_err(LimiteronError::ConfigError)
    }

    /// 验证限流器配置
    ///
    /// # 参数
    /// - `config`: 要验证的限流器配置
    ///
    /// # 返回
    /// - `Ok(())`: 验证通过
    /// - `Err(LimiteronError)`: 验证失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::limiters::factory::LimiterFactory;
    /// use limiteron::config::LimiterConfig;
    ///
    /// let config = LimiterConfig::TokenBucket { capacity: 1000, refill_rate: 100 };
    /// LimiterFactory::validate_config(&config).unwrap();
    /// ```
    /// 验证窗口配置（适用于滑动窗口和固定窗口）
    ///
    /// `max_limit` 由调用方按算法传入：计数器型窗口用
    /// `MAX_WINDOW_REQUESTS`，日志型滑动窗口用更严的
    /// `MAX_SLIDING_LOG_REQUESTS`（条目内存随配额线性增长）。
    fn validate_window_config(
        window_size: &str,
        max_requests: u64,
        limiter_type: &str,
        max_limit: u64,
    ) -> Result<(), LimiteronError> {
        Self::parse_window_size(window_size)?;
        if max_requests == 0 {
            return Err(LimiteronError::ConfigError(t(
                "limiter-max-requests-must-be-positive",
                &[("limiter_type", limiter_type.to_string())],
            )));
        }
        if max_requests > max_limit {
            return Err(LimiteronError::ConfigError(t(
                "limiter-max-requests-too-large",
                &[
                    ("limiter_type", limiter_type.to_string()),
                    ("max", max_limit.to_string()),
                ],
            )));
        }
        Ok(())
    }

    pub fn validate_config(config: &LimiterConfig) -> Result<(), LimiteronError> {
        match config {
            LimiterConfig::TokenBucket {
                capacity,
                refill_rate,
            } => {
                if *capacity == 0 {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-capacity-must-be-positive",
                        &[],
                    )));
                }
                if *refill_rate == 0 {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-refill-rate-must-be-positive",
                        &[],
                    )));
                }
                if *capacity > MAX_TOKEN_BUCKET_CAPACITY {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-capacity-too-large",
                        &[("max", MAX_TOKEN_BUCKET_CAPACITY.to_string())],
                    )));
                }
                if *refill_rate > MAX_TOKEN_BUCKET_REFILL_RATE {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-refill-rate-too-large",
                        &[("max", MAX_TOKEN_BUCKET_REFILL_RATE.to_string())],
                    )));
                }
            }
            LimiterConfig::SlidingWindow {
                window_size,
                max_requests,
            } => {
                Self::validate_window_config(
                    window_size,
                    *max_requests,
                    "sliding window",
                    MAX_WINDOW_REQUESTS,
                )?;
            }
            LimiterConfig::FixedWindow {
                window_size,
                max_requests,
            } => {
                Self::validate_window_config(
                    window_size,
                    *max_requests,
                    "fixed window",
                    MAX_WINDOW_REQUESTS,
                )?;
            }
            LimiterConfig::Concurrency { max_concurrent } => {
                if *max_concurrent == 0 {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-max-concurrent-must-be-positive",
                        &[],
                    )));
                }
                if *max_concurrent > MAX_CONCURRENT_REQUESTS {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-max-concurrent-too-large",
                        &[("max", MAX_CONCURRENT_REQUESTS.to_string())],
                    )));
                }
            }
            LimiterConfig::PriorityQueue { .. } | LimiterConfig::AdmissionControl { .. } => {
                // 数值合法性规则与配置层校验（LimiterConfig::validate）完全一致，
                // 直接复用以免同一规则两处维护
                config.validate().map_err(LimiteronError::ConfigError)?;
            }
            LimiterConfig::LeakyBucket {
                capacity,
                leak_rate,
            } => {
                if *capacity == 0 {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-leaky-capacity-must-be-positive",
                        &[],
                    )));
                }
                if *leak_rate == 0 {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-leak-rate-must-be-positive",
                        &[],
                    )));
                }
                if *capacity > MAX_TOKEN_BUCKET_CAPACITY {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-leaky-capacity-too-large",
                        &[("max", MAX_TOKEN_BUCKET_CAPACITY.to_string())],
                    )));
                }
                if *leak_rate > MAX_TOKEN_BUCKET_REFILL_RATE {
                    return Err(LimiteronError::ConfigError(t(
                        "limiter-leak-rate-too-large",
                        &[("max", MAX_TOKEN_BUCKET_REFILL_RATE.to_string())],
                    )));
                }
            }
            LimiterConfig::SlidingWindowLog {
                window_size,
                max_requests,
            } => {
                Self::validate_window_config(
                    window_size,
                    *max_requests,
                    "sliding window log",
                    MAX_SLIDING_LOG_REQUESTS,
                )?;
            }
            LimiterConfig::Quota { .. } => {
                // Quota 类型由QuotaController处理
                return Err(LimiteronError::LimitError(t(
                    "limiter-quota-requires-controller",
                    &[],
                )));
            }
            LimiterConfig::Custom { .. } => {
                // Custom 类型由CustomLimiterRegistry处理
                return Err(LimiteronError::LimitError(t(
                    "limiter-custom-requires-registry",
                    &[],
                )));
            }
        }

        Ok(())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::QuotaType;
    use std::time::Duration;

    #[test]
    fn test_create_token_bucket() {
        let config = LimiterConfig::TokenBucket {
            capacity: 1000,
            refill_rate: 100,
        };

        let limiter = LimiterFactory::create(&config);
        assert!(limiter.is_ok());
    }

    #[test]
    fn test_create_sliding_window() {
        let config = LimiterConfig::SlidingWindow {
            window_size: "1m".to_string(),
            max_requests: 60,
        };

        let limiter = LimiterFactory::create(&config);
        assert!(limiter.is_ok());
    }

    #[test]
    fn test_create_fixed_window() {
        let config = LimiterConfig::FixedWindow {
            window_size: "30s".to_string(),
            max_requests: 30,
        };

        let limiter = LimiterFactory::create(&config);
        assert!(limiter.is_ok());
    }

    #[test]
    fn test_create_concurrency() {
        let config = LimiterConfig::Concurrency { max_concurrent: 50 };

        let limiter = LimiterFactory::create(&config);
        assert!(limiter.is_ok());
    }

    #[test]
    fn test_create_leaky_bucket() {
        let config = LimiterConfig::LeakyBucket {
            capacity: 100,
            leak_rate: 10,
        };

        let limiter = LimiterFactory::create(&config);
        assert!(limiter.is_ok());
    }

    #[test]
    fn test_create_sliding_window_log() {
        let config = LimiterConfig::SlidingWindowLog {
            window_size: "30s".to_string(),
            max_requests: 100,
        };

        let limiter = LimiterFactory::create(&config);
        assert!(limiter.is_ok());

        // 工厂产物的判定行为与直构一致（放行直至配额耗尽）
        let limiter = limiter.unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            for _ in 0..100 {
                assert!(Limiter::allow(limiter.as_ref(), 1).await.unwrap());
            }
            assert!(!Limiter::allow(limiter.as_ref(), 1).await.unwrap());
        });
    }

    #[test]
    fn test_create_batch() {
        let configs = vec![
            LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            },
            LimiterConfig::Concurrency { max_concurrent: 50 },
        ];

        let limiters = LimiterFactory::create_batch(&configs);
        assert!(limiters.is_ok());
        assert_eq!(limiters.unwrap().len(), 2);
    }

    #[test]
    fn test_parse_window_size_seconds() {
        let duration = LimiterFactory::parse_window_size("10s");
        assert!(duration.is_ok());
        assert_eq!(duration.unwrap(), Duration::from_secs(10));
    }

    #[test]
    fn test_parse_window_size_minutes() {
        let duration = LimiterFactory::parse_window_size("5m");
        assert!(duration.is_ok());
        assert_eq!(duration.unwrap(), Duration::from_secs(5 * 60));
    }

    #[test]
    fn test_parse_window_size_hours() {
        let duration = LimiterFactory::parse_window_size("2h");
        assert!(duration.is_ok());
        assert_eq!(duration.unwrap(), Duration::from_secs(2 * 3600));
    }

    #[test]
    fn test_parse_window_size_days() {
        let duration = LimiterFactory::parse_window_size("1d");
        assert!(duration.is_ok());
        assert_eq!(duration.unwrap(), Duration::from_secs(24 * 3600));
    }

    #[test]
    fn test_parse_window_size_invalid() {
        let duration = LimiterFactory::parse_window_size("invalid");
        assert!(duration.is_err());
    }

    #[test]
    fn test_parse_window_size_empty() {
        let duration = LimiterFactory::parse_window_size("");
        assert!(duration.is_err());
    }

    #[test]
    fn test_parse_window_size_zero() {
        let duration = LimiterFactory::parse_window_size("0s");
        assert!(duration.is_err());
    }

    #[test]
    fn test_validate_token_bucket_valid() {
        let config = LimiterConfig::TokenBucket {
            capacity: 1000,
            refill_rate: 100,
        };

        let result = LimiterFactory::validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_token_bucket_invalid_capacity() {
        let config = LimiterConfig::TokenBucket {
            capacity: 0,
            refill_rate: 100,
        };

        let result = LimiterFactory::validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_token_bucket_invalid_refill() {
        let config = LimiterConfig::TokenBucket {
            capacity: 1000,
            refill_rate: 0,
        };

        let result = LimiterFactory::validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_concurrency_valid() {
        let config = LimiterConfig::Concurrency { max_concurrent: 50 };

        let result = LimiterFactory::validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_concurrency_invalid() {
        let config = LimiterConfig::Concurrency { max_concurrent: 0 };

        let result = LimiterFactory::validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_leaky_bucket() {
        assert!(
            LimiterFactory::validate_config(&LimiterConfig::LeakyBucket {
                capacity: 100,
                leak_rate: 10,
            })
            .is_ok()
        );
        // 容量/漏速为零、超上限均拒绝
        assert!(
            LimiterFactory::validate_config(&LimiterConfig::LeakyBucket {
                capacity: 0,
                leak_rate: 10,
            })
            .is_err()
        );
        assert!(
            LimiterFactory::validate_config(&LimiterConfig::LeakyBucket {
                capacity: 100,
                leak_rate: 0,
            })
            .is_err()
        );
        assert!(
            LimiterFactory::validate_config(&LimiterConfig::LeakyBucket {
                capacity: MAX_TOKEN_BUCKET_CAPACITY + 1,
                leak_rate: 10,
            })
            .is_err()
        );
        assert!(
            LimiterFactory::validate_config(&LimiterConfig::LeakyBucket {
                capacity: 100,
                leak_rate: MAX_TOKEN_BUCKET_REFILL_RATE + 1,
            })
            .is_err()
        );
    }

    #[test]
    fn test_validate_sliding_window_log() {
        assert!(
            LimiterFactory::validate_config(&LimiterConfig::SlidingWindowLog {
                window_size: "1m".to_string(),
                max_requests: 100,
            })
            .is_ok()
        );
        assert!(
            LimiterFactory::validate_config(&LimiterConfig::SlidingWindowLog {
                window_size: "bogus".to_string(),
                max_requests: 100,
            })
            .is_err()
        );
        assert!(
            LimiterFactory::validate_config(&LimiterConfig::SlidingWindowLog {
                window_size: "1m".to_string(),
                max_requests: 0,
            })
            .is_err()
        );
    }

    #[test]
    fn test_validate_window_config_valid() {
        assert!(
            LimiterFactory::validate_window_config("1m", 100, "test", MAX_WINDOW_REQUESTS).is_ok()
        );
        assert!(
            LimiterFactory::validate_window_config("1h", 1000, "test", MAX_WINDOW_REQUESTS).is_ok()
        );
    }

    #[test]
    fn test_validate_window_config_invalid_size() {
        assert!(
            LimiterFactory::validate_window_config("", 100, "test", MAX_WINDOW_REQUESTS).is_err()
        );
    }

    #[test]
    fn test_validate_window_config_invalid_requests() {
        assert!(
            LimiterFactory::validate_window_config("1m", 0, "test", MAX_WINDOW_REQUESTS).is_err()
        );
    }

    #[test]
    fn test_validate_window_config_requests_exceeded() {
        assert!(
            LimiterFactory::validate_window_config("1m", 10_000_001, "test", MAX_WINDOW_REQUESTS)
                .is_err()
        );
        // 日志型窗口的独立更严上界：MAX_WINDOW_REQUESTS 内的值也可能超界
        assert!(
            LimiterFactory::validate_window_config(
                "1m",
                MAX_WINDOW_REQUESTS,
                "sliding window log",
                MAX_SLIDING_LOG_REQUESTS
            )
            .is_err()
        );
        assert!(
            LimiterFactory::validate_window_config(
                "1m",
                MAX_SLIDING_LOG_REQUESTS,
                "sliding window log",
                MAX_SLIDING_LOG_REQUESTS
            )
            .is_ok()
        );
    }

    #[test]
    fn test_create_quota_returns_error() {
        let config = LimiterConfig::Quota {
            quota_type: QuotaType::Count,
            limit: 100,
            window: "1m".to_string(),
            alert_threshold: None,
            overdraft: None,
        };
        assert!(LimiterFactory::create(&config).is_err());
    }

    #[test]
    fn test_create_custom_returns_error() {
        let config = LimiterConfig::Custom {
            name: "my_limiter".to_string(),
            config: serde_json::json!({}),
        };
        assert!(LimiterFactory::create(&config).is_err());
    }

    #[test]
    fn test_create_batch_with_invalid_config() {
        let configs = vec![
            LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            },
            LimiterConfig::Custom {
                name: "bad".to_string(),
                config: serde_json::json!({}),
            },
        ];
        let result = LimiterFactory::create_batch(&configs);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_batch_empty() {
        let result = LimiterFactory::create_batch(&[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_validate_token_bucket_capacity_exceeded() {
        let config = LimiterConfig::TokenBucket {
            capacity: 10_000_001,
            refill_rate: 100,
        };
        assert!(LimiterFactory::validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_token_bucket_refill_exceeded() {
        let config = LimiterConfig::TokenBucket {
            capacity: 1000,
            refill_rate: 1_000_001,
        };
        assert!(LimiterFactory::validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_sliding_window_valid() {
        let config = LimiterConfig::SlidingWindow {
            window_size: "1m".to_string(),
            max_requests: 100,
        };
        assert!(LimiterFactory::validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_sliding_window_invalid_window() {
        let config = LimiterConfig::SlidingWindow {
            window_size: "".to_string(),
            max_requests: 100,
        };
        assert!(LimiterFactory::validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_fixed_window_valid() {
        let config = LimiterConfig::FixedWindow {
            window_size: "1m".to_string(),
            max_requests: 100,
        };
        assert!(LimiterFactory::validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_fixed_window_invalid_window() {
        let config = LimiterConfig::FixedWindow {
            window_size: "".to_string(),
            max_requests: 100,
        };
        assert!(LimiterFactory::validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_concurrency_exceeded() {
        let config = LimiterConfig::Concurrency {
            max_concurrent: 100_001,
        };
        assert!(LimiterFactory::validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_quota_returns_limit_error() {
        let config = LimiterConfig::Quota {
            quota_type: QuotaType::Count,
            limit: 100,
            window: "1m".to_string(),
            alert_threshold: None,
            overdraft: None,
        };
        assert!(LimiterFactory::validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_custom_returns_limit_error() {
        let config = LimiterConfig::Custom {
            name: "my_limiter".to_string(),
            config: serde_json::json!({}),
        };
        assert!(LimiterFactory::validate_config(&config).is_err());
    }
}
