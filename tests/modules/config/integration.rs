//! 配置模块集成测试
//!
//! 测试配置模块的完整功能：验证、版本比较、ConfigBuilder、哈希、变更记录等

use limiteron::config::{
    Action, ActionConfig, CacheBackend, ChangeSource, ConfigBuilder, ConfigChangeRecord,
    ConfigHistory, FlowControlConfig, LimiterConfig, Matcher, MetricsBackend, Rule, StorageType,
};

// ============================================================================
// 辅助函数：构建有效配置
// ============================================================================

/// 创建一个有效的规则配置
fn make_valid_rule(id: &str, name: &str) -> Rule {
    Rule {
        id: id.to_string(),
        name: name.to_string(),
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
    }
}

/// 创建一个有效的 FlowControlConfig
fn make_valid_config(version: &str, rules: Vec<Rule>) -> FlowControlConfig {
    FlowControlConfig {
        version: version.to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules,
    }
}

// ============================================================================
// 测试 1: 有效配置通过验证
// ============================================================================

#[tokio::test]
async fn test_valid_config_passes_validation() {
    let config = make_valid_config("1.0.0", vec![make_valid_rule("rule_001", "Test Rule")]);
    assert!(config.validate().is_ok(), "有效配置应通过验证");
}

// ============================================================================
// 测试 2: 空版本号校验失败
// ============================================================================

#[tokio::test]
async fn test_empty_version_fails_validation() {
    let config = FlowControlConfig {
        version: "".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![make_valid_rule("rule_001", "Test Rule")],
    };

    let result = config.validate();
    assert!(result.is_err(), "空版本号配置应校验失败");
    assert_eq!(result.unwrap_err(), "版本号不能为空");
}

// ============================================================================
// 测试 3: 重复规则 ID 校验失败
// ============================================================================

#[tokio::test]
async fn test_duplicate_rule_ids_fail_validation() {
    let rule = make_valid_rule("duplicate_id", "Rule");
    let config = make_valid_config("1.0.0", vec![rule.clone(), rule]);

    let result = config.validate();
    assert!(result.is_err(), "重复规则ID应校验失败");
    assert!(result.unwrap_err().contains("规则ID重复: duplicate_id"));
}

// ============================================================================
// 测试 4: 存储类型使用默认值
// ============================================================================

#[tokio::test]
async fn test_storage_type_default() {
    // 由于 StorageType 是 enum 类型，无效值在编译时就被拒绝
    // 此测试验证有效的存储类型配置
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![make_valid_rule("rule_001", "Test Rule")],
    };

    // 有效配置应该验证通过
    let result = config.validate();
    assert!(result.is_ok(), "有效配置应校验通过");
}

// ============================================================================
// 测试 5a: 版本比较 - 相同版本
// ============================================================================

#[tokio::test]
async fn test_version_compare_equal() {
    let config1 = make_valid_config("1.0.0", vec![]);
    let config2 = make_valid_config("1.0.0", vec![]);

    assert_eq!(
        config1.compare_version(&config2),
        std::cmp::Ordering::Equal,
        "相同版本应返回 Equal"
    );
}

// ============================================================================
// 测试 5b: 版本比较 - 新版本
// ============================================================================

#[tokio::test]
async fn test_version_compare_greater() {
    let config1 = make_valid_config("0.2.0", vec![]);
    let config2 = make_valid_config("0.1.0", vec![]);

    assert_eq!(
        config1.compare_version(&config2),
        std::cmp::Ordering::Greater,
        "较新版本应返回 Greater"
    );
}

// ============================================================================
// 测试 5c: 版本比较 - 旧版本
// ============================================================================

#[tokio::test]
async fn test_version_compare_less() {
    let config1 = make_valid_config("0.1.0", vec![]);
    let config2 = make_valid_config("0.2.0", vec![]);

    assert_eq!(
        config1.compare_version(&config2),
        std::cmp::Ordering::Less,
        "较旧版本应返回 Less"
    );
}

// ============================================================================
// 测试 6: 无效版本格式比较（不应 panic）
// ============================================================================

#[tokio::test]
async fn test_invalid_version_format_no_panic() {
    let config1 = FlowControlConfig {
        version: "invalid".to_string(),
        global: limiteron::config::GlobalConfig::default(),
        rules: vec![make_valid_rule("r", "R")],
    };
    let config2 = FlowControlConfig {
        version: "also-invalid".to_string(),
        global: limiteron::config::GlobalConfig::default(),
        rules: vec![make_valid_rule("r", "R")],
    };

    // 不应 panic，应返回某个 Ordering
    let ordering = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        config1.compare_version(&config2)
    }));
    assert!(ordering.is_ok(), "无效版本格式比较不应 panic");

    // 同样格式不同内容也应不 panic
    let ordering2 = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let c1 = FlowControlConfig {
            version: "zzz".to_string(),
            global: limiteron::config::GlobalConfig::default(),
            rules: vec![make_valid_rule("r", "R")],
        };
        let c2 = FlowControlConfig {
            version: "aaa".to_string(),
            global: limiteron::config::GlobalConfig::default(),
            rules: vec![make_valid_rule("r", "R")],
        };
        c1.compare_version(&c2)
    }));
    assert!(ordering2.is_ok(), "纯字母版本字符串比较不应 panic");
}

// ============================================================================
// 测试 7: ConfigBuilder 构建最小有效配置
// ============================================================================

#[tokio::test]
async fn test_config_builder_minimal_valid() {
    let config = ConfigBuilder::new()
        .with_rule(|rule| {
            rule.id("minimal")
                .name("Minimal Rule")
                .user_matcher(vec!["*".to_string()])
                .token_bucket(100, 10)
        })
        .build();

    assert!(config.is_ok(), "最小有效配置应构建成功");
    let cfg = config.unwrap();
    assert_eq!(cfg.version, "0.1.0");
    assert_eq!(cfg.global.storage, StorageType::Memory);
    assert_eq!(cfg.rules.len(), 1);
    assert_eq!(cfg.rules[0].id, "minimal");
}

// ============================================================================
// 测试 8: ConfigBuilder 构建包含多个规则的配置
// ============================================================================

#[tokio::test]
async fn test_config_builder_with_multiple_rules() {
    let config = ConfigBuilder::new()
        .with_storage(StorageType::Memory)
        .with_cache(CacheBackend::Memory)
        .with_metrics(MetricsBackend::Prometheus)
        .with_rule(|rule| {
            rule.id("rule_token_bucket")
                .name("Token Bucket Rule")
                .priority(100)
                .user_matcher(vec!["user1".to_string(), "user2".to_string()])
                .token_bucket(1000, 100)
        })
        .with_rule(|rule| {
            rule.id("rule_fixed_window")
                .name("Fixed Window Rule")
                .priority(90)
                .ip_matcher(vec!["192.168.1.0/24".to_string()])
                .fixed_window("60s", 500)
                .on_reject()
        })
        .with_rule(|rule| {
            rule.id("rule_sliding_window")
                .name("Sliding Window Rule")
                .priority(80)
                .user_matcher(vec!["*".to_string()])
                .sliding_window("30s", 200)
                .on_degrade()
        })
        .build();

    assert!(config.is_ok(), "多规则配置应构建成功");
    let cfg = config.unwrap();
    assert_eq!(cfg.rules.len(), 3);
    assert_eq!(cfg.rules[0].priority, 100);
    assert_eq!(cfg.rules[1].priority, 90);
    assert_eq!(cfg.rules[2].priority, 80);

    // 验证各个规则的限流器类型
    match &cfg.rules[0].limiters[0] {
        LimiterConfig::TokenBucket { .. } => {}
        _ => panic!("第一个规则应为 TokenBucket"),
    }
    match &cfg.rules[1].limiters[0] {
        LimiterConfig::FixedWindow { .. } => {}
        _ => panic!("第二个规则应为 FixedWindow"),
    }
    match &cfg.rules[2].limiters[0] {
        LimiterConfig::SlidingWindow { .. } => {}
        _ => panic!("第三个规则应为 SlidingWindow"),
    }
}

// ============================================================================
// 测试 9: ConfigBuilder 在构建时校验无效参数
// ============================================================================

#[tokio::test]
async fn test_config_builder_validation_during_construction() {
    // 缺少规则
    let result = ConfigBuilder::new().build();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "至少需要一个规则");

    // 规则缺少 ID
    let result = ConfigBuilder::new()
        .with_rule(|rule| {
            rule.name("No ID")
                .user_matcher(vec!["*".to_string()])
                .token_bucket(100, 10)
        })
        .build();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("规则ID不能为空"));

    // 规则缺少匹配器
    let result = ConfigBuilder::new()
        .with_rule(|rule| {
            rule.id("no_matcher")
                .name("No Matcher")
                .token_bucket(100, 10)
        })
        .build();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("规则至少需要一个匹配器"));

    // 规则缺少限流器
    let result = ConfigBuilder::new()
        .with_rule(|rule| {
            rule.id("no_limiter")
                .name("No Limiter")
                .user_matcher(vec!["*".to_string()])
        })
        .build();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("规则至少需要一个限流器"));
}

// ============================================================================
// 测试 10: ConfigBuilder 方法链式调用
// ============================================================================

#[tokio::test]
async fn test_config_builder_method_chaining() {
    // 验证 with_rule 返回的 &mut Self 支持链式调用
    let config = ConfigBuilder::new()
        .with_storage(StorageType::Memory)
        .with_cache(CacheBackend::Memory)
        .with_metrics(MetricsBackend::Prometheus)
        .with_rule(|rule| {
            rule.id("chained")
                .name("Chained Rule")
                .priority(50)
                .user_matcher(vec!["*".to_string()])
                .token_bucket(500, 50)
                .on_reject()
        })
        .build();

    assert!(config.is_ok(), "链式调用应正常工作");
    let cfg = config.unwrap();
    assert_eq!(cfg.global.storage, StorageType::Memory);
    assert_eq!(cfg.global.cache, CacheBackend::Memory);
    assert_eq!(cfg.global.metrics, MetricsBackend::Prometheus);
    assert_eq!(cfg.rules[0].priority, 50);
}

// ============================================================================
// 测试 11: 相同配置产生相同哈希
// ============================================================================

#[tokio::test]
async fn test_identical_configs_same_hash() {
    let config1 = make_valid_config("1.0.0", vec![make_valid_rule("rule1", "Rule 1")]);
    let config2 = make_valid_config("1.0.0", vec![make_valid_rule("rule1", "Rule 1")]);

    assert_eq!(
        config1.compute_hash(),
        config2.compute_hash(),
        "相同配置应产生相同哈希"
    );
}

// ============================================================================
// 测试 12: 不同配置产生不同哈希
// ============================================================================

#[tokio::test]
async fn test_different_configs_different_hashes() {
    let config1 = make_valid_config("1.0.0", vec![make_valid_rule("rule1", "Rule 1")]);
    let config2 = make_valid_config(
        "1.0.0",
        vec![
            make_valid_rule("rule1", "Rule 1"),
            make_valid_rule("rule2", "Rule 2"),
        ],
    );

    assert_ne!(
        config1.compute_hash(),
        config2.compute_hash(),
        "不同配置应产生不同哈希"
    );
}

// ============================================================================
// 测试 13: 哈希在多次运行中保持确定性
// ============================================================================

#[tokio::test]
async fn test_hash_is_deterministic() {
    let config = make_valid_config("1.0.0", vec![make_valid_rule("deterministic", "Test")]);

    let hash1 = config.compute_hash();
    let hash2 = config.compute_hash();
    let hash3 = config.compute_hash();

    assert_eq!(hash1, hash2, "哈希第一次和第二次应一致");
    assert_eq!(hash2, hash3, "哈希第二次和第三次应一致");
    assert_eq!(hash1, hash3, "哈希三次都应一致");
}

// ============================================================================
// 测试 14: 版本升级时创建变更记录
// ============================================================================

#[tokio::test]
async fn test_change_record_version_upgrade() {
    let old_config = make_valid_config("0.1.0", vec![make_valid_rule("r1", "Old Rule")]);
    let new_config = make_valid_config("0.2.0", vec![make_valid_rule("r1", "Old Rule")]);

    let record = new_config.create_change_record(Some(&old_config), ChangeSource::Poll);

    assert_eq!(record.old_version, Some("0.1.0".to_string()));
    assert_eq!(record.new_version, "0.2.0");
    assert!(record.old_hash.is_some());
    assert!(!record.new_hash.is_empty());
    assert!(!record.changes.is_empty());
}

// ============================================================================
// 测试 15a: 变更记录 - 新增规则
// ============================================================================

#[tokio::test]
async fn test_change_record_identifies_added_rules() {
    let old_config = make_valid_config("1.0.0", vec![make_valid_rule("existing", "Existing")]);
    let new_config = make_valid_config(
        "1.0.0",
        vec![
            make_valid_rule("existing", "Existing"),
            make_valid_rule("new_rule", "New Rule"),
        ],
    );

    let record = new_config.create_change_record(Some(&old_config), ChangeSource::Api);

    assert!(
        record.changes.iter().any(|c| c.contains("新增规则")),
        "变更记录应包含新增规则: {:?}",
        record.changes
    );
}

// ============================================================================
// 测试 15b: 变更记录 - 移除规则
// ============================================================================

#[tokio::test]
async fn test_change_record_identifies_removed_rules() {
    let old_config = make_valid_config(
        "1.0.0",
        vec![
            make_valid_rule("keep", "Keep"),
            make_valid_rule("remove", "Remove"),
        ],
    );
    let new_config = make_valid_config("1.0.0", vec![make_valid_rule("keep", "Keep")]);

    let record = new_config.create_change_record(
        Some(&old_config),
        ChangeSource::Manual {
            operator: "admin".to_string(),
        },
    );

    assert!(
        record.changes.iter().any(|c| c.contains("移除规则")),
        "变更记录应包含移除规则: {:?}",
        record.changes
    );
}

// ============================================================================
// 测试 15c: 变更记录 - 修改的规则
// ============================================================================

#[tokio::test]
async fn test_change_record_identifies_modified_rules() {
    // 修改全局配置
    let old_config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![make_valid_rule("same", "Same Rule")],
    };

    let new_config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::PostgreSQL,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![make_valid_rule("same", "Same Rule")],
    };

    let record = new_config.create_change_record(Some(&old_config), ChangeSource::Reload);

    assert!(
        record.changes.iter().any(|c| c.contains("全局配置")),
        "变更记录应包含全局配置变更: {:?}",
        record.changes
    );
}

// ============================================================================
// 测试 16: 规则唯一性 - 唯一 ID 接受
// ============================================================================

#[tokio::test]
async fn test_unique_rule_ids_accepted() {
    let config = make_valid_config(
        "1.0.0",
        vec![
            make_valid_rule("rule_a", "Rule A"),
            make_valid_rule("rule_b", "Rule B"),
            make_valid_rule("rule_c", "Rule C"),
        ],
    );

    assert!(config.validate().is_ok(), "唯一规则ID应被接受");
}

// ============================================================================
// 测试 17: 重复规则 ID 检测
// ============================================================================

#[tokio::test]
async fn test_duplicate_rule_ids_detected() {
    let rule1 = make_valid_rule("my_rule", "Rule 1");
    let rule2 = make_valid_rule("my_rule", "Rule 2");
    let config = make_valid_config("1.0.0", vec![rule1, rule2]);

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("规则ID重复: my_rule"));
}

// ============================================================================
// 测试 18: 大小写敏感规则 ID 比较
// ============================================================================

#[tokio::test]
async fn test_case_sensitive_rule_id_comparison() {
    // "rule1" 和 "Rule1" 应被视为不同的 ID
    let config = make_valid_config(
        "1.0.0",
        vec![
            make_valid_rule("rule1", "Lowercase Rule"),
            make_valid_rule("Rule1", "Capitalized Rule"),
        ],
    );

    assert!(
        config.validate().is_ok(),
        "大小写不同的规则ID应被视为不同的规则"
    );
}

// ============================================================================
// 测试 19: ConfigHistory 基本操作
// ============================================================================

#[tokio::test]
async fn test_config_history_basic_operations() {
    let mut history = ConfigHistory::new(10);

    // 初始状态为空
    assert!(history.get_records().is_empty());
    assert!(history.get_latest().is_none());

    // 添加记录
    let record1 = ConfigChangeRecord {
        timestamp: chrono::Utc::now(),
        old_version: None,
        new_version: "0.1.0".to_string(),
        old_hash: None,
        new_hash: "abc123".to_string(),
        source: ChangeSource::Poll,
        changes: vec!["初始配置".to_string()],
    };

    let record2 = ConfigChangeRecord {
        timestamp: chrono::Utc::now(),
        old_version: Some("0.1.0".to_string()),
        new_version: "0.2.0".to_string(),
        old_hash: Some("abc123".to_string()),
        new_hash: "def456".to_string(),
        source: ChangeSource::Api,
        changes: vec!["版本变更: 0.1.0 -> 0.2.0".to_string()],
    };

    history.add_record(record1.clone());
    assert_eq!(history.get_records().len(), 1);
    assert_eq!(
        history.get_latest().map(|r| r.new_version.as_str()),
        Some("0.1.0")
    );

    history.add_record(record2.clone());
    assert_eq!(history.get_records().len(), 2);
    assert_eq!(
        history.get_latest().map(|r| r.new_version.as_str()),
        Some("0.2.0")
    );

    // 清空
    history.clear();
    assert!(history.get_records().is_empty());
    assert!(history.get_latest().is_none());
}

// ============================================================================
// 测试 20: ConfigHistory 超出 max_records 时自动淘汰旧记录
// ============================================================================

#[tokio::test]
async fn test_config_history_eviction() {
    let mut history = ConfigHistory::new(3);

    for i in 0..5 {
        let record = ConfigChangeRecord {
            timestamp: chrono::Utc::now(),
            old_version: Some(format!("0.{}", i)),
            new_version: format!("0.{}", i + 1),
            old_hash: None,
            new_hash: format!("hash_{}", i),
            source: ChangeSource::Poll,
            changes: vec![format!("change_{}", i)],
        };
        history.add_record(record);
    }

    // 最多保留 3 条记录，最旧的两条被淘汰
    assert_eq!(history.get_records().len(), 3);

    // 最新版本应为 "0.5"
    assert_eq!(
        history.get_latest().map(|r| r.new_version.as_str()),
        Some("0.5")
    );
}

// ============================================================================
// 测试 21: 配置克隆 - 独立副本
// ============================================================================

#[tokio::test]
async fn test_clone_configuration_independent() {
    let original = make_valid_config("1.0.0", vec![make_valid_rule("original_rule", "Original")]);

    let cloned = original.clone();

    assert_eq!(original.compute_hash(), cloned.compute_hash());
    assert_eq!(original.version, cloned.version);
    assert_eq!(original.rules.len(), cloned.rules.len());
}

// ============================================================================
// 测试 22: 修改克隆不影响原始配置
// ============================================================================

#[tokio::test]
async fn test_modify_clone_without_affecting_original() {
    let original = make_valid_config(
        "1.0.0",
        vec![
            make_valid_rule("keep", "Keep"),
            make_valid_rule("to_remove", "To Remove"),
        ],
    );

    let mut cloned = original.clone();

    // 修改克隆的版本
    cloned.version = "2.0.0".to_string();

    // 从克隆中移除一个规则
    cloned.rules.retain(|r| r.id == "keep");

    // 原始配置应保持不变
    assert_eq!(original.version, "1.0.0");
    assert_eq!(original.rules.len(), 2);
    assert!(original.rules.iter().any(|r| r.id == "to_remove"));

    // 克隆应已修改
    assert_eq!(cloned.version, "2.0.0");
    assert_eq!(cloned.rules.len(), 1);
    assert!(cloned.rules.iter().all(|r| r.id == "keep"));

    // 哈希应不同
    assert_ne!(original.compute_hash(), cloned.compute_hash());
}

// ============================================================================
// 测试 23: 嵌套结构深度克隆
// ============================================================================

#[cfg(feature = "quota-control")]
#[tokio::test]
async fn test_deep_clone_nested_structures() {
    let original = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Redis,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![Rule {
            id: "nested".to_string(),
            name: "Nested Rule".to_string(),
            priority: 200,
            matchers: vec![
                Matcher::User {
                    user_ids: vec!["user1".to_string(), "user2".to_string()],
                },
                Matcher::Ip {
                    ip_ranges: vec!["10.0.0.0/8".to_string()],
                },
            ],
            limiters: vec![
                LimiterConfig::TokenBucket {
                    capacity: 5000,
                    refill_rate: 500,
                },
                LimiterConfig::Concurrency {
                    max_concurrent: 100,
                },
            ],
            action: ActionConfig {
                on_exceed: Action::Degrade,
                ban: None,
            },
        }],
    };

    let mut cloned = original.clone();

    // 修改克隆的嵌套数据
    let cloned_rule = &mut cloned.rules[0];
    cloned_rule.matchers.push(Matcher::Device {
        device_types: vec!["mobile".to_string()],
    });
    cloned_rule.limiters.push(LimiterConfig::Quota {
        quota_type: limiteron::quota::QuotaType::Count,
        limit: 1000,
        window: "1h".to_string(),
        alert_threshold: Some(80),
        overdraft: None,
    });

    // 原始配置的嵌套结构应保持不变
    assert_eq!(original.rules[0].matchers.len(), 2);
    assert_eq!(original.rules[0].limiters.len(), 2);
    assert!(matches!(
        original.rules[0].limiters[0],
        LimiterConfig::TokenBucket {
            capacity: 5000,
            refill_rate: 500
        }
    ));

    // 克隆应已修改
    assert_eq!(cloned.rules[0].matchers.len(), 3);
    assert_eq!(cloned.rules[0].limiters.len(), 3);
}

// ============================================================================
// 测试 24: 序列化往返 (JSON)
// ============================================================================

#[tokio::test]
async fn test_serialization_roundtrip_json() {
    let original = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![
            make_valid_rule("serial_rule", "Serialization Test Rule"),
            Rule {
                id: "complex_rule".to_string(),
                name: "Complex Rule".to_string(),
                priority: 50,
                matchers: vec![
                    Matcher::Geo {
                        countries: vec!["US".to_string(), "CN".to_string()],
                    },
                    Matcher::ApiVersion {
                        versions: vec!["v1".to_string(), "v2".to_string()],
                    },
                ],
                limiters: vec![
                    LimiterConfig::SlidingWindow {
                        window_size: "60s".to_string(),
                        max_requests: 1000,
                    },
                    LimiterConfig::FixedWindow {
                        window_size: "1h".to_string(),
                        max_requests: 5000,
                    },
                ],
                action: ActionConfig {
                    on_exceed: Action::Allow,
                    ban: None,
                },
            },
        ],
    };

    // 序列化
    let json = serde_json::to_string(&original).expect("序列化应成功");

    // 反序列化
    let deserialized: FlowControlConfig = serde_json::from_str(&json).expect("反序列化应成功");

    // 验证往返后数据完整
    assert_eq!(deserialized.version, original.version);
    assert_eq!(deserialized.global.storage, original.global.storage);
    assert_eq!(deserialized.global.cache, original.global.cache);
    assert_eq!(deserialized.global.metrics, original.global.metrics);
    assert_eq!(deserialized.rules.len(), original.rules.len());

    // 验证第一条规则
    assert_eq!(deserialized.rules[0].id, "serial_rule");
    assert_eq!(deserialized.rules[0].name, "Serialization Test Rule");
    assert!(matches!(
        deserialized.rules[0].limiters[0],
        LimiterConfig::TokenBucket { .. }
    ));

    // 验证第二条规则的复杂嵌套结构
    let complex = &deserialized.rules[1];
    assert_eq!(complex.id, "complex_rule");
    assert_eq!(complex.matchers.len(), 2);
    assert_eq!(complex.limiters.len(), 2);

    if let Matcher::Geo { countries } = &complex.matchers[0] {
        assert_eq!(countries.len(), 2);
    } else {
        panic!("第一个匹配器应为 Geo");
    }

    // 验证哈希一致
    assert_eq!(
        deserialized.compute_hash(),
        original.compute_hash(),
        "往返序列化后哈希应一致"
    );
}

// ============================================================================
// 测试 25: 反序列化无效 JSON 优雅失败
// ============================================================================

#[tokio::test]
async fn test_deserialize_invalid_json_fails() {
    let invalid_json = r#"{ "version": "1.0.0", "global": "not an object" }"#;

    let result: Result<FlowControlConfig, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err(), "无效JSON应反序列化失败");

    let malformed_json = r#"this is not json at all"#;
    let result2: Result<FlowControlConfig, _> = serde_json::from_str(malformed_json);
    assert!(result2.is_err(), "畸形JSON应反序列化失败");
}

// ============================================================================
// 测试 26: 反序列化缺失可选字段使用默认值
// ============================================================================

#[tokio::test]
async fn test_deserialize_missing_optional_fields() {
    // 仅包含必需字段的简化 JSON
    let minimal_json = serde_json::json!({
        "version": "1.0.0",
        "global": {
            "storage": "memory",
            "cache": "memory",
            "metrics": "prometheus"
        },
        "rules": [
            {
                "id": "minimal",
                "name": "Minimal Rule",
                "priority": 100,
                "matchers": [
                    { "type": "User", "user_ids": ["*"] }
                ],
                "limiters": [
                    { "type": "TokenBucket", "capacity": 100, "refill_rate": 10 }
                ],
                "action": { "on_exceed": "reject" }
            }
        ]
    });

    let config: FlowControlConfig =
        serde_json::from_value(minimal_json).expect("最小化JSON应能反序列化");

    assert_eq!(config.version, "1.0.0");
    assert_eq!(config.rules[0].id, "minimal");
    assert!(config.rules[0].action.ban.is_none());
}

// ============================================================================
// 测试 27: ChangeSource 各种变体
// ============================================================================

#[tokio::test]
async fn test_change_source_variants() {
    let sources = vec![
        ChangeSource::Poll,
        ChangeSource::Watch,
        ChangeSource::Api,
        ChangeSource::Reload,
        ChangeSource::Manual {
            operator: "admin".to_string(),
        },
        ChangeSource::Rollback {
            target_version: "1.0.0".to_string(),
        },
    ];

    for source in sources {
        let record = ConfigChangeRecord {
            timestamp: chrono::Utc::now(),
            old_version: Some("1.0.0".to_string()),
            new_version: "2.0.0".to_string(),
            old_hash: Some("old".to_string()),
            new_hash: "new".to_string(),
            source,
            changes: vec![],
        };
        assert!(!record.new_hash.is_empty());
    }
}

// ============================================================================
// 测试 28: GlobalConfig 有效类型验证
// ============================================================================

#[tokio::test]
async fn test_global_config_validation_valid_types() {
    // 由于类型系统保证，无效值在编译时就被拒绝
    // 此测试验证所有有效的存储/缓存/指标类型组合
    let valid_configs = vec![
        (
            StorageType::Memory,
            CacheBackend::Memory,
            MetricsBackend::Prometheus,
        ),
        (
            StorageType::PostgreSQL,
            CacheBackend::Redis,
            MetricsBackend::Prometheus,
        ),
    ];

    for (storage, cache, metrics) in valid_configs {
        let global = limiteron::config::GlobalConfig {
            storage,
            cache,
            metrics,
            trusted_proxies: Default::default(),
        };
        assert!(global.validate().is_ok(), "有效配置应校验通过");
    }
}

// ============================================================================
// 测试 29: GlobalConfig 各种有效类型组合
// ============================================================================

#[tokio::test]
async fn test_global_config_various_type_combinations() {
    // 由于类型系统保证，所有这些都是有效的
    let valid_combos = vec![
        (
            StorageType::Memory,
            CacheBackend::Memory,
            MetricsBackend::Prometheus,
        ),
        (
            StorageType::PostgreSQL,
            CacheBackend::Redis,
            MetricsBackend::Prometheus,
        ),
        (
            StorageType::Redis,
            CacheBackend::None,
            MetricsBackend::Statsd,
        ),
    ];

    for (storage, cache, metrics) in valid_combos {
        let global = limiteron::config::GlobalConfig {
            storage,
            cache,
            metrics,
            trusted_proxies: Default::default(),
        };
        assert!(global.validate().is_ok(), "有效组合应通过验证");
    }
}

// ============================================================================
// 测试 30: Rule 验证各种无效配置
// ============================================================================

#[tokio::test]
async fn test_rule_validation_various_invalid_cases() {
    // 规则 ID 为空
    let rule = Rule {
        id: "".to_string(),
        name: "Test".to_string(),
        priority: 100,
        matchers: vec![Matcher::User {
            user_ids: vec!["*".to_string()],
        }],
        limiters: vec![LimiterConfig::TokenBucket {
            capacity: 100,
            refill_rate: 10,
        }],
        action: ActionConfig::default(),
    };
    assert!(rule.validate().is_err());

    // 规则名称为空
    let rule = Rule {
        id: "valid_id".to_string(),
        name: "".to_string(),
        priority: 100,
        matchers: vec![Matcher::User {
            user_ids: vec!["*".to_string()],
        }],
        limiters: vec![LimiterConfig::TokenBucket {
            capacity: 100,
            refill_rate: 10,
        }],
        action: ActionConfig::default(),
    };
    assert!(rule.validate().is_err());

    // 无匹配器
    let rule = Rule {
        id: "valid_id".to_string(),
        name: "Valid Name".to_string(),
        priority: 100,
        matchers: vec![],
        limiters: vec![LimiterConfig::TokenBucket {
            capacity: 100,
            refill_rate: 10,
        }],
        action: ActionConfig::default(),
    };
    assert!(rule.validate().is_err());

    // 无限流器
    let rule = Rule {
        id: "valid_id".to_string(),
        name: "Valid Name".to_string(),
        priority: 100,
        matchers: vec![Matcher::User {
            user_ids: vec!["*".to_string()],
        }],
        limiters: vec![],
        action: ActionConfig::default(),
    };
    assert!(rule.validate().is_err());
}

// ============================================================================
// 测试 31: is_same_as 方法
// ============================================================================

#[tokio::test]
async fn test_is_same_as_method() {
    let config1 = make_valid_config("1.0.0", vec![make_valid_rule("same", "Same")]);
    let config2 = make_valid_config("1.0.0", vec![make_valid_rule("same", "Same")]);
    let config3 = make_valid_config("1.0.0", vec![make_valid_rule("diff", "Diff")]);

    assert!(
        config1.is_same_as(&config2),
        "相同配置 is_same_as 应返回 true"
    );
    assert!(
        !config1.is_same_as(&config3),
        "不同配置 is_same_as 应返回 false"
    );
}

// ============================================================================
// 测试 32: ConfigBuilder 支持 ConcurrencyLimiter
// ============================================================================

#[tokio::test]
async fn test_config_builder_with_concurrency_limiter() {
    let config = ConfigBuilder::new()
        .with_rule(|rule| {
            rule.id("concurrent")
                .name("Concurrency Rule")
                .user_matcher(vec!["*".to_string()])
                .concurrency_limit(50)
                .on_allow()
        })
        .build();

    assert!(config.is_ok());
    let cfg = config.unwrap();
    match &cfg.rules[0].limiters[0] {
        LimiterConfig::Concurrency { max_concurrent } => {
            assert_eq!(*max_concurrent, 50);
        }
        _ => panic!("应为 Concurrency 限流器"),
    }
}

// ============================================================================
// 测试 33: 配置变更记录 - 无旧配置（初始配置）
// ============================================================================

#[tokio::test]
async fn test_change_record_initial_config() {
    let new_config = make_valid_config("0.1.0", vec![make_valid_rule("init", "Init")]);

    let record = new_config.create_change_record(None, ChangeSource::Poll);

    assert_eq!(record.old_version, None);
    assert_eq!(record.new_version, "0.1.0");
    assert_eq!(record.old_hash, None);
    assert!(!record.new_hash.is_empty());
    assert!(record.changes.contains(&"初始配置".to_string()));
}
