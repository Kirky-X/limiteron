// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 配额类型枚举

use serde::{Deserialize, Serialize};

/// 配额类型
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaType {
    /// 令牌配额
    Token,
    /// 金额配额
    Money,
    /// 计数配额
    #[default]
    Count,
}

impl QuotaType {
    /// 从字符串解析配额类型
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "token" => Some(QuotaType::Token),
            "money" => Some(QuotaType::Money),
            "count" => Some(QuotaType::Count),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            QuotaType::Token => "token",
            QuotaType::Money => "money",
            QuotaType::Count => "count",
        }
    }
}

impl std::fmt::Display for QuotaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_type_parse() {
        assert_eq!(QuotaType::parse("token"), Some(QuotaType::Token));
        assert_eq!(QuotaType::parse("TOKEN"), Some(QuotaType::Token));
        assert_eq!(QuotaType::parse("money"), Some(QuotaType::Money));
        assert_eq!(QuotaType::parse("count"), Some(QuotaType::Count));
        assert_eq!(QuotaType::parse("invalid"), None);
        assert_eq!(QuotaType::parse(""), None);
    }

    #[test]
    fn test_quota_type_as_str() {
        assert_eq!(QuotaType::Token.as_str(), "token");
        assert_eq!(QuotaType::Money.as_str(), "money");
        assert_eq!(QuotaType::Count.as_str(), "count");
    }

    #[test]
    fn test_quota_type_display() {
        assert_eq!(format!("{}", QuotaType::Token), "token");
        assert_eq!(format!("{}", QuotaType::Money), "money");
        assert_eq!(format!("{}", QuotaType::Count), "count");
    }

    #[test]
    fn test_quota_type_default() {
        assert_eq!(QuotaType::default(), QuotaType::Count);
    }

    #[test]
    fn test_quota_type_serde() {
        let token = QuotaType::Token;
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"token\"");
        let parsed: QuotaType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, QuotaType::Token);
    }
}
