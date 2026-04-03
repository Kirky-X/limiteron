//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
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

use http::header::HeaderValue;
use http::Response;

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

/// 从 HTTP 响应中移除所有限流响应头
///
/// 用于需要清理响应的场景。
pub fn remove_rate_limit_headers<B>(response: &mut Response<B>) {
    let headers = response.headers_mut();
    headers.remove("RateLimit-Limit");
    headers.remove("RateLimit-Remaining");
    headers.remove("RateLimit-Reset");
    headers.remove("RateLimit-Policy");
    headers.remove("Retry-After");
}

/// 检查响应是否包含限流响应头
pub fn has_rate_limit_headers<B>(response: &Response<B>) -> bool {
    let headers = response.headers();
    headers.contains_key("RateLimit-Limit")
        || headers.contains_key("RateLimit-Remaining")
        || headers.contains_key("RateLimit-Reset")
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_rate_limit_headers() {
        let mut response = Response::new(());
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
        let mut response = Response::new(());
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
        let mut response = Response::new(());
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
    fn test_remove_rate_limit_headers() {
        let mut response = Response::new(());
        let values = RateLimitHeaderValues {
            limit: 100,
            remaining: 99,
            reset_at: 1234567890,
            retry_after: Some(30),
            policy: "test".to_string(),
        };

        let mut response = inject_rate_limit_headers(response, &values);
        assert!(has_rate_limit_headers(&response));

        remove_rate_limit_headers(&mut response);
        assert!(!has_rate_limit_headers(&response));
    }

    #[test]
    fn test_has_rate_limit_headers() {
        let response = Response::new(());
        assert!(!has_rate_limit_headers(&response));

        let mut response = Response::new(());
        let values = RateLimitHeaderValues {
            limit: 100,
            remaining: 99,
            reset_at: 1234567890,
            retry_after: None,
            policy: String::new(),
        };

        let response = inject_rate_limit_headers(response, &values);
        assert!(has_rate_limit_headers(&response));
    }
}
