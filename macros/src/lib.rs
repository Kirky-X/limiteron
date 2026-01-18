//! Flowguard 过程宏
//!
//! 提供声明式的流量控制宏，简化限流配置。

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, ItemFn};

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
#[derive(Debug, Clone, Default)]
struct FlowControlConfig {
    rate: Option<RateLimit>,
    quota: Option<QuotaLimit>,
    concurrency: Option<u32>,
    identifiers: Vec<String>,
    on_exceed: String,
    reject_message: String,
}

impl FlowControlConfig {
    fn parse(tokens: &proc_macro2::TokenStream) -> Result<Self, String> {
        use syn::parse::Parser;
        use syn::punctuated::Punctuated;
        use syn::Token;

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

                    match ident_str.as_str() {
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
        if !["s", "m", "h"].contains(&unit.as_str()) {
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
            _ => quote!(std::time::Duration::from_secs(1)),
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
        if !["s", "m", "h", "d"].contains(&period.as_str()) {
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
            _ => quote!(std::time::Duration::from_secs(1)),
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

    let rate_check = if let Some(ref rate) = config.rate {
        let amount = rate.amount;
        let msg = reject_message.clone();
        let fn_name_str = stringify!(#fn_name).to_string();
        quote! {
            let rate_key = {
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
                    .take(128)
                    .collect::<String>();
                format!("rate:{}:{}", #fn_name_str, sanitize(&identifier))
            };
            let rate_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_rate_limiter(&rate_key, #amount, 1);
            if !rate_limiter.allow(1).await? {
                return Err(limiteron::error::FlowGuardError::RateLimitExceeded(#msg.to_string()));
            }
        }
    } else {
        quote!()
    };

    let quota_check = if let Some(ref quota) = config.quota {
        let max = quota.max;
        let duration = quota.to_duration();
        let msg = reject_message.clone();
        let fn_name_str = stringify!(#fn_name).to_string();
        quote! {
            let quota_key = {
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
                    .take(128)
                    .collect::<String>();
                format!("quota:{}:{}", #fn_name_str, sanitize(&identifier))
            };
            let quota_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_quota_limiter(&quota_key, #duration, #max);
            if !quota_limiter.allow(1).await? {
                return Err(limiteron::error::FlowGuardError::QuotaExceeded(#msg.to_string()));
            }
        }
    } else {
        quote!()
    };

    let concurrency_check = if let Some(concurrency) = config.concurrency {
        let msg = reject_message.clone();
        let fn_name_str = stringify!(#fn_name).to_string();
        quote! {
            let concurrency_key = {
                let sanitize = |s: &str| s
                    .chars()
                    .filter(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
                    .take(128)
                    .collect::<String>();
                format!("concurrency:{}:{}", #fn_name_str, sanitize(&identifier))
            };
            let concurrency_limiter = limiteron::GLOBAL_LIMITER_MANAGER.get_concurrency_limiter(&concurrency_key, #concurrency as u64);
            let _permit = concurrency_limiter.acquire(1).await.map_err(|_| limiteron::error::FlowGuardError::ConcurrencyLimitExceeded(#msg.to_string()))?;
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

    let tracing_start = quote! {
        let _span = tracing::span!(tracing::Level::INFO, "flow_control", function = stringify!(#fn_name));
        let _enter = _span.enter();
    };

    let metrics_record = quote! {
        if let Some(metrics) = limiteron::telemetry::try_global() {
            metrics.requests_total.inc();
        }
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
        // 注意：#[derive(Default)] 会将 String 字段默认为空字符串
        assert_eq!(config.on_exceed, "");
        assert_eq!(config.reject_message, "");
    }
}
