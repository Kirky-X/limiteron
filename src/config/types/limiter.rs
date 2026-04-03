// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! 限流器相关类型

use serde::{Deserialize, Serialize};

/// 透支配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverdraftConfig {
    pub enabled: bool,
    pub max_overdraft: u64,
}

impl OverdraftConfig {
    /// 校验透支配置
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled && self.max_overdraft == 0 {
            return Err("透支启用时，最大透支量不能为0".to_string());
        }
        Ok(())
    }
}

/// 限流器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LimiterConfig {
    TokenBucket {
        capacity: u64,
        refill_rate: u64,
    },
    SlidingWindow {
        window_size: String,
        max_requests: u64,
    },
    FixedWindow {
        window_size: String,
        max_requests: u64,
    },
    Quota {
        quota_type: String,
        limit: u64,
        window: String,
        /// 告警触发阈值（使用百分比 0-100），超过此比例时触发告警
        /// 默认值：80，即使用率达到 80% 时触发告警
        alert_threshold: Option<u8>,
        overdraft: Option<OverdraftConfig>,
    },
    Concurrency {
        max_concurrent: u64,
    },
    /// 自定义限流器
    Custom {
        /// 限流器名称
        name: String,
        /// 限流器配置（JSON格式）
        config: serde_json::Value,
    },
}

impl LimiterConfig {
    /// 校验限流器
    pub fn validate(&self) -> Result<(), String> {
        match self {
            LimiterConfig::TokenBucket {
                capacity,
                refill_rate,
            } => {
                if *capacity == 0 {
                    return Err("令牌桶容量不能为0".to_string());
                }
                if *refill_rate == 0 {
                    return Err("填充速率不能为0".to_string());
                }
            }
            LimiterConfig::SlidingWindow {
                window_size,
                max_requests,
            } => {
                if *max_requests == 0 {
                    return Err("最大请求数不能为0".to_string());
                }
                Self::validate_window_size(window_size)?;
            }
            LimiterConfig::FixedWindow {
                window_size,
                max_requests,
            } => {
                if *max_requests == 0 {
                    return Err("最大请求数不能为0".to_string());
                }
                Self::validate_window_size(window_size)?;
            }
            LimiterConfig::Quota {
                quota_type,
                limit,
                window,
                alert_threshold,
                overdraft,
            } => {
                if quota_type.is_empty() {
                    return Err("配额类型不能为空".to_string());
                }
                if *limit == 0 {
                    return Err("配额限制不能为0".to_string());
                }
                if let Some(threshold) = alert_threshold {
                    if *threshold > 100 {
                        return Err("告警阈值不能超过100%".to_string());
                    }
                }
                Self::validate_window_size(window)?;
                if let Some(overdraft) = overdraft {
                    overdraft.validate()?;
                }
            }
            LimiterConfig::Concurrency { max_concurrent } => {
                if *max_concurrent == 0 {
                    return Err("最大并发数不能为0".to_string());
                }
            }
            LimiterConfig::Custom { name, config } => {
                if name.is_empty() {
                    return Err("自定义限流器名称不能为空".to_string());
                }
                if config.is_null() {
                    return Err("自定义限流器配置不能为空".to_string());
                }
            }
        }
        Ok(())
    }

    /// 校验窗口大小
    fn validate_window_size(window_size: &str) -> Result<(), String> {
        parse_window_size(window_size).map(|_| ())
    }
}

/// 解析窗口大小字符串
pub(crate) fn parse_window_size(window_size: &str) -> Result<std::time::Duration, String> {
    let trimmed = window_size.trim();
    if trimmed.is_empty() {
        return Err("窗口大小不能为空".to_string());
    }

    let split_index = trimmed
        .find(|c: char| c.is_alphabetic())
        .unwrap_or(trimmed.len());
    let (num_part, unit_part) = trimmed.split_at(split_index);
    let num_str = num_part.trim();
    let unit = unit_part.trim().to_lowercase();

    if num_str.is_empty() {
        return Err("窗口大小格式错误：缺少数字部分".to_string());
    }

    if unit.is_empty() {
        return Err("窗口大小格式错误：缺少单位".to_string());
    }

    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("无效的数字格式: {}", num_str))?;

    if num == 0 {
        return Err("窗口大小必须大于0".to_string());
    }

    match unit.as_str() {
        "ms" | "millisecond" | "milliseconds" => Ok(std::time::Duration::from_millis(num)),
        "s" | "sec" | "second" | "seconds" => Ok(std::time::Duration::from_secs(num)),
        "m" | "min" | "minute" | "minutes" => Ok(std::time::Duration::from_secs(num * 60)),
        "h" | "hr" | "hour" | "hours" => Ok(std::time::Duration::from_secs(num * 3600)),
        "d" | "day" | "days" => Ok(std::time::Duration::from_secs(num * 86400)),
        _ => Err(format!(
            "不支持的单位: {}。支持的单位: ms, s, m, h, d",
            unit
        )),
    }
}
