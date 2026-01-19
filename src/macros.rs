//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Limiteron 宏框架
//!
//! 提供声明式的流量控制宏框架支持。
//!
//! # 宏使用
//!
//! 使用 `#[flow_control]` 属性宏为函数自动注入限流检查：
//!
//! ```rust
//! // 注意：#[flow_control] 宏不能在 doctest 中测试
//! // 它需要在实际的代码中使用，如下所示：
//!
//! // use limiteron::flow_control;
//! //
//! // #[flow_control(rate = "100/s")]
//! // async fn my_api_function(user_id: &str) -> String {
//! //     format!("Hello, {}", user_id)
//! // }
//! ```

// 重新导出过程宏
pub use flowguard_macros::flow_control;

/// 流量控制配置
#[derive(Debug, Clone)]
pub struct FlowControlConfig {
    /// 速率限制 (数量/单位)
    pub rate: Option<RateLimit>,
    /// 配额限制
    pub quota: Option<QuotaLimit>,
    /// 并发限制
    pub concurrency: Option<u32>,
    /// 标识符列表
    pub identifiers: Vec<String>,
    /// 超限行为
    pub on_exceed: String,
    /// 拒绝消息
    pub reject_message: String,
}

/// 速率限制配置
#[derive(Debug, Clone)]
pub struct RateLimit {
    /// 数量
    pub amount: u64,
    /// 单位 (s, m, h)
    pub unit: String,
}

/// 配额限制配置
#[derive(Debug, Clone)]
pub struct QuotaLimit {
    /// 最大配额
    pub max: u64,
    /// 周期
    pub period: String,
}

/// 解析速率限制字符串
pub fn parse_rate_limit(rate_str: &str) -> Result<RateLimit, String> {
    let parts: Vec<&str> = rate_str.split('/').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid rate format: '{}', expected 'amount/unit' (e.g., '100/s')",
            rate_str
        ));
    }

    let amount: u64 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid rate amount: '{}'", parts[0]))?;

    let unit = parts[1].to_lowercase();
    if !["s", "m", "h"].contains(&unit.as_str()) {
        return Err(format!(
            "Invalid rate unit: '{}', expected one of: s, m, h",
            unit
        ));
    }

    Ok(RateLimit { amount, unit })
}

/// 解析配额限制字符串
pub fn parse_quota_limit(quota_str: &str) -> Result<QuotaLimit, String> {
    let parts: Vec<&str> = quota_str.split('/').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid quota format: '{}', expected 'max/period' (e.g., '1000/h')",
            quota_str
        ));
    }

    let max: u64 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid quota max: '{}'", parts[0]))?;

    let period = parts[1].to_lowercase();
    if !["s", "m", "h", "d"].contains(&period.as_str()) {
        return Err(format!(
            "Invalid quota period: '{}', expected one of: s, m, h, d",
            period
        ));
    }

    Ok(QuotaLimit { max, period })
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rate_limit() {
        let rate = parse_rate_limit("100/s").unwrap();
        assert_eq!(rate.amount, 100);
        assert_eq!(rate.unit, "s");

        let rate = parse_rate_limit("50/m").unwrap();
        assert_eq!(rate.amount, 50);
        assert_eq!(rate.unit, "m");

        let rate = parse_rate_limit("10/h").unwrap();
        assert_eq!(rate.amount, 10);
        assert_eq!(rate.unit, "h");
    }

    #[test]
    fn test_parse_rate_limit_invalid() {
        assert!(parse_rate_limit("invalid").is_err());
        assert!(parse_rate_limit("100/x").is_err());
        assert!(parse_rate_limit("abc/s").is_err());
    }

    #[test]
    fn test_parse_rate_limit_units() {
        let rate = parse_rate_limit("100/s").unwrap();
        assert_eq!(rate.unit, "s");

        let rate = parse_rate_limit("100/S").unwrap();
        assert_eq!(rate.unit, "s"); // 应该转换为小写

        let rate = parse_rate_limit("100/M").unwrap();
        assert_eq!(rate.unit, "m");

        let rate = parse_rate_limit("100/H").unwrap();
        assert_eq!(rate.unit, "h");
    }

    #[test]
    fn test_parse_rate_limit_edge_cases() {
        // 测试边界值
        let rate = parse_rate_limit("1/s").unwrap();
        assert_eq!(rate.amount, 1);

        let rate = parse_rate_limit("999999999999/s").unwrap();
        assert_eq!(rate.amount, 999999999999);
    }

    #[test]
    fn test_parse_quota_limit() {
        let quota = parse_quota_limit("1000/h").unwrap();
        assert_eq!(quota.max, 1000);
        assert_eq!(quota.period, "h");

        let quota = parse_quota_limit("10000/d").unwrap();
        assert_eq!(quota.max, 10000);
        assert_eq!(quota.period, "d");
    }

    #[test]
    fn test_parse_quota_limit_invalid() {
        assert!(parse_quota_limit("invalid").is_err());
        assert!(parse_quota_limit("1000/x").is_err());
        assert!(parse_quota_limit("abc/h").is_err());
    }

    #[test]
    fn test_parse_quota_limit_periods() {
        let quota = parse_quota_limit("100/s").unwrap();
        assert_eq!(quota.period, "s");

        let quota = parse_quota_limit("100/m").unwrap();
        assert_eq!(quota.period, "m");

        let quota = parse_quota_limit("100/h").unwrap();
        assert_eq!(quota.period, "h");

        let quota = parse_quota_limit("100/d").unwrap();
        assert_eq!(quota.period, "d");
    }

    #[test]
    fn test_flow_control_config_default() {
        let config = FlowControlConfig {
            rate: None,
            quota: None,
            concurrency: None,
            identifiers: vec![],
            on_exceed: "reject".to_string(),
            reject_message: "Rate limit exceeded".to_string(),
        };

        assert!(config.rate.is_none());
        assert!(config.quota.is_none());
        assert!(config.concurrency.is_none());
        assert!(config.identifiers.is_empty());
        assert_eq!(config.on_exceed, "reject");
        assert_eq!(config.reject_message, "Rate limit exceeded");
    }
}
