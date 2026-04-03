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
