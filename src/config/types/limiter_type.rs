// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! 限流器类型名称枚举
//!
//! 提供类型安全的限流器类型标识，替代硬编码字符串。

use serde::{Deserialize, Serialize};

/// 限流器类型名称
///
/// 替代硬编码字符串（"TokenBucket", "SlidingWindow" 等），
/// 提供类型安全的限流器类型标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LimiterTypeName {
    /// 令牌桶算法
    TokenBucket,
    /// 滑动窗口算法
    SlidingWindow,
    /// 固定窗口算法
    FixedWindow,
    /// 并发控制
    Concurrency,
    /// 配额限制
    Quota,
    /// 自定义限流器
    Custom,
}

impl LimiterTypeName {
    /// 从字符串解析
    #[cfg(test)]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "TokenBucket" => Some(Self::TokenBucket),
            "SlidingWindow" => Some(Self::SlidingWindow),
            "FixedWindow" => Some(Self::FixedWindow),
            "Concurrency" => Some(Self::Concurrency),
            "Quota" => Some(Self::Quota),
            "Custom" => Some(Self::Custom),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TokenBucket => "TokenBucket",
            Self::SlidingWindow => "SlidingWindow",
            Self::FixedWindow => "FixedWindow",
            Self::Concurrency => "Concurrency",
            Self::Quota => "Quota",
            Self::Custom => "Custom",
        }
    }
}

impl std::fmt::Display for LimiterTypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_bucket() {
        assert_eq!(
            LimiterTypeName::parse("TokenBucket"),
            Some(LimiterTypeName::TokenBucket)
        );
    }

    #[test]
    fn test_parse_sliding_window() {
        assert_eq!(
            LimiterTypeName::parse("SlidingWindow"),
            Some(LimiterTypeName::SlidingWindow)
        );
    }

    #[test]
    fn test_parse_fixed_window() {
        assert_eq!(
            LimiterTypeName::parse("FixedWindow"),
            Some(LimiterTypeName::FixedWindow)
        );
    }

    #[test]
    fn test_parse_concurrency() {
        assert_eq!(
            LimiterTypeName::parse("Concurrency"),
            Some(LimiterTypeName::Concurrency)
        );
    }

    #[test]
    fn test_parse_invalid() {
        assert_eq!(LimiterTypeName::parse("Invalid"), None);
        assert_eq!(LimiterTypeName::parse(""), None);
    }

    #[test]
    fn test_as_str() {
        assert_eq!(LimiterTypeName::TokenBucket.as_str(), "TokenBucket");
        assert_eq!(LimiterTypeName::SlidingWindow.as_str(), "SlidingWindow");
        assert_eq!(LimiterTypeName::FixedWindow.as_str(), "FixedWindow");
        assert_eq!(LimiterTypeName::Concurrency.as_str(), "Concurrency");
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", LimiterTypeName::TokenBucket), "TokenBucket");
        assert_eq!(format!("{}", LimiterTypeName::Custom), "Custom");
    }

    #[test]
    fn test_eq() {
        assert_eq!(LimiterTypeName::TokenBucket, LimiterTypeName::TokenBucket);
        assert_ne!(LimiterTypeName::TokenBucket, LimiterTypeName::SlidingWindow);
    }

    #[test]
    fn test_serialize_deserialize() {
        let original = LimiterTypeName::TokenBucket;
        let serialized = serde_json::to_string(&original).unwrap();
        assert_eq!(serialized, "\"TokenBucket\"");

        let deserialized: LimiterTypeName = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }
}
