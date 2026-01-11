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

use crate::config::LimiterConfig;
use crate::error::FlowGuardError;
use crate::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
};
use std::sync::Arc;

/// 限流器工厂
///
/// 提供统一的限流器创建接口，支持从配置创建各种限流器。
///
/// # 示例
///
/// ```rust
/// use limiteron::factory::LimiterFactory;
/// use limiteron::config::LimiterConfig;
///
/// // 创建令牌桶限流器
/// let config = LimiterConfig::TokenBucket {
///     capacity: 1000,
///     refill_rate: 100,
/// };
/// let limiter = LimiterFactory::create(&config)?;
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
    /// - `Err(FlowGuardError)`: 创建失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::factory::LimiterFactory;
    /// use limiteron::config::LimiterConfig;
    ///
    /// let config = LimiterConfig::TokenBucket {
    ///     capacity: 1000,
    ///     refill_rate: 100,
    /// };
    /// let limiter = LimiterFactory::create(&config)?;
    /// ```
    pub fn create(config: &LimiterConfig) -> Result<Arc<dyn Limiter>, FlowGuardError> {
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
                Ok(Arc::new(SlidingWindowLimiter::new(duration, *max_requests)))
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
            LimiterConfig::Quota {
                quota_type,
                limit,
                window,
                overdraft: _,
            } => {
                // Quota 类型由QuotaController处理
                Err(FlowGuardError::LimitError(
                    "Quota 限流器类型需要由QuotaController处理".to_string(),
                ))
            }
            LimiterConfig::Custom { .. } => {
                // Custom 类型由CustomLimiterRegistry处理
                Err(FlowGuardError::LimitError(
                    "Custom 限流器类型需要由CustomLimiterRegistry处理".to_string(),
                ))
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
    /// - `Err(FlowGuardError)`: 创建失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::factory::LimiterFactory;
    /// use limiteron::config::LimiterConfig;
    ///
    /// let configs = vec![
    ///     LimiterConfig::TokenBucket { capacity: 1000, refill_rate: 100 },
    ///     LimiterConfig::Concurrency { max_concurrent: 50 },
    /// ];
    /// let limiters = LimiterFactory::create_batch(&configs)?;
    /// ```
    pub fn create_batch(
        configs: &[LimiterConfig],
    ) -> Result<Vec<Arc<dyn Limiter>>, FlowGuardError> {
        let mut limiters = Vec::with_capacity(configs.len());

        for (index, config) in configs.iter().enumerate() {
            let limiter = Self::create(config).map_err(|e| {
                FlowGuardError::LimitError(format!("创建第 {} 个限流器失败: {}", index + 1, e))
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
    /// - `Err(FlowGuardError)`: 解析失败
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
    /// use limiteron::factory::LimiterFactory;
    /// use std::time::Duration;
    ///
    /// let duration = LimiterFactory::parse_window_size("5m")?;
    /// assert_eq!(duration, Duration::from_secs(300));
    /// ```
    pub fn parse_window_size(window_size: &str) -> Result<std::time::Duration, FlowGuardError> {
        if window_size.is_empty() {
            return Err(FlowGuardError::ConfigError("窗口大小不能为空".to_string()));
        }

        let (num_part, unit_part) = window_size.split_at(
            window_size
                .find(|c: char| c.is_alphabetic())
                .unwrap_or(window_size.len()),
        );

        let num_str = num_part.trim();
        let unit = unit_part.trim().to_lowercase();

        if num_str.is_empty() {
            return Err(FlowGuardError::ConfigError(
                "窗口大小格式错误：缺少数字部分".to_string(),
            ));
        }

        let num: u64 = num_str
            .parse()
            .map_err(|_| FlowGuardError::ConfigError(format!("无效的数字格式: {}", num_str)))?;

        if num == 0 {
            return Err(FlowGuardError::ConfigError("窗口大小必须大于0".to_string()));
        }

        let duration = match unit.as_str() {
            "s" | "sec" | "second" | "seconds" => std::time::Duration::from_secs(num),
            "m" | "min" | "minute" | "minutes" => std::time::Duration::from_secs(num * 60),
            "h" | "hr" | "hour" | "hours" => std::time::Duration::from_secs(num * 3600),
            "d" | "day" | "days" => std::time::Duration::from_secs(num * 86400),
            _ => {
                return Err(FlowGuardError::ConfigError(format!(
                    "不支持的单位: {}。支持的单位: s, m, h, d",
                    unit
                )));
            }
        };

        Ok(duration)
    }

    /// 验证限流器配置
    ///
    /// # 参数
    /// - `config`: 要验证的限流器配置
    ///
    /// # 返回
    /// - `Ok(())`: 验证通过
    /// - `Err(FlowGuardError)`: 验证失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::factory::LimiterFactory;
    /// use limiteron::config::LimiterConfig;
    ///
    /// let config = LimiterConfig::TokenBucket { capacity: 1000, refill_rate: 100 };
    /// LimiterFactory::validate_config(&config)?;
    /// ```
    pub fn validate_config(config: &LimiterConfig) -> Result<(), FlowGuardError> {
        match config {
            LimiterConfig::TokenBucket {
                capacity,
                refill_rate,
            } => {
                if *capacity == 0 {
                    return Err(FlowGuardError::ConfigError(
                        "令牌桶容量必须大于0".to_string(),
                    ));
                }
                if *refill_rate == 0 {
                    return Err(FlowGuardError::ConfigError(
                        "令牌桶补充速率必须大于0".to_string(),
                    ));
                }
                if *capacity > 10_000_000 {
                    return Err(FlowGuardError::ConfigError(
                        "令牌桶容量过大，最大值为10000000".to_string(),
                    ));
                }
                if *refill_rate > 1_000_000 {
                    return Err(FlowGuardError::ConfigError(
                        "令牌桶补充速率过大，最大值为1000000".to_string(),
                    ));
                }
            }
            LimiterConfig::SlidingWindow {
                window_size,
                max_requests,
            } => {
                Self::parse_window_size(window_size)?;
                if *max_requests == 0 {
                    return Err(FlowGuardError::ConfigError(
                        "滑动窗口最大请求数必须大于0".to_string(),
                    ));
                }
                if *max_requests > 10_000_000 {
                    return Err(FlowGuardError::ConfigError(
                        "滑动窗口最大请求数过大，最大值为10000000".to_string(),
                    ));
                }
            }
            LimiterConfig::FixedWindow {
                window_size,
                max_requests,
            } => {
                Self::parse_window_size(window_size)?;
                if *max_requests == 0 {
                    return Err(FlowGuardError::ConfigError(
                        "固定窗口最大请求数必须大于0".to_string(),
                    ));
                }
                if *max_requests > 10_000_000 {
                    return Err(FlowGuardError::ConfigError(
                        "固定窗口最大请求数过大，最大值为10000000".to_string(),
                    ));
                }
            }
            LimiterConfig::Concurrency { max_concurrent } => {
                if *max_concurrent == 0 {
                    return Err(FlowGuardError::ConfigError(
                        "并发限制数必须大于0".to_string(),
                    ));
                }
                if *max_concurrent > 100_000 {
                    return Err(FlowGuardError::ConfigError(
                        "并发限制数过大，最大值为100000".to_string(),
                    ));
                }
            }
            LimiterConfig::Quota { .. } => {
                // Quota 类型由QuotaController处理
                return Err(FlowGuardError::LimitError(
                    "Quota 限流器类型需要由QuotaController处理".to_string(),
                ));
            }
            LimiterConfig::Custom { .. } => {
                // Custom 类型由CustomLimiterRegistry处理
                return Err(FlowGuardError::LimitError(
                    "Custom 限流器类型需要由CustomLimiterRegistry处理".to_string(),
                ));
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
}
