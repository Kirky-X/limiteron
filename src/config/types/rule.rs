// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 规则相关类型

use serde::{Deserialize, Serialize};

use super::actions::ActionConfig;
use super::limiter::LimiterConfig;

/// 规则配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub priority: u16,
    pub matchers: Vec<Matcher>,
    pub limiters: Vec<LimiterConfig>,
    pub action: ActionConfig,
}

impl Rule {
    /// 校验规则
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("规则ID不能为空".to_string());
        }

        if self.name.is_empty() {
            return Err("规则名称不能为空".to_string());
        }

        if self.matchers.is_empty() {
            return Err("规则至少需要一个匹配器".to_string());
        }

        if self.limiters.is_empty() {
            return Err("规则至少需要一个限流器".to_string());
        }

        // 校验匹配器
        for (index, matcher) in self.matchers.iter().enumerate() {
            matcher
                .validate()
                .map_err(|e| format!("匹配器[{}]: {}", index, e))?;
        }

        // 校验限流器
        for (index, limiter) in self.limiters.iter().enumerate() {
            limiter
                .validate()
                .map_err(|e| format!("限流器[{}]: {}", index, e))?;
        }

        // 校验动作
        self.action.validate()?;

        Ok(())
    }
}

/// 匹配器
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Matcher {
    User {
        user_ids: Vec<String>,
    },
    Ip {
        ip_ranges: Vec<String>,
    },
    Geo {
        countries: Vec<String>,
    },
    ApiVersion {
        versions: Vec<String>,
    },
    Device {
        device_types: Vec<String>,
    },
    /// 自定义匹配器
    Custom {
        /// 匹配器名称
        name: String,
        /// 匹配器配置（JSON格式）
        config: serde_json::Value,
    },
}

impl Matcher {
    /// 校验匹配器
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Matcher::User { user_ids } => {
                if user_ids.is_empty() {
                    return Err("用户ID列表不能为空".to_string());
                }
            }
            Matcher::Ip { ip_ranges } => {
                if ip_ranges.is_empty() {
                    return Err("IP范围列表不能为空".to_string());
                }
            }
            Matcher::Geo { countries } => {
                if countries.is_empty() {
                    return Err("国家列表不能为空".to_string());
                }
            }
            Matcher::ApiVersion { versions } => {
                if versions.is_empty() {
                    return Err("API版本列表不能为空".to_string());
                }
            }
            Matcher::Device { device_types } => {
                if device_types.is_empty() {
                    return Err("设备类型列表不能为空".to_string());
                }
            }
            Matcher::Custom { name, config } => {
                if name.is_empty() {
                    return Err("自定义匹配器名称不能为空".to_string());
                }
                if config.is_null() {
                    return Err("自定义匹配器配置不能为空".to_string());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_rule() -> Rule {
        Rule {
            id: "rule1".into(),
            name: "Test Rule".into(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["u1".into()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig::default(),
        }
    }

    // ==================== Rule::validate ====================

    #[test]
    fn test_rule_valid() {
        assert!(valid_rule().validate().is_ok());
    }

    #[test]
    fn test_rule_empty_id() {
        let mut rule = valid_rule();
        rule.id = "".into();
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_rule_empty_name() {
        let mut rule = valid_rule();
        rule.name = "".into();
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_rule_no_matchers() {
        let mut rule = valid_rule();
        rule.matchers = vec![];
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_rule_no_limiters() {
        let mut rule = valid_rule();
        rule.limiters = vec![];
        assert!(rule.validate().is_err());
    }

    // ==================== Matcher::validate ====================

    #[test]
    fn test_matcher_user_valid() {
        assert!(
            Matcher::User {
                user_ids: vec!["u1".into()]
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn test_matcher_user_empty() {
        assert!(Matcher::User { user_ids: vec![] }.validate().is_err());
    }

    #[test]
    fn test_matcher_ip_valid() {
        assert!(
            Matcher::Ip {
                ip_ranges: vec!["10.0.0.0/8".into()]
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn test_matcher_ip_empty() {
        assert!(Matcher::Ip { ip_ranges: vec![] }.validate().is_err());
    }

    #[test]
    fn test_matcher_geo_empty() {
        assert!(Matcher::Geo { countries: vec![] }.validate().is_err());
    }

    #[test]
    fn test_matcher_api_version_empty() {
        assert!(Matcher::ApiVersion { versions: vec![] }.validate().is_err());
    }

    #[test]
    fn test_matcher_device_empty() {
        assert!(
            Matcher::Device {
                device_types: vec![]
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn test_matcher_custom_valid() {
        assert!(
            Matcher::Custom {
                name: "my-custom".into(),
                config: serde_json::json!({"key": "val"}),
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn test_matcher_custom_empty_name() {
        assert!(
            Matcher::Custom {
                name: "".into(),
                config: serde_json::json!({"key": "val"}),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn test_matcher_custom_null_config() {
        assert!(
            Matcher::Custom {
                name: "my-custom".into(),
                config: serde_json::Value::Null,
            }
            .validate()
            .is_err()
        );
    }
}
