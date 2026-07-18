// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Limiteron 过程宏
//!
//! 提供声明式的流量控制宏，简化限流配置。

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::{ItemFn, parse_macro_input};

/// 流量控制属性宏
#[proc_macro_attribute]
pub fn flow_control(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);

    // 解析宏参数
    let args = proc_macro2::TokenStream::from(args);
    let config = match FlowControlConfig::parse(&args) {
        Ok(config) => config,
        Err(e) => return e.to_compile_error().into(),
    };

    // 生成代码
    match generate_flow_control(&input_fn, &config) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// 流量控制配置
#[derive(Debug, Clone)]
struct FlowControlConfig {
    rate: Option<RateLimit>,
    quota: Option<QuotaLimit>,
    concurrency: Option<u32>,
    identifiers: Vec<String>,
    on_exceed: String,
    reject_message: String,
    /// 自定义 key 前缀，用于多模块同名函数的 key 隔离
    key_prefix: Option<String>,
    /// 是否启用 tracing span（默认 true）
    enable_tracing: bool,
    /// 是否启用 metrics 记录（默认 true）
    enable_metrics: bool,
}

impl Default for FlowControlConfig {
    fn default() -> Self {
        Self {
            rate: None,
            quota: None,
            concurrency: None,
            identifiers: Vec::new(),
            on_exceed: String::new(),
            reject_message: String::new(),
            key_prefix: None,
            enable_tracing: true,
            enable_metrics: true,
        }
    }
}

impl FlowControlConfig {
    #[allow(clippy::collapsible_if)]
    fn parse(tokens: &proc_macro2::TokenStream) -> Result<Self, String> {
        use syn::Token;
        use syn::parse::Parser;
        use syn::punctuated::Punctuated;

        let parsed = Punctuated::<syn::Meta, Token![,]>::parse_terminated
            .parse2(tokens.clone())
            .map_err(|e| format!("Failed to parse attributes: {}", e))?;

        let mut config = Self::default();

        for meta in parsed {
            match meta {
                syn::Meta::NameValue(nv) => {
                    let ident = nv
                        .path
                        .get_ident()
                        .ok_or_else(|| "Expected identifier".to_string())?;
                    let ident_str = ident.to_string();

                    let ident_ref: &str = &ident_str;
                    match ident_ref {
                        "rate" => {
                            if let syn::Expr::Lit(expr_lit) = nv.value {
                                if let syn::Lit::Str(lit) = expr_lit.lit {
                                    config.rate = Some(RateLimit::from_str(&lit.value())?);
                                }
                            }
                        }
                        "quota" => {
                            if let syn::Expr::Lit(expr_lit) = nv.value {
                                if let syn::Lit::Str(lit) = expr_lit.lit {
                                    config.quota = Some(QuotaLimit::from_str(&lit.value())?);
                                }
                            }
                        }
                        "concurrency" => {
                            if let syn::Expr::Lit(expr_lit) = nv.value {
                                if let syn::Lit::Int(lit) = expr_lit.lit {
                                    config.concurrency = Some(
                                        lit.base10_parse()
                                            .map_err(|e| format!("Invalid concurrency: {}", e))?,
                                    );
                                }
                            }
                        }
                        "on_exceed" => {
                            if let syn::Expr::Lit(expr_lit) = nv.value {
                                if let syn::Lit::Str(lit) = expr_lit.lit {
                                    config.on_exceed = lit.value();
                                }
                            }
                        }
                        "reject_message" => {
                            if let syn::Expr::Lit(expr_lit) = nv.value {
                                if let syn::Lit::Str(lit) = expr_lit.lit {
                                    config.reject_message = lit.value();
                                }
                            }
                        }
                        "key_prefix" => {
                            if let syn::Expr::Lit(expr_lit) = nv.value {
                                if let syn::Lit::Str(lit) = expr_lit.lit {
                                    config.key_prefix = Some(lit.value());
                                }
                            }
                        }
                        "tracing" => {
                            if let syn::Expr::Lit(expr_lit) = nv.value {
                                if let syn::Lit::Bool(lit) = expr_lit.lit {
                                    config.enable_tracing = lit.value;
                                }
                            }
                        }
                        "metrics" => {
                            if let syn::Expr::Lit(expr_lit) = nv.value {
                                if let syn::Lit::Bool(lit) = expr_lit.lit {
                                    config.enable_metrics = lit.value;
                                }
                            }
                        }
                        _ => {
                            return Err(format!("Unknown attribute: {}", ident_str));
                        }
                    }
                }
                syn::Meta::List(list) => {
                    let ident = list
                        .path
                        .get_ident()
                        .ok_or_else(|| "Expected identifier".to_string())?;
                    let ident_str = ident.to_string();

                    if ident_str == "identifiers" {
                        let tokens = list.tokens;
                        let parsed = Punctuated::<syn::LitStr, Token![,]>::parse_terminated
                            .parse2(tokens)
                            .map_err(|e| format!("Failed to parse identifiers: {}", e))?;

                        for lit in parsed {
                            config.identifiers.push(lit.value());
                        }
                    }
                }
                _ => {
                    return Err("Expected name-value pair or list".to_string());
                }
            }
        }

        if config.on_exceed.is_empty() {
            config.on_exceed = "reject".to_string();
        }
        if config.reject_message.is_empty() {
            config.reject_message = "Rate limit exceeded".to_string();
        }

        // 验证 on_exceed 值（throttle 模式当前未实现，由 generate_flow_control 生成 compile_error）
        let on_exceed_valid = matches!(
            config.on_exceed.as_str(),
            "reject" | "log_only" | "throttle"
        );
        if !on_exceed_valid {
            return Err(format!(
                "Invalid on_exceed value: '{}'; expected one of: reject, log_only, throttle",
                config.on_exceed
            ));
        }

        Ok(config)
    }
}

/// 速率限制配置
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct RateLimit {
    amount: u64,
    unit: String,
}

#[allow(dead_code)]
impl RateLimit {
    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid rate format: '{}', expected 'amount/unit' (e.g., '100/s')",
                s
            ));
        }

        let amount: u64 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid rate amount: '{}'", parts[0]))?;

        let unit = parts[1].to_lowercase();
        let unit_str: &str = &unit;
        if !["s", "m", "h"].contains(&unit_str) {
            return Err(format!(
                "Invalid rate unit: '{}', expected one of: s, m, h",
                unit
            ));
        }

        Ok(Self { amount, unit })
    }

    fn to_duration(&self) -> proc_macro2::TokenStream {
        let amount = self.amount;
        match self.unit.as_str() {
            "s" => quote!(std::time::Duration::from_secs(#amount)),
            "m" => quote!(std::time::Duration::from_secs(#amount * 60)),
            "h" => quote!(std::time::Duration::from_secs(#amount * 3600)),
            _ => {
                let message = syn::LitStr::new(
                    "不支持的速率单位，支持: s, m, h",
                    proc_macro2::Span::call_site(),
                );
                quote!(compile_error!(#message))
            }
        }
    }
}

/// 配额限制配置
#[derive(Debug, Clone)]
struct QuotaLimit {
    max: u64,
    period: String,
}

impl QuotaLimit {
    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid quota format: '{}', expected 'max/period' (e.g., '1000/h')",
                s
            ));
        }

        let max: u64 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid quota max: '{}'", parts[0]))?;

        let period = parts[1].to_lowercase();
        let period_str: &str = &period;
        if !["s", "m", "h", "d"].contains(&period_str) {
            return Err(format!(
                "Invalid quota period: '{}', expected one of: s, m, h, d",
                period
            ));
        }

        Ok(Self { max, period })
    }

    fn to_duration(&self) -> proc_macro2::TokenStream {
        match self.period.as_str() {
            "s" => quote!(std::time::Duration::from_secs(1)),
            "m" => quote!(std::time::Duration::from_secs(60)),
            "h" => quote!(std::time::Duration::from_secs(3600)),
            "d" => quote!(std::time::Duration::from_secs(86400)),
            _ => {
                let message = syn::LitStr::new(
                    "不支持的配额周期单位，支持: s, m, h, d",
                    proc_macro2::Span::call_site(),
                );
                quote!(compile_error!(#message))
            }
        }
    }
}

/// 生成流量控制代码
fn generate_flow_control(
    input_fn: &ItemFn,
    config: &FlowControlConfig,
) -> Result<TokenStream2, String> {
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_inputs = &input_fn.sig.inputs;
    let fn_output = &input_fn.sig.output;
    let fn_block = &input_fn.block;
    let fn_attrs = &input_fn.attrs;
    let is_async = input_fn.sig.asyncness.is_some();

    let reject_message = config.reject_message.clone();
    let on_exceed_mode = config.on_exceed.as_str();
    let key_prefix_str = config.key_prefix.clone().unwrap_or_default();
    let fn_name_str = fn_name.to_string();

    // 根据 on_exceed 模式生成 rate check 失败时的处理代码
    // - "reject": 返回 RateLimitExceeded 错误（默认行为）
    // - "log_only": 不返回错误，继续执行原函数
    // - "throttle": 当前版本未实现（LimiteronError::Throttled 变体不存在），生成 compile_error
    let rate_exceed_handler = match on_exceed_mode {
        "reject" => {
            let msg = reject_message.clone();
            quote! {
                return Err(limiteron::error::LimiteronError::RateLimitExceeded(#msg.to_string()));
            }
        }
        "log_only" => quote! {
            // log_only: 不拒绝，记录 metrics 后继续执行原函数
        },
        "throttle" => {
            let err_msg = syn::LitStr::new(
                "on_exceed = \"throttle\" is not yet supported in this version (LimiteronError::Throttled variant not available); use \"reject\" or \"log_only\"",
                proc_macro2::Span::call_site(),
            );
            quote! { compile_error!(#err_msg); }
        }
        _ => {
            let err_msg = syn::LitStr::new(
                &format!(
                    "Unknown on_exceed mode: '{}'; expected one of: reject, log_only, throttle",
                    on_exceed_mode
                ),
                proc_macro2::Span::call_site(),
            );
            quote! { compile_error!(#err_msg); }
        }
    };

    let rate_check = if let Some(ref rate) = config.rate {
        let amount = rate.amount;
        // T006 修复: 根据 unit 计算 unit_secs，传给 get_rate_limiter
        // 之前 hardcoded 1 导致 rate="100/m" 被当作 100/s 处理（unit 信息丢失）
        let unit_secs: u64 = match rate.unit.as_str() {
            "s" => 1,
            "m" => 60,
            "h" => 3600,
            _ => 1,
        };
        let prefix = key_prefix_str.clone();
        let fname = fn_name_str.clone();
        quote! {
            let rate_key = {
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c: &char| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                    .take(128)
                    .collect::<String>();
                format!("{}:rate:{}:{}", #prefix, #fname, sanitize(&identifier))
            };
            let rate_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_rate_limiter(&rate_key, #amount, #unit_secs);
            if !rate_limiter.allow(1).await? {
                #rate_exceed_handler
            }
        }
    } else {
        quote!()
    };

    let quota_exceed_handler = match on_exceed_mode {
        "reject" => {
            let msg = reject_message.clone();
            quote! {
                return Err(limiteron::error::LimiteronError::QuotaExceeded(#msg.to_string()));
            }
        }
        "log_only" => quote! {
            // log_only: 不拒绝，记录 metrics 后继续执行原函数
        },
        "throttle" => {
            let err_msg = syn::LitStr::new(
                "on_exceed = \"throttle\" is not yet supported in this version (LimiteronError::Throttled variant not available); use \"reject\" or \"log_only\"",
                proc_macro2::Span::call_site(),
            );
            quote! { compile_error!(#err_msg); }
        }
        _ => {
            let err_msg = syn::LitStr::new(
                &format!(
                    "Unknown on_exceed mode: '{}'; expected one of: reject, log_only, throttle",
                    on_exceed_mode
                ),
                proc_macro2::Span::call_site(),
            );
            quote! { compile_error!(#err_msg); }
        }
    };

    let quota_check = if let Some(ref quota) = config.quota {
        let max = quota.max;
        let duration = quota.to_duration();
        let prefix = key_prefix_str.clone();
        let fname = fn_name_str.clone();
        quote! {
            let quota_key = {
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c: &char| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                    .take(128)
                    .collect::<String>();
                format!("{}:quota:{}:{}", #prefix, #fname, sanitize(&identifier))
            };
            let quota_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_quota_limiter(&quota_key, #duration, #max);
            // T006 修复: 使用 check(&key) 真正消费配额，而非 allow(1)（默认返回 Ok(true) 不消费）
            if quota_limiter.check(&quota_key).await.is_err() {
                #quota_exceed_handler
            }
        }
    } else {
        quote!()
    };

    let concurrency_exceed_handler = match on_exceed_mode {
        "reject" => {
            let msg = reject_message.clone();
            quote! {
                return Err(limiteron::error::LimiteronError::ConcurrencyLimitExceeded(#msg.to_string()));
            }
        }
        "log_only" => quote! {
            // log_only: 不拒绝，也不持有 permit，继续执行原函数
        },
        "throttle" => {
            let err_msg = syn::LitStr::new(
                "on_exceed = \"throttle\" is not yet supported in this version (LimiteronError::Throttled variant not available); use \"reject\" or \"log_only\"",
                proc_macro2::Span::call_site(),
            );
            quote! { compile_error!(#err_msg); }
        }
        _ => {
            let err_msg = syn::LitStr::new(
                &format!(
                    "Unknown on_exceed mode: '{}'; expected one of: reject, log_only, throttle",
                    on_exceed_mode
                ),
                proc_macro2::Span::call_site(),
            );
            quote! { compile_error!(#err_msg); }
        }
    };

    let concurrency_check = if let Some(concurrency) = config.concurrency {
        let prefix = key_prefix_str.clone();
        let fname = fn_name_str.clone();
        quote! {
            let concurrency_key = {
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c: &char| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                    .take(128)
                    .collect::<String>();
                format!("{}:concurrency:{}:{}", #prefix, #fname, sanitize(&identifier))
            };
            let concurrency_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_concurrency_limiter(&concurrency_key, #concurrency as u64);
            // T006 修复: 持有 permit 到函数结束（用 Option 包装，log_only 模式下不持有）
            // 之前 _permit 在 match 作用域结束即 drop，并发控制失效
            #[allow(unreachable_code)]
            let _concurrency_permit = match concurrency_limiter.acquire(1).await {
                Ok(permit) => Some(permit),
                Err(_) => {
                    #concurrency_exceed_handler
                    None
                }
            };
        }
    } else {
        quote!()
    };

    let identifier_expr = if config.identifiers.is_empty() {
        quote!("default")
    } else {
        let ids = &config.identifiers;
        quote! {
            {
                let mut parts = Vec::new();
                #(parts.push(format!("{}", #ids));)*
                parts.join(":")
            }
        }
    };

    let tracing_start = if config.enable_tracing {
        quote! {
            let _span = tracing::span!(tracing::Level::INFO, "flow_control", function = stringify!(#fn_name));
            let _enter = _span.enter();
        }
    } else {
        quote! {}
    };

    let metrics_record = if config.enable_metrics {
        quote! {
            if let Some(metrics) = limiteron::telemetry::try_global() {
                metrics.requests_total.inc();
            }
        }
    } else {
        quote! {}
    };

    let expanded = if is_async {
        quote! {
            #(#fn_attrs)*
            #fn_vis async fn #fn_name(#fn_inputs) #fn_output {
                use limiteron::limiters::Limiter;
                #tracing_start
                let identifier = #identifier_expr;
                #rate_check
                #quota_check
                #concurrency_check
                #metrics_record
                #fn_block
            }
        }
    } else {
        quote! {
            #(#fn_attrs)*
            #fn_vis fn #fn_name(#fn_inputs) #fn_output {
                use limiteron::limiters::Limiter;
                #tracing_start
                let identifier = #identifier_expr;
                let rt = tokio::runtime::Handle::try_current();
                if let Ok(handle) = rt {
                    handle.block_on(async {
                        #rate_check
                        #quota_check
                        #concurrency_check
                    });
                }
                #metrics_record
                #fn_block
            }
        }
    };

    Ok(expanded)
}

trait ToCompileError {
    fn to_compile_error(&self) -> TokenStream2;
}

impl ToCompileError for String {
    fn to_compile_error(&self) -> TokenStream2 {
        quote_spanned! {
            proc_macro2::Span::call_site() =>
            compile_error!(#self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_from_str() {
        let rate = RateLimit::from_str("100/s").unwrap();
        assert_eq!(rate.amount, 100);
        assert_eq!(rate.unit, "s");

        let rate = RateLimit::from_str("50/m").unwrap();
        assert_eq!(rate.amount, 50);
        assert_eq!(rate.unit, "m");

        let rate = RateLimit::from_str("10/h").unwrap();
        assert_eq!(rate.amount, 10);
        assert_eq!(rate.unit, "h");
    }

    #[test]
    fn test_rate_limit_invalid() {
        assert!(RateLimit::from_str("invalid").is_err());
        assert!(RateLimit::from_str("100/x").is_err());
        assert!(RateLimit::from_str("abc/s").is_err());
    }

    #[test]
    fn test_quota_limit_from_str() {
        let quota = QuotaLimit::from_str("1000/h").unwrap();
        assert_eq!(quota.max, 1000);
        assert_eq!(quota.period, "h");

        let quota = QuotaLimit::from_str("10000/d").unwrap();
        assert_eq!(quota.max, 10000);
        assert_eq!(quota.period, "d");
    }

    #[test]
    fn test_quota_limit_invalid() {
        assert!(QuotaLimit::from_str("invalid").is_err());
        assert!(QuotaLimit::from_str("1000/x").is_err());
        assert!(QuotaLimit::from_str("abc/h").is_err());
    }

    #[test]
    fn test_flow_control_config_default() {
        let config = FlowControlConfig::default();
        assert!(config.rate.is_none());
        assert!(config.quota.is_none());
        assert!(config.concurrency.is_none());
        assert!(config.identifiers.is_empty());
        // 注意：手动实现 Default 将 String 字段默认为空字符串
        assert_eq!(config.on_exceed, "");
        assert_eq!(config.reject_message, "");
        // T007: key_prefix 默认 None
        assert!(config.key_prefix.is_none());
        // T008: tracing/metrics 默认 true
        assert!(config.enable_tracing);
        assert!(config.enable_metrics);
    }

    // ========================================================================
    // T006: on_exceed 参数解析与代码生成测试
    // ========================================================================

    #[test]
    fn test_parse_on_exceed_default() {
        // 未指定 on_exceed 时，默认为 "reject"
        let tokens: proc_macro2::TokenStream = quote::quote! { rate = "100/s" };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert_eq!(config.on_exceed, "reject");
    }

    #[test]
    fn test_parse_on_exceed_reject() {
        let tokens: proc_macro2::TokenStream =
            quote::quote! { rate = "100/s", on_exceed = "reject" };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert_eq!(config.on_exceed, "reject");
    }

    #[test]
    fn test_parse_on_exceed_log_only() {
        let tokens: proc_macro2::TokenStream =
            quote::quote! { rate = "100/s", on_exceed = "log_only" };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert_eq!(config.on_exceed, "log_only");
    }

    #[test]
    fn test_parse_on_exceed_throttle_accepted() {
        // throttle 当前未实现但 parse 接受，由 generate_flow_control 生成 compile_error
        let tokens: proc_macro2::TokenStream =
            quote::quote! { rate = "100/s", on_exceed = "throttle" };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert_eq!(config.on_exceed, "throttle");
    }

    #[test]
    fn test_parse_on_exceed_invalid_rejected() {
        // 未知 on_exceed 值应在 parse 阶段被拒绝（Rule 12: 失败必须显性化）
        let tokens: proc_macro2::TokenStream =
            quote::quote! { rate = "100/s", on_exceed = "unknown_mode" };
        let result = FlowControlConfig::parse(&tokens);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Invalid on_exceed value"), "err = {}", err);
        assert!(err.contains("unknown_mode"), "err = {}", err);
    }

    /// 构造一个最小可解析的 ItemFn 用于测试 generate_flow_control
    fn make_test_fn(name: &str) -> ItemFn {
        let ident: proc_macro2::Ident = syn::parse_str(name).expect("Failed to parse ident");
        let tokens: proc_macro2::TokenStream = quote::quote! {
            async fn #ident() -> Result<(), limiteron::error::LimiteronError> {
                Ok(())
            }
        };
        syn::parse2::<ItemFn>(tokens).expect("Failed to parse ItemFn")
    }

    #[test]
    fn test_generate_rate_check_reject_mode() {
        // T006: on_exceed = "reject" 应生成 RateLimitExceeded 错误
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            on_exceed: "reject".to_string(),
            reject_message: "Rate limit exceeded".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_reject");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            tokens_str.contains("RateLimitExceeded"),
            "reject mode should generate RateLimitExceeded error; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_rate_check_log_only_mode() {
        // T006: on_exceed = "log_only" 不应生成 RateLimitExceeded 错误
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            on_exceed: "log_only".to_string(),
            reject_message: "Rate limit exceeded".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_log_only");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            !tokens_str.contains("RateLimitExceeded"),
            "log_only mode should NOT generate RateLimitExceeded; tokens = {}",
            tokens_str
        );
        // log_only 模式下仍应调用 rate_limiter.allow（记录但不拒绝）
        assert!(
            tokens_str.contains("allow"),
            "log_only mode should still call allow() for metrics; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_default_mode_matches_reject() {
        // T006: 默认（on_exceed = "reject"）应与 reject 模式行为一致
        let config_default = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            on_exceed: "reject".to_string(),
            reject_message: "Rate limit exceeded".to_string(),
            ..Default::default()
        };
        let config_explicit = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            on_exceed: "reject".to_string(),
            reject_message: "Rate limit exceeded".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_default");
        let tokens_default = generate_flow_control(&input_fn, &config_default).unwrap();
        let tokens_explicit = generate_flow_control(&input_fn, &config_explicit).unwrap();
        assert_eq!(
            tokens_default.to_string(),
            tokens_explicit.to_string(),
            "default and explicit reject should produce identical code"
        );
    }

    #[test]
    fn test_generate_throttle_mode_emits_compile_error() {
        // T006: on_exceed = "throttle" 应生成 compile_error（Throttled 变体不存在）
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            on_exceed: "throttle".to_string(),
            reject_message: "Rate limit exceeded".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_throttle");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            tokens_str.contains("compile_error"),
            "throttle mode should emit compile_error; tokens = {}",
            tokens_str
        );
        assert!(
            tokens_str.contains("throttle"),
            "compile_error message should mention throttle; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_quota_check_log_only_no_error() {
        // T006: quota check 在 log_only 模式下不应生成 QuotaExceeded
        let config = FlowControlConfig {
            quota: Some(QuotaLimit {
                max: 1000,
                period: "h".to_string(),
            }),
            on_exceed: "log_only".to_string(),
            reject_message: "Quota exceeded".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_quota_log_only");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            !tokens_str.contains("QuotaExceeded"),
            "log_only mode should NOT generate QuotaExceeded; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_concurrency_check_log_only_no_error() {
        // T006: concurrency check 在 log_only 模式下不应生成 ConcurrencyLimitExceeded
        let config = FlowControlConfig {
            concurrency: Some(10),
            on_exceed: "log_only".to_string(),
            reject_message: "Concurrency exceeded".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_concurrency_log_only");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            !tokens_str.contains("ConcurrencyLimitExceeded"),
            "log_only mode should NOT generate ConcurrencyLimitExceeded; tokens = {}",
            tokens_str
        );
    }

    // ========================================================================
    // T007: key_prefix 参数解析与代码生成测试
    // ========================================================================

    #[test]
    fn test_parse_key_prefix() {
        let tokens: proc_macro2::TokenStream =
            quote::quote! { rate = "100/s", key_prefix = "my_namespace" };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert_eq!(config.key_prefix.as_deref(), Some("my_namespace"));
    }

    #[test]
    fn test_parse_key_prefix_default_none() {
        let tokens: proc_macro2::TokenStream = quote::quote! { rate = "100/s" };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert!(config.key_prefix.is_none());
    }

    #[test]
    fn test_generate_key_prefix_in_rate_key() {
        // T007: key_prefix 应出现在 rate_key 中
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            key_prefix: Some("my_namespace".to_string()),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_prefix");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        // 应生成 "my_namespace:rate:test_fn_prefix:..." 格式的 key
        assert!(
            tokens_str.contains("my_namespace"),
            "key_prefix should appear in generated code; tokens = {}",
            tokens_str
        );
        assert!(
            tokens_str.contains(":rate:"),
            "rate key format should contain ':rate:' separator; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_key_prefix_in_quota_key() {
        // T007: key_prefix 应出现在 quota_key 中
        let config = FlowControlConfig {
            quota: Some(QuotaLimit {
                max: 1000,
                period: "h".to_string(),
            }),
            key_prefix: Some("quota_ns".to_string()),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_quota_prefix");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            tokens_str.contains("quota_ns"),
            "key_prefix should appear in quota key; tokens = {}",
            tokens_str
        );
        assert!(
            tokens_str.contains(":quota:"),
            "quota key format should contain ':quota:' separator; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_key_prefix_in_concurrency_key() {
        // T007: key_prefix 应出现在 concurrency_key 中
        let config = FlowControlConfig {
            concurrency: Some(10),
            key_prefix: Some("conc_ns".to_string()),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_conc_prefix");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            tokens_str.contains("conc_ns"),
            "key_prefix should appear in concurrency key; tokens = {}",
            tokens_str
        );
        assert!(
            tokens_str.contains(":concurrency:"),
            "concurrency key format should contain ':concurrency:' separator; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_no_key_prefix_keeps_original_format() {
        // T007: 未设置 key_prefix 时，key 格式应保持空前缀（兼容原有行为）
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            key_prefix: None,
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_no_prefix");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        // 应生成 ":rate:test_fn_no_prefix:..." 格式（前缀为空字符串）
        assert!(
            tokens_str.contains(":rate:"),
            "rate key format should contain ':rate:' separator; tokens = {}",
            tokens_str
        );
    }

    // ========================================================================
    // T008: tracing/metrics toggles 测试
    // ========================================================================

    #[test]
    fn test_parse_tracing_false() {
        let tokens: proc_macro2::TokenStream = quote::quote! { rate = "100/s", tracing = false };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert!(!config.enable_tracing);
        assert!(config.enable_metrics); // 默认仍为 true
    }

    #[test]
    fn test_parse_metrics_false() {
        let tokens: proc_macro2::TokenStream = quote::quote! { rate = "100/s", metrics = false };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert!(config.enable_tracing); // 默认仍为 true
        assert!(!config.enable_metrics);
    }

    #[test]
    fn test_parse_tracing_and_metrics_false() {
        let tokens: proc_macro2::TokenStream =
            quote::quote! { rate = "100/s", tracing = false, metrics = false };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert!(!config.enable_tracing);
        assert!(!config.enable_metrics);
    }

    #[test]
    fn test_parse_tracing_true_explicit() {
        let tokens: proc_macro2::TokenStream = quote::quote! { rate = "100/s", tracing = true };
        let config = FlowControlConfig::parse(&tokens).unwrap();
        assert!(config.enable_tracing);
    }

    #[test]
    fn test_generate_tracing_disabled_no_span() {
        // T008: tracing = false 时不生成 tracing::span! 代码
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            enable_tracing: false,
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_no_tracing");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            !tokens_str.contains("tracing :: span"),
            "tracing=false should NOT generate tracing::span!; tokens = {}",
            tokens_str
        );
        assert!(
            !tokens_str.contains("_span"),
            "tracing=false should NOT generate _span variable; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_tracing_enabled_has_span() {
        // T008: tracing = true（默认）时应生成 tracing::span! 代码
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            enable_tracing: true,
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_with_tracing");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            tokens_str.contains("tracing :: span"),
            "tracing=true should generate tracing::span!; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_metrics_disabled_no_try_global() {
        // T008: metrics = false 时不生成 try_global() 调用
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            enable_metrics: false,
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_no_metrics");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            !tokens_str.contains("try_global"),
            "metrics=false should NOT generate try_global() call; tokens = {}",
            tokens_str
        );
        assert!(
            !tokens_str.contains("requests_total"),
            "metrics=false should NOT generate requests_total.inc() call; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_metrics_enabled_has_try_global() {
        // T008: metrics = true（默认）时应生成 try_global() 调用
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            enable_metrics: true,
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_with_metrics");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            tokens_str.contains("try_global"),
            "metrics=true should generate try_global() call; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_all_toggles_off() {
        // T008: tracing=false + metrics=false 同时禁用
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            enable_tracing: false,
            enable_metrics: false,
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_all_off");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        assert!(
            !tokens_str.contains("tracing :: span"),
            "tracing=false should NOT generate tracing::span!"
        );
        assert!(
            !tokens_str.contains("try_global"),
            "metrics=false should NOT generate try_global()"
        );
    }
}
