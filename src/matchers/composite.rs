//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 组合提取器
//!
//! 包含 CompositeExtractor 和 CompositeExtractorBuilder

use super::traits::{Identifier, IdentifierExtractor, RequestContext};
use std::net::IpAddr;

// ============================================================================
// 组合提取器
// ============================================================================

/// 组合提取器
///
/// 按顺序尝试多个提取器，直到成功提取标识符。
pub struct CompositeExtractor {
    /// 提取器列表（按优先级顺序）
    extractors: Vec<Box<dyn IdentifierExtractor>>,
    /// 是否在所有提取器都失败时返回默认标识符
    fallback_to_default: bool,
}

/// CompositeExtractor 构建器
///
/// 用于链式配置 CompositeExtractor 实例。
#[derive(Default)]
pub struct CompositeExtractorBuilder {
    extractors: Vec<Box<dyn IdentifierExtractor>>,
    fallback_to_default: bool,
}

impl CompositeExtractorBuilder {
    /// 创建新的 CompositeExtractorBuilder
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
            fallback_to_default: false,
        }
    }

    /// 添加提取器
    pub fn add_extractor(mut self, extractor: Box<dyn IdentifierExtractor>) -> Self {
        self.extractors.push(extractor);
        self
    }

    /// 设置是否在所有提取器都失败时返回默认标识符
    pub fn with_fallback(mut self, fallback: bool) -> Self {
        self.fallback_to_default = fallback;
        self
    }

    /// 构建 CompositeExtractor 实例
    pub fn build(self) -> CompositeExtractor {
        CompositeExtractor::with_dependencies(self.extractors, self.fallback_to_default)
    }
}

impl CompositeExtractor {
    /// 创建新的组合提取器
    ///
    /// # 参数
    /// - `extractors`: 提取器列表
    /// - `fallback_to_default`: 是否在所有提取器都失败时返回默认标识符
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::{CompositeExtractor, UserIdExtractor, IpExtractor};
    ///
    /// let extractor = CompositeExtractor::new(
    ///     vec![
    ///         Box::new(UserIdExtractor::from_header("X-User-Id")),
    ///         Box::new(IpExtractor::new_default()),
    ///     ],
    ///     true,
    /// );
    /// ```
    pub fn new(extractors: Vec<Box<dyn IdentifierExtractor>>, fallback_to_default: bool) -> Self {
        Self {
            extractors,
            fallback_to_default,
        }
    }

    /// 创建 CompositeExtractorBuilder 用于链式配置
    pub fn builder() -> CompositeExtractorBuilder {
        CompositeExtractorBuilder::new()
    }

    /// 使用依赖注入创建 CompositeExtractor（用于应用容器集成）
    pub fn with_dependencies(
        extractors: Vec<Box<dyn IdentifierExtractor>>,
        fallback_to_default: bool,
    ) -> Self {
        Self {
            extractors,
            fallback_to_default,
        }
    }

    /// 添加提取器
    ///
    /// # 参数
    /// - `extractor`: 提取器
    pub fn add_extractor(mut self, extractor: Box<dyn IdentifierExtractor>) -> Self {
        self.extractors.push(extractor);
        self
    }

    /// 设置是否回退到默认标识符
    ///
    /// # 参数
    /// - `fallback`: 是否回退
    pub fn with_fallback(mut self, fallback: bool) -> Self {
        self.fallback_to_default = fallback;
        self
    }
}

impl IdentifierExtractor for CompositeExtractor {
    fn extract(&self, context: &RequestContext) -> Option<Identifier> {
        // 按顺序尝试每个提取器
        for extractor in &self.extractors {
            if let Some(identifier) = extractor.extract(context) {
                return Some(identifier);
            }
        }

        // 如果所有提取器都失败且启用了回退，使用 IP 作为后备
        if self.fallback_to_default {
            // 使用 IP 作为后备，而不是固定的 "default"
            if let Some(client_ip) = &context.client_ip {
                // 验证 IP 格式
                if client_ip.parse::<IpAddr>().is_ok() {
                    return Some(Identifier::Ip(client_ip.clone()));
                }
            }
            // 如果没有 IP 或 IP 无效，返回 None
            // 这样可以让调用者决定如何处理未识别的请求
        }

        None
    }

    fn name(&self) -> &str {
        "CompositeExtractor"
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::matchers::{IpExtractor, UserIdExtractor};

    #[test]
    fn test_composite_builder() {
        let extractor = CompositeExtractor::builder()
            .add_extractor(Box::new(UserIdExtractor::from_header("X-User-Id")))
            .add_extractor(Box::new(IpExtractor::from_header("X-Forwarded-For")))
            .with_fallback(true)
            .build();
        assert_eq!(extractor.name(), "CompositeExtractor");
    }

    #[test]
    fn test_composite_fallback_to_client_ip() {
        let extractor = CompositeExtractor::new(vec![], true);
        let ctx = RequestContext::new().with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.1".into()))
        );
    }

    #[test]
    fn test_composite_fallback_no_ip() {
        let extractor = CompositeExtractor::new(vec![], true);
        let ctx = RequestContext::new();
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_composite_fallback_invalid_ip() {
        let extractor = CompositeExtractor::new(vec![], true);
        let ctx = RequestContext::new().with_client_ip("not-an-ip");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_composite_with_dependencies() {
        let extractor = CompositeExtractor::with_dependencies(
            vec![Box::new(UserIdExtractor::from_header("X-User-Id"))],
            false,
        );
        let ctx = RequestContext::new().with_header("X-User-Id", "u1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("u1".into()))
        );
    }

    #[test]
    fn test_composite_add_extractor() {
        let extractor = CompositeExtractor::new(vec![], false)
            .add_extractor(Box::new(UserIdExtractor::from_header("X-User-Id")));
        let ctx = RequestContext::new().with_header("X-User-Id", "added");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("added".into()))
        );
    }

    #[test]
    fn test_composite_with_fallback() {
        let extractor = CompositeExtractor::new(vec![], false).with_fallback(true);
        assert_eq!(extractor.name(), "CompositeExtractor");
    }

    #[test]
    fn test_composite_fallback_after_extractor_failure() {
        // 第一提取器存在但不匹配（返回 None），fallback 应启用并使用 client_ip
        let extractor = CompositeExtractor::new(
            vec![Box::new(UserIdExtractor::from_header("X-User-Id"))],
            true,
        );
        let ctx = RequestContext::new()
            .with_header("X-Other-Header", "value")
            .with_client_ip("10.0.0.2");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.2".into()))
        );
    }

    #[test]
    fn test_composite_no_fallback_after_extractor_failure() {
        // fallback_to_default=false 且提取器不匹配 → 返回 None（不回退到 IP）
        let extractor = CompositeExtractor::new(
            vec![Box::new(UserIdExtractor::from_header("X-User-Id"))],
            false,
        );
        let ctx = RequestContext::new()
            .with_header("X-Other-Header", "value")
            .with_client_ip("10.0.0.3");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_composite_first_extractor_wins() {
        // 多个提取器：第一个成功即返回，不尝试后续
        let extractor = CompositeExtractor::new(
            vec![
                Box::new(UserIdExtractor::from_header("X-User-Id")),
                Box::new(IpExtractor::from_header("X-Forwarded-For")),
            ],
            false,
        );
        let ctx = RequestContext::new()
            .with_header("X-User-Id", "winner")
            .with_header("X-Forwarded-For", "1.2.3.4");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("winner".into()))
        );
    }

    #[test]
    fn test_composite_builder_default_no_fallback() {
        // builder 默认 fallback_to_default=false
        let extractor = CompositeExtractor::builder()
            .add_extractor(Box::new(UserIdExtractor::from_header("X-User-Id")))
            .build();
        let ctx = RequestContext::new().with_client_ip("10.0.0.4");
        // 无 header 匹配，且 fallback=false → None
        assert_eq!(extractor.extract(&ctx), None);
    }
}
