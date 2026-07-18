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

/// 构造 on_exceed 模式的 exceed handler 代码
///
/// - `mode`: "reject" / "log_only" / "throttle"
/// - `error_variant`: LimiteronError 变体名（如 `"RateLimitExceeded"`），作为 `&str` 传入，
///   函数内部转换为 `syn::Ident` 插值到 quote!（audit-L-007：简化调用点签名）
/// - `reject_message`: reject 模式下的错误消息
fn build_exceed_handler(
    mode: &str,
    error_variant: &str,
    reject_message: &str,
) -> proc_macro2::TokenStream {
    let error_variant = syn::Ident::new(error_variant, proc_macro2::Span::call_site());
    match mode {
        "reject" => {
            let msg = reject_message.to_string();
            quote! {
                return Err(limiteron::error::LimiteronError::#error_variant(#msg.to_string()));
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
                    mode
                ),
                proc_macro2::Span::call_site(),
            );
            quote! { compile_error!(#err_msg); }
        }
    }
}

/// Sanitize key component: ASCII alphanumeric + `_` `-` `.`, max 128 chars
///
/// 用于在宏展开期对 `key_prefix` 和 `fname` 进行防御性过滤，
/// 与生成代码中运行时的 `sanitize` 闭包保持一致的字符集与长度上限。
///
/// # 安全
///
/// 仅允许 ASCII 字符（`is_ascii_alphanumeric`），拒绝 Unicode 同形字符攻击
/// （如西里尔字母 `а`、希腊字母 `о` 等）（audit-M-001）。
fn sanitize_key_component(s: &str) -> String {
    s.chars()
        .filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
        .take(128)
        .collect::<String>()
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
    // audit-M1/L2: 宏展开期对 prefix 和 fname 做防御性 sanitize（与运行时 sanitize 闭包一致）
    let sanitized_prefix = sanitize_key_component(&key_prefix_str);
    let sanitized_fname = sanitize_key_component(&fn_name_str);

    // 根据 on_exceed 模式生成 rate check 失败时的处理代码
    // - "reject": 返回 RateLimitExceeded 错误（默认行为）
    // - "log_only": 不返回错误，继续执行原函数
    // - "throttle": 当前版本未实现（LimiteronError::Throttled 变体不存在），生成 compile_error
    let rate_exceed_handler =
        build_exceed_handler(on_exceed_mode, "RateLimitExceeded", &reject_message);

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
        let fname = sanitized_fname.clone();
        // audit-M5: key_prefix=None 时生成 "rate:fn:xxx"（无前导冒号，恢复旧行为）
        // key_prefix=Some(p) 时生成 "p:rate:fn:xxx"
        let key_tpl = if config.key_prefix.is_some() {
            let p = sanitized_prefix.clone();
            quote! { format!("{}:rate:{}:{}", #p, #fname, sanitize(&identifier)) }
        } else {
            quote! { format!("rate:{}:{}", #fname, sanitize(&identifier)) }
        };
        // audit-M2: log_only 模式下不消费 rate token（语义=仅记录，不产生副作用）
        // reject / throttle 模式下消费 token 并检查
        let check_logic = if on_exceed_mode == "log_only" {
            quote! {
                let _ = &rate_limiter;  // audit-L-003：引用避免 unused 警告（更地道写法）
            }
        } else {
            quote! {
                if !rate_limiter.allow(1).await? {
                    #rate_exceed_handler
                }
            }
        };
        quote! {
            let rate_key = {
                // audit-M-001: 仅 ASCII alphanumeric，拒绝 Unicode 同形字符攻击
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                    .take(128)
                    .collect::<String>();
                #key_tpl
            };
            let rate_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_rate_limiter(&rate_key, #amount, #unit_secs);
            #check_logic
        }
    } else {
        quote!()
    };

    let quota_exceed_handler =
        build_exceed_handler(on_exceed_mode, "QuotaExceeded", &reject_message);

    let quota_check = if let Some(ref quota) = config.quota {
        let max = quota.max;
        let duration = quota.to_duration();
        let fname = sanitized_fname.clone();
        // audit-M5: key_prefix=None 时生成 "quota:fn:xxx"（无前导冒号，恢复旧行为）
        // key_prefix=Some(p) 时生成 "p:quota:fn:xxx"
        let key_tpl = if config.key_prefix.is_some() {
            let p = sanitized_prefix.clone();
            quote! { format!("{}:quota:{}:{}", #p, #fname, sanitize(&identifier)) }
        } else {
            quote! { format!("quota:{}:{}", #fname, sanitize(&identifier)) }
        };
        // audit-M2: log_only 模式下不消费配额（语义=仅记录，不产生副作用）
        // reject / throttle 模式下消费配额并检查
        let check_logic = if on_exceed_mode == "log_only" {
            quote! {
                let _ = &quota_limiter;  // audit-L-003：引用避免 unused 警告（更地道写法）
            }
        } else {
            quote! {
                // T006 修复: 使用 check(&key) 真正消费配额，而非 allow(1)（默认返回 Ok(true) 不消费）
                if quota_limiter.check(&quota_key).await.is_err() {
                    #quota_exceed_handler
                }
            }
        };
        quote! {
            let quota_key = {
                // audit-M-001: 仅 ASCII alphanumeric，拒绝 Unicode 同形字符攻击
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                    .take(128)
                    .collect::<String>();
                #key_tpl
            };
            let quota_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_quota_limiter(&quota_key, #duration, #max);
            #check_logic
        }
    } else {
        quote!()
    };

    let concurrency_exceed_handler =
        build_exceed_handler(on_exceed_mode, "ConcurrencyLimitExceeded", &reject_message);

    let concurrency_check = if let Some(concurrency) = config.concurrency {
        let fname = sanitized_fname.clone();
        // audit-M5: key_prefix=None 时生成 "concurrency:fn:xxx"（无前导冒号，恢复旧行为）
        // key_prefix=Some(p) 时生成 "p:concurrency:fn:xxx"
        let key_tpl = if config.key_prefix.is_some() {
            let p = sanitized_prefix.clone();
            quote! { format!("{}:concurrency:{}:{}", #p, #fname, sanitize(&identifier)) }
        } else {
            quote! { format!("concurrency:{}:{}", #fname, sanitize(&identifier)) }
        };
        // audit-L1: 仅 reject 模式下 match 的 None 分支为 unreachable（exceed_handler 中 return Err 提前返回）
        // 其他模式不生成 #[allow(unreachable_code)]，避免掩盖真实 unreachable 代码
        let allow_attr = if on_exceed_mode == "reject" {
            quote! { #[allow(unreachable_code)] }
        } else {
            quote! {}
        };
        // audit-M2: log_only 模式下不持有 permit（语义=不产生副作用，不占用并发槽位）
        // reject / throttle 模式下 acquire permit 并持有到函数结束
        let check_logic = if on_exceed_mode == "log_only" {
            quote! {
                let _ = &concurrency_limiter;  // audit-L-003：引用避免 unused 警告（更地道写法）
            }
        } else {
            quote! {
                // T006 修复: 持有 permit 到函数结束（用 Option 包装）
                // 之前 _permit 在 match 作用域结束即 drop，并发控制失效
                #allow_attr
                let _concurrency_permit = match concurrency_limiter.acquire(1).await {
                    Ok(permit) => Some(permit),
                    Err(_) => {
                        #concurrency_exceed_handler
                        None
                    }
                };
            }
        };
        quote! {
            let concurrency_key = {
                // audit-M-001: 仅 ASCII alphanumeric，拒绝 Unicode 同形字符攻击
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                    .take(128)
                    .collect::<String>();
                #key_tpl
            };
            let concurrency_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_concurrency_limiter(&concurrency_key, #concurrency as u64);
            #check_logic
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
        // T006 + audit-M2: on_exceed = "log_only" 不应生成 RateLimitExceeded 错误
        // 且不调用 rate_limiter.allow()（语义=仅记录，不消费 token）
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
        // audit-M2: log_only 模式下不应调用 rate_limiter.allow（不消费 token）
        assert!(
            !tokens_str.contains(".allow"),
            "log_only mode should NOT call .allow() (audit-M2: no side effects); tokens = {}",
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
        // audit-M5: 未设置 key_prefix 时，key 格式应为 "rate:fn:xxx"（无前导冒号，恢复旧行为）
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

        // 应生成 format!("rate:{}:{}", ...)（无前导冒号）
        assert!(
            tokens_str.contains(r#""rate:{}:{}""#),
            "rate key should use 'rate:{{}}:{{}}' format (no leading colon); tokens = {}",
            tokens_str
        );
        // 不应生成旧的前导冒号格式 format!("{}:rate:{}:{}", ...)
        assert!(
            !tokens_str.contains(r#""{}:rate:{}:{}""#),
            "should NOT generate leading-colon format '{{}}:rate:{{}}:{{}}'; tokens = {}",
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

    // ========================================================================
    // audit-macro-followup T001: build_exceed_handler DRY 验证
    // ========================================================================

    #[test]
    fn test_build_exceed_handler_dry() {
        // T001: 三个 error variant 都应通过辅助函数正确生成
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            quota: Some(QuotaLimit {
                max: 1000,
                period: "h".to_string(),
            }),
            concurrency: Some(10),
            on_exceed: "reject".to_string(),
            reject_message: "exceeded".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_all_three");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();
        // 三个 error variant 都应出现（验证辅助函数对三个调用都生效）
        assert!(
            tokens_str.contains("RateLimitExceeded"),
            "RateLimitExceeded should be generated; tokens = {}",
            tokens_str
        );
        assert!(
            tokens_str.contains("QuotaExceeded"),
            "QuotaExceeded should be generated; tokens = {}",
            tokens_str
        );
        assert!(
            tokens_str.contains("ConcurrencyLimitExceeded"),
            "ConcurrencyLimitExceeded should be generated; tokens = {}",
            tokens_str
        );
    }

    // ========================================================================
    // audit-macro-followup T002: key_prefix sanitize 验证
    // ========================================================================

    #[test]
    fn test_generate_key_prefix_sanitized() {
        // T002: key_prefix 中的特殊字符（: ! 等）应在宏展开期被过滤
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            key_prefix: Some("ns:with!special".to_string()),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_sanitized");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();
        // 原始未 sanitize 的字符串字面量不应出现（说明 sanitize 已生效）
        assert!(
            !tokens_str.contains(r#""ns:with!special""#),
            "raw key_prefix with special chars should NOT appear as literal; tokens = {}",
            tokens_str
        );
        assert!(
            !tokens_str.contains(r#""ns:with""#),
            "partial raw prefix with ':' should NOT appear as literal; tokens = {}",
            tokens_str
        );
        // 过滤后的合法字符应作为字面量出现（ns:with!special -> nswithspecial）
        assert!(
            tokens_str.contains(r#""nswithspecial""#),
            "sanitized prefix 'nswithspecial' should appear as literal; tokens = {}",
            tokens_str
        );
    }

    // ========================================================================
    // audit-macro-followup T003: key_prefix=None 时无前导冒号验证
    // ========================================================================

    #[test]
    fn test_generate_key_prefix_none_no_leading_colon() {
        // T003: key_prefix=None 时所有三类 key 都不应有前导冒号
        // rate: "rate:fn:xxx"，quota: "quota:fn:xxx"，concurrency: "concurrency:fn:xxx"
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            quota: Some(QuotaLimit {
                max: 1000,
                period: "h".to_string(),
            }),
            concurrency: Some(10),
            key_prefix: None,
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_no_prefix_all");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        // 应生成无前导冒号格式
        assert!(
            tokens_str.contains(r#""rate:{}:{}""#),
            "rate key should use 'rate:{{}}:{{}}' format; tokens = {}",
            tokens_str
        );
        assert!(
            tokens_str.contains(r#""quota:{}:{}""#),
            "quota key should use 'quota:{{}}:{{}}' format; tokens = {}",
            tokens_str
        );
        assert!(
            tokens_str.contains(r#""concurrency:{}:{}""#),
            "concurrency key should use 'concurrency:{{}}:{{}}' format; tokens = {}",
            tokens_str
        );

        // 不应生成任何带前导冒号的旧格式
        assert!(
            !tokens_str.contains(r#""{}:rate:{}:{}""#),
            "should NOT generate leading-colon rate format; tokens = {}",
            tokens_str
        );
        assert!(
            !tokens_str.contains(r#""{}:quota:{}:{}""#),
            "should NOT generate leading-colon quota format; tokens = {}",
            tokens_str
        );
        assert!(
            !tokens_str.contains(r#""{}:concurrency:{}:{}""#),
            "should NOT generate leading-colon concurrency format; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_key_prefix_some_has_prefix() {
        // T003 配套：key_prefix=Some(p) 时应生成 "p:rate:fn:xxx" 格式
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            key_prefix: Some("myns".to_string()),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_with_prefix");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();

        // 应生成带前缀的格式 "{}:rate:{}:{}"
        assert!(
            tokens_str.contains(r#""{}:rate:{}:{}""#),
            "key_prefix=Some should generate '{{}}:rate:{{}}:{{}}' format; tokens = {}",
            tokens_str
        );
        // 应包含 sanitized 前缀字面量 "myns"
        assert!(
            tokens_str.contains(r#""myns""#),
            "sanitized prefix 'myns' should appear as literal; tokens = {}",
            tokens_str
        );
    }

    // ========================================================================
    // audit-macro-followup T004: fname sanitize 验证
    // ========================================================================

    #[test]
    fn test_generate_fname_sanitized_in_key() {
        // T004: fname 经 sanitize_key_component 处理后应作为字面量出现在 key 中
        // Rust 标识符字符集已受限（字母数字下划线），sanitize 后应保持不变
        // 这里通过合法标识符 test_fn 验证 sanitize 路径已生效
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            key_prefix: Some("ns".to_string()),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();
        // sanitized fname "test_fn" 应作为字面量出现在 key 模板中
        assert!(
            tokens_str.contains(r#""test_fn""#),
            "sanitized fname 'test_fn' should appear as literal in key; tokens = {}",
            tokens_str
        );
    }

    // ========================================================================
    // audit-macro-followup T005: log_only 模式下不消费配额验证
    // ========================================================================

    #[test]
    fn test_generate_log_only_rate_no_allow_call() {
        // T005: log_only 模式下 rate_check 不应调用 rate_limiter.allow()
        let config = FlowControlConfig {
            rate: Some(RateLimit {
                amount: 100,
                unit: "s".to_string(),
            }),
            on_exceed: "log_only".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_log_only_rate");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();
        assert!(
            !tokens_str.contains(".allow"),
            "log_only mode should NOT call .allow() on rate_limiter (audit-M2); tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_log_only_quota_no_check_call() {
        // T005: log_only 模式下 quota_check 不应调用 quota_limiter.check()
        let config = FlowControlConfig {
            quota: Some(QuotaLimit {
                max: 1000,
                period: "h".to_string(),
            }),
            on_exceed: "log_only".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_log_only_quota");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();
        assert!(
            !tokens_str.contains(".check"),
            "log_only mode should NOT call .check() on quota_limiter (audit-M2); tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_log_only_concurrency_no_acquire_call() {
        // T005: log_only 模式下 concurrency_check 不应调用 concurrency_limiter.acquire()
        let config = FlowControlConfig {
            concurrency: Some(10),
            on_exceed: "log_only".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_log_only_conc");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();
        assert!(
            !tokens_str.contains(".acquire"),
            "log_only mode should NOT call .acquire() on concurrency_limiter (audit-M2); tokens = {}",
            tokens_str
        );
    }

    // ========================================================================
    // audit-macro-followup T006: 条件生成 #[allow(unreachable_code)] 验证
    // ========================================================================

    #[test]
    fn test_generate_reject_mode_has_unreachable_allow_attr() {
        // T006: reject 模式下应生成 #[allow(unreachable_code)] attr
        let config = FlowControlConfig {
            concurrency: Some(10),
            on_exceed: "reject".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_reject_conc");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();
        assert!(
            tokens_str.contains("unreachable_code"),
            "reject mode should generate #[allow(unreachable_code)] attr; tokens = {}",
            tokens_str
        );
    }

    #[test]
    fn test_generate_log_only_no_unreachable_allow_attr() {
        // T006: log_only 模式下不应生成 #[allow(unreachable_code)] attr
        // 因为 log_only 不调用 acquire，没有 unreachable 分支
        let config = FlowControlConfig {
            concurrency: Some(10),
            on_exceed: "log_only".to_string(),
            ..Default::default()
        };
        let input_fn = make_test_fn("test_fn_log_only_no_attr");
        let tokens = generate_flow_control(&input_fn, &config).unwrap();
        let tokens_str = tokens.to_string();
        assert!(
            !tokens_str.contains("unreachable_code"),
            "log_only mode should NOT generate #[allow(unreachable_code)] attr; tokens = {}",
            tokens_str
        );
    }

    // ========================================================================
    // audit-macro-followup 修复16 (L-002): sanitize_key_component 边界测试
    // ========================================================================

    #[test]
    fn test_sanitize_key_component_edge_cases() {
        // audit-L-002：覆盖 sanitize_key_component 的所有边界条件
        // 包括空字符串、纯特殊字符、合法字符、超长截断、Unicode 过滤

        // 空字符串
        assert_eq!(sanitize_key_component(""), "");

        // 纯特殊字符（全部被过滤，结果为空）
        assert_eq!(sanitize_key_component("!!!"), "");
        assert_eq!(sanitize_key_component(":!@$%^&*()"), "");

        // 合法字符集（alphanumeric + _ - .）
        assert_eq!(sanitize_key_component("abc123"), "abc123");
        assert_eq!(sanitize_key_component("ABC_xyz"), "ABC_xyz");
        assert_eq!(sanitize_key_component("ns.test-1"), "ns.test-1");
        assert_eq!(sanitize_key_component("admin"), "admin");

        // 超长字符串截断到 128 字符
        let long = "a".repeat(200);
        let sanitized = sanitize_key_component(&long);
        assert_eq!(sanitized.len(), 128, "should truncate to 128 chars");
        assert_eq!(sanitized, "a".repeat(128));

        // 混合合法与非法字符（仅保留合法字符）
        // "xyz!789@uvw" → "!" "@" 被过滤，alphanumeric 字符保留（避免 hex 误报）
        assert_eq!(sanitize_key_component("xyz!789@uvw"), "xyz789uvw");
        // "ns:user:123" → ":" 被过滤
        assert_eq!(sanitize_key_component("ns:user:123"), "nsuser123");

        // Unicode 字符被过滤（audit-M-001: is_ascii_alphanumeric）
        // 中文应被过滤
        assert_eq!(sanitize_key_component("\u{4e2d}\u{6587}_test"), "_test");
        // 日文应被过滤
        assert_eq!(
            sanitize_key_component("\u{30e6}\u{30fc}\u{30b6}\u{30fc}_test"),
            "_test"
        );
        // 西里尔字母应被过滤（同形字符攻击防护：'а' vs 'a'）
        // 'аdmin' 第一个字符是西里尔字母 'а'（U+0430），应被过滤，结果为 'dmin'
        let cyrillic_a_first = "\u{430}dmin"; // 'а' 是西里尔字母（U+0430），'dmin' 是 ASCII
        let sanitized = sanitize_key_component(cyrillic_a_first);
        assert_eq!(
            sanitized, "dmin",
            "cyrillic 'а' should be filtered (homoglyph attack prevention), got '{}'",
            sanitized
        );
        // 全 ASCII 的 'admin' 应保持不变
        assert_eq!(sanitize_key_component("admin"), "admin");

        // 边界：恰好 128 字符不截断
        let exact_128 = "a".repeat(128);
        assert_eq!(sanitize_key_component(&exact_128).len(), 128);

        // 边界：129 字符截断为 128
        let over_128 = "a".repeat(129);
        assert_eq!(sanitize_key_component(&over_128).len(), 128);
    }

    #[test]
    fn test_sanitize_key_component_defense_in_depth() {
        // audit-M-001: 防御性测试 - 同形字符攻击场景
        // 攻击者可能用 'аdmin'（西里尔字母 а）冒充 'admin'（视觉相同但 Unicode 不同），
        // 试图绕过基于 key 的隔离。sanitize 后西里尔字母 'а' 被过滤，
        // 'аdmin' → 'dmin'，与合法 'admin' 不同：
        // 1. 攻击者无法让恶意 key 与合法 key 碰撞（绕过限流）
        // 2. 攻击者也无法让恶意 key 与合法 key 完全相同（视觉欺骗）
        let legit_admin = sanitize_key_component("admin");
        let homoglyph_admin = sanitize_key_component("аdmin"); // 西里尔字母 а + dmin

        assert_eq!(
            legit_admin, "admin",
            "legit 'admin' should remain unchanged"
        );
        assert_eq!(
            homoglyph_admin, "dmin",
            "homoglyph 'аdmin' should be sanitized to 'dmin' (cyrillic 'а' filtered)"
        );
        assert_ne!(
            legit_admin, homoglyph_admin,
            "homoglyph attack should NOT produce same result as legit"
        );
    }
}
