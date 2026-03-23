//! 配置集成测试
//!
//! 测试配置解析、环境变量覆盖和配置验证。

use limiteron::config::{
    Action, ActionConfig, BanConfig, BanScope, FlowControlConfig, LimiterConfig, Matcher, Rule,
};

// ==================== YAML 配置解析测试 ====================

/// 测试基本配置解析
#[test]
fn test_basic_config() {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: Default::default(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }],
    };

    assert_eq!(config.version, "1.0");
    assert_eq!(config.rules.len(), 1);
    assert_eq!(config.rules[0].id, "test_rule");
}

/// 测试多规则配置
#[test]
fn test_multi_rule_config() {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: Default::default(),
        },
        rules: vec![
            Rule {
                id: "api_v1".to_string(),
                name: "API V1 Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 1000,
                    refill_rate: 100,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            },
            Rule {
                id: "api_v2".to_string(),
                name: "API V2 Rule".to_string(),
                priority: 200,
                matchers: vec![Matcher::Ip {
                    ip_ranges: vec!["0.0.0.0/0".to_string()],
                }],
                limiters: vec![LimiterConfig::SlidingWindow {
                    window_size: "60s".to_string(),
                    max_requests: 500,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            },
        ],
    };

    assert_eq!(config.rules.len(), 2);
    assert_eq!(config.rules[0].id, "api_v1");
    assert_eq!(config.rules[1].id, "api_v2");
}

/// 测试不同限流器类型配置
#[test]
fn test_different_limiter_types() {
    // TokenBucket
    let token_bucket = LimiterConfig::TokenBucket {
        capacity: 100,
        refill_rate: 10,
    };
    match token_bucket {
        LimiterConfig::TokenBucket {
            capacity,
            refill_rate,
        } => {
            assert_eq!(capacity, 100);
            assert_eq!(refill_rate, 10);
        }
        _ => panic!("Expected TokenBucket"),
    }

    // SlidingWindow
    let sliding_window = LimiterConfig::SlidingWindow {
        window_size: "60s".to_string(),
        max_requests: 100,
    };
    match sliding_window {
        LimiterConfig::SlidingWindow {
            window_size,
            max_requests,
        } => {
            assert_eq!(window_size, "60s");
            assert_eq!(max_requests, 100);
        }
        _ => panic!("Expected SlidingWindow"),
    }

    // FixedWindow
    let fixed_window = LimiterConfig::FixedWindow {
        window_size: "60s".to_string(),
        max_requests: 50,
    };
    match fixed_window {
        LimiterConfig::FixedWindow {
            window_size,
            max_requests,
        } => {
            assert_eq!(window_size, "60s");
            assert_eq!(max_requests, 50);
        }
        _ => panic!("Expected FixedWindow"),
    }
}

/// 测试不同匹配器类型配置
#[test]
fn test_different_matcher_types() {
    // User matcher
    let user_matcher = Matcher::User {
        user_ids: vec!["user1".to_string(), "user2".to_string()],
    };
    match user_matcher {
        Matcher::User { user_ids } => {
            assert_eq!(user_ids.len(), 2);
        }
        _ => panic!("Expected User matcher"),
    }

    // IP matcher
    let ip_matcher = Matcher::Ip {
        ip_ranges: vec!["192.168.1.0/24".to_string()],
    };
    match ip_matcher {
        Matcher::Ip { ip_ranges } => {
            assert_eq!(ip_ranges.len(), 1);
        }
        _ => panic!("Expected Ip matcher"),
    }

    // Geo matcher
    let geo_matcher = Matcher::Geo {
        countries: vec!["CN".to_string(), "US".to_string()],
    };
    match geo_matcher {
        Matcher::Geo { countries } => {
            assert_eq!(countries.len(), 2);
        }
        _ => panic!("Expected Geo matcher"),
    }

    // Device matcher
    let device_matcher = Matcher::Device {
        device_types: vec!["mobile".to_string(), "desktop".to_string()],
    };
    match device_matcher {
        Matcher::Device { device_types } => {
            assert_eq!(device_types.len(), 2);
        }
        _ => panic!("Expected Device matcher"),
    }
}

/// 测试规则优先级
#[test]
fn test_rule_priority() {
    let rules = vec![
        Rule {
            id: "low_priority".to_string(),
            name: "Low Priority Rule".to_string(),
            priority: 50,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        },
        Rule {
            id: "high_priority".to_string(),
            name: "High Priority Rule".to_string(),
            priority: 200,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        },
    ];

    // 验证优先级设置
    assert_eq!(rules[0].priority, 50);
    assert_eq!(rules[1].priority, 200);
}

/// 测试动作配置
#[test]
fn test_action_config() {
    let action = ActionConfig {
        on_exceed: Action::Reject,
        ban: Some(BanConfig {
            threshold: 3,
            initial_duration: "1h".to_string(),
            backoff_multiplier: 2.0,
            max_duration: "24h".to_string(),
            scope: BanScope::User,
        }),
    };

    assert_eq!(action.on_exceed, Action::Reject);
    assert!(action.ban.is_some());
    let ban = action.ban.unwrap();
    assert_eq!(ban.threshold, 3);
    assert_eq!(ban.initial_duration, "1h");
    assert_eq!(ban.backoff_multiplier, 2.0);
    assert_eq!(ban.max_duration, "24h");
    assert_eq!(ban.scope, BanScope::User);
}

/// 测试全局配置
#[test]
fn test_global_config() {
    let global = limiteron::config::GlobalConfig {
        storage: "postgres".to_string(),
        cache: "redis".to_string(),
        metrics: "prometheus".to_string(),
        trusted_proxies: Default::default(),
    };

    assert_eq!(global.storage, "postgres");
    assert_eq!(global.cache, "redis");
    assert_eq!(global.metrics, "prometheus");
}

/// 测试配置序列化
#[test]
fn test_config_serialization() {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: Default::default(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }],
    };

    // 序列化为 YAML
    let yaml = serde_yaml::to_string(&config).expect("序列化失败");

    // 反序列化
    let deserialized: FlowControlConfig = serde_yaml::from_str(&yaml).expect("反序列化失败");

    assert_eq!(config.version, deserialized.version);
    assert_eq!(config.rules.len(), deserialized.rules.len());
}

/// 测试配置序列化为 JSON
#[test]
fn test_config_serialization_json() {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: Default::default(),
        },
        rules: vec![],
    };

    // 序列化为 JSON
    let json = serde_json::to_string(&config).expect("序列化失败");

    // 反序列化
    let deserialized: FlowControlConfig = serde_json::from_str(&json).expect("反序列化失败");

    assert_eq!(config.version, deserialized.version);
}

/// 测试空规则列表配置
#[test]
fn test_empty_rules_config() {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: Default::default(),
        },
        rules: vec![],
    };

    assert_eq!(config.rules.len(), 0);
}

/// 测试复杂匹配器组合
#[test]
fn test_complex_matcher_combination() {
    let rule = Rule {
        id: "complex_rule".to_string(),
        name: "Complex Rule".to_string(),
        priority: 100,
        matchers: vec![
            Matcher::User {
                user_ids: vec!["premium_user".to_string()],
            },
            Matcher::Ip {
                ip_ranges: vec!["192.168.0.0/16".to_string()],
            },
            Matcher::Geo {
                countries: vec!["CN".to_string()],
            },
        ],
        limiters: vec![LimiterConfig::TokenBucket {
            capacity: 1000,
            refill_rate: 100,
        }],
        action: ActionConfig {
            on_exceed: Action::Reject,
            ban: None,
        },
    };

    assert_eq!(rule.matchers.len(), 3);
}

/// 测试多限流器组合
#[test]
fn test_multiple_limiters() {
    let rule = Rule {
        id: "multi_limiter_rule".to_string(),
        name: "Multi Limiter Rule".to_string(),
        priority: 100,
        matchers: vec![Matcher::User {
            user_ids: vec!["*".to_string()],
        }],
        limiters: vec![
            LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            },
            LimiterConfig::SlidingWindow {
                window_size: "60s".to_string(),
                max_requests: 500,
            },
            LimiterConfig::FixedWindow {
                window_size: "1s".to_string(),
                max_requests: 100,
            },
        ],
        action: ActionConfig {
            on_exceed: Action::Reject,
            ban: None,
        },
    };

    assert_eq!(rule.limiters.len(), 3);
}

/// 测试自定义匹配器
#[test]
fn test_custom_matcher() {
    let custom_matcher = Matcher::Custom {
        name: "custom_auth".to_string(),
        config: serde_json::json!({"header": "X-Auth-Token"}),
    };

    match custom_matcher {
        Matcher::Custom { name, config } => {
            assert_eq!(name, "custom_auth");
            assert!(config.get("header").is_some());
            assert_eq!(
                config.get("header").unwrap().as_str().unwrap(),
                "X-Auth-Token"
            );
        }
        _ => panic!("Expected Custom matcher"),
    }
}

/// 测试 API 版本匹配器
#[test]
fn test_api_version_matcher() {
    let api_matcher = Matcher::ApiVersion {
        versions: vec!["v1".to_string(), "v2".to_string()],
    };

    match api_matcher {
        Matcher::ApiVersion { versions } => {
            assert_eq!(versions.len(), 2);
            assert!(versions.contains(&"v1".to_string()));
            assert!(versions.contains(&"v2".to_string()));
        }
        _ => panic!("Expected ApiVersion matcher"),
    }
}

/// 测试 BanConfig 验证
#[test]
fn test_ban_config_validation() {
    let ban_config = BanConfig {
        threshold: 3,
        initial_duration: "1h".to_string(),
        backoff_multiplier: 2.0,
        max_duration: "24h".to_string(),
        scope: BanScope::User,
    };

    // 验证配置
    let result = ban_config.validate();
    assert!(result.is_ok(), "BanConfig should be valid");
}

/// 测试默认 ActionConfig
#[test]
fn test_default_action_config() {
    let action = ActionConfig::default();

    assert_eq!(action.on_exceed, Action::Reject);
    assert!(action.ban.is_none());
}
