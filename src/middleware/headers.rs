// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! HTTP 响应头注入
//!
//! 实现标准限流响应头的注入，遵循 IETF Draft 规范。
//!
//! # 响应头列表
//!
//! - `RateLimit-Limit`: 限流上限
//! - `RateLimit-Remaining`: 剩余可用次数
//! - `RateLimit-Reset`: 重置时间戳（Unix 秒）
//! - `Retry-After`: 重试等待时间（秒，仅在请求被拒绝时）
//!
//! # 参考
//!
//! - [IETF Draft: HTTP RateLimit Header Fields](https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/)

use http::Response;
use http::header::HeaderValue;

/// 限流响应头值
#[derive(Debug, Clone)]
pub struct RateLimitHeaderValues {
    /// 限流上限
    pub limit: u64,
    /// 剩余可用次数
    pub remaining: u64,
    /// 重置时间戳（Unix 秒）
    pub reset_at: u64,
    /// 重试等待时间（秒，仅在请求被拒绝时）
    pub retry_after: Option<u64>,
    /// 限流策略名称
    pub policy: String,
}

/// 向 HTTP 响应注入限流响应头
///
/// # 参数
///
/// - `response`: 原始 HTTP 响应
/// - `values`: 限流头值
///
/// # 返回
///
/// 注入限流头后的响应
///
/// # 示例
///
/// ```rust
/// use limiteron::middleware::{inject_rate_limit_headers, RateLimitHeaderValues};
/// use http::Response;
///
/// let mut response = Response::new(());
/// let values = RateLimitHeaderValues {
///     limit: 100,
///     remaining: 99,
///     reset_at: 1234567890,
///     retry_after: None,
///     policy: "token_bucket".to_string(),
/// };
///
/// let response = inject_rate_limit_headers(response, &values);
///
/// assert_eq!(response.headers().get("RateLimit-Limit").unwrap(), "100");
/// assert_eq!(response.headers().get("RateLimit-Remaining").unwrap(), "99");
/// assert_eq!(response.headers().get("RateLimit-Reset").unwrap(), "1234567890");
/// ```
pub fn inject_rate_limit_headers<B>(
    mut response: Response<B>,
    values: &RateLimitHeaderValues,
) -> Response<B> {
    // RateLimit-Limit
    if let Ok(header_value) = HeaderValue::from_str(&values.limit.to_string()) {
        response
            .headers_mut()
            .insert("RateLimit-Limit", header_value);
    }

    // RateLimit-Remaining
    if let Ok(header_value) = HeaderValue::from_str(&values.remaining.to_string()) {
        response
            .headers_mut()
            .insert("RateLimit-Remaining", header_value);
    }

    // RateLimit-Reset
    if let Ok(header_value) = HeaderValue::from_str(&values.reset_at.to_string()) {
        response
            .headers_mut()
            .insert("RateLimit-Reset", header_value);
    }

    // RateLimit-Policy (可选，包含策略名称)
    if !values.policy.is_empty() {
        if let Ok(header_value) = HeaderValue::from_str(&values.policy) {
            response
                .headers_mut()
                .insert("RateLimit-Policy", header_value);
        }
    }

    // Retry-After (仅在请求被拒绝时)
    if let Some(retry_after) = values.retry_after {
        if let Ok(header_value) = HeaderValue::from_str(&retry_after.to_string()) {
            response.headers_mut().insert("Retry-After", header_value);
        }
    }

    response
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_rate_limit_headers() {
        let response = Response::new(());
        let values = RateLimitHeaderValues {
            limit: 100,
            remaining: 99,
            reset_at: 1234567890,
            retry_after: None,
            policy: "token_bucket".to_string(),
        };

        let response = inject_rate_limit_headers(response, &values);

        assert_eq!(response.headers().get("RateLimit-Limit").unwrap(), "100");
        assert_eq!(response.headers().get("RateLimit-Remaining").unwrap(), "99");
        assert_eq!(
            response.headers().get("RateLimit-Reset").unwrap(),
            "1234567890"
        );
        assert_eq!(
            response.headers().get("RateLimit-Policy").unwrap(),
            "token_bucket"
        );
        assert!(response.headers().get("Retry-After").is_none());
    }

    #[test]
    fn test_inject_rate_limit_headers_with_retry_after() {
        let response = Response::new(());
        let values = RateLimitHeaderValues {
            limit: 100,
            remaining: 0,
            reset_at: 1234567890,
            retry_after: Some(60),
            policy: "fixed_window".to_string(),
        };

        let response = inject_rate_limit_headers(response, &values);

        assert_eq!(response.headers().get("RateLimit-Limit").unwrap(), "100");
        assert_eq!(response.headers().get("RateLimit-Remaining").unwrap(), "0");
        assert_eq!(response.headers().get("Retry-After").unwrap(), "60");
    }

    #[test]
    fn test_inject_rate_limit_headers_empty_policy() {
        let response = Response::new(());
        let values = RateLimitHeaderValues {
            limit: 50,
            remaining: 25,
            reset_at: 999999999,
            retry_after: None,
            policy: String::new(),
        };

        let response = inject_rate_limit_headers(response, &values);

        assert_eq!(response.headers().get("RateLimit-Limit").unwrap(), "50");
        assert!(response.headers().get("RateLimit-Policy").is_none());
    }

    #[test]
    fn test_inject_rate_limit_headers_invalid_policy_skipped() {
        // HeaderValue::from_str 拒绝非可见 ASCII 字符（如换行符），
        // 此时 RateLimit-Policy 头应被跳过（不 panic、不注入）
        let response = Response::new(());
        let values = RateLimitHeaderValues {
            limit: 10,
            remaining: 5,
            reset_at: 1,
            retry_after: None,
            policy: "bad\nvalue".to_string(),
        };

        let response = inject_rate_limit_headers(response, &values);

        // 数字头仍应正常注入
        assert_eq!(response.headers().get("RateLimit-Limit").unwrap(), "10");
        // 无效 policy 不注入
        assert!(response.headers().get("RateLimit-Policy").is_none());
    }

    #[test]
    fn test_inject_rate_limit_headers_all_zero_values() {
        // 边界值：全部为 0
        let response = Response::new(());
        let values = RateLimitHeaderValues {
            limit: 0,
            remaining: 0,
            reset_at: 0,
            retry_after: Some(0),
            policy: "zero".to_string(),
        };

        let response = inject_rate_limit_headers(response, &values);

        assert_eq!(response.headers().get("RateLimit-Limit").unwrap(), "0");
        assert_eq!(response.headers().get("RateLimit-Remaining").unwrap(), "0");
        assert_eq!(response.headers().get("RateLimit-Reset").unwrap(), "0");
        assert_eq!(response.headers().get("Retry-After").unwrap(), "0");
    }
}
