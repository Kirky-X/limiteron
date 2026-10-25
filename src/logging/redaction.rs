// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 日志脱敏模块
//!
//! 提供日志脱敏功能，保护敏感信息不被泄露到日志中。
//! 即使没有启用 log-redaction feature，基础脱敏函数也可用。

/// 基础脱敏函数 - 即使没有启用 log-redaction feature 也可用
#[inline]
pub fn redact_basic(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    // 按字符（而非字节）计长与切片：多字节 UTF-8（中文/emoji）按字节
    // 切片会落在字符内部，触发 str 切片 panic。
    let char_count = value.chars().count();
    if char_count < 4 {
        return "***".to_string();
    }

    // 前缀/后缀按字符边界取字节偏移后直接切片，避免中间 String 分配
    let prefix_end = value.char_indices().nth(2).map_or(value.len(), |(i, _)| i);
    let suffix_start = value
        .char_indices()
        .nth(char_count - 2)
        .map_or(0, |(i, _)| i);
    format!("{}***{}", &value[..prefix_end], &value[suffix_start..])
}

/// 用户ID脱敏 - 即使没有启用 log-redaction feature 也可用
#[inline]
pub fn redact_user_id(value: Option<&str>) -> String {
    redact_basic(value)
}

/// IP地址脱敏 - 即使没有启用 log-redaction feature 也可用
#[inline]
pub fn redact_ip(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    // 如果是IP地址，保留前两段
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() == 4 {
        return format!("{}.{}.***.***", parts[0], parts[1]);
    }

    // IPv6简化处理
    if value.contains(':') {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() >= 2 {
            return format!("{}:***:***", parts[0]);
        }
    }

    redact_basic(Some(value))
}

/// 邮箱脱敏 - 即使没有启用 log-redaction feature 也可用
#[inline]
pub fn redact_email(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    if let Some(at_pos) = value.find('@') {
        // '@' 为 ASCII 字节，find 返回的偏移必为字符边界
        let local_part = &value[..at_pos];
        let domain = &value[at_pos..];

        // 本地部分按字符计长与切片（多字节 UTF-8 按字节切片会 panic）
        let local_chars = local_part.chars().count();
        if local_chars <= 2 {
            return format!("***{}", domain);
        }

        let prefix_len = if local_chars <= 4 { 1 } else { 2 };
        let prefix_end = local_part
            .char_indices()
            .nth(prefix_len)
            .map_or(local_part.len(), |(i, _)| i);
        return format!("{}***{}", &local_part[..prefix_end], domain);
    }

    redact_basic(Some(value))
}

/// URL 脱敏：只保留 scheme 与 host，丢弃 path / query / fragment
///
/// Webhook 目标的 path/query 可能携带令牌（如带签名的回调 URL），整条
/// URL 入日志等于把令牌写进日志文件。仅保留 scheme 与 host 足以定位
/// 「是哪个端点出问题」，又不会外泄凭据。
///
/// 无法解析出 scheme://host 时退回基础脱敏（不整条回显）。
pub(crate) fn redact_url(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    let Some((scheme, rest)) = value.split_once("://") else {
        return redact_basic(Some(value));
    };

    // authority 结束于 path / query / fragment 的分隔符
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return redact_basic(Some(value));
    }

    // userinfo（user:pass@host）同样属凭据，不入日志
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);

    format!("{scheme}://{host}")
}

#[cfg(feature = "log-redaction")]
use inklog::{DataMasker, MaskRule, MaskRuleRegistry};
#[cfg(feature = "log-redaction")]
use std::sync::LazyLock;

/// 进程级脱敏引擎：inklog `DataMasker`（21 条内置规则 + limiteron
/// 键值对补充规则），不可变且可跨线程共享
#[cfg(feature = "log-redaction")]
static DATA_MASKER: LazyLock<DataMasker> = LazyLock::new(build_data_masker);

/// limiteron 公开契约钉死的脱敏补充规则（priority 90，先于内置值
/// 模式规则执行）
///
/// inklog 内置 `generic_secret`/`api_key` 规则对短值（分别 <16/<20
/// 字符）不命中，且 `authorization` 不在其键名集合内；`bank_card`/
/// `credit_card` 仅匹配连续数字、不匹配分隔符形态的卡号。以下规则按
/// 原手写正则的契约补齐：
/// - 六类键值对（URL `k=v`、YAML/plain `k: v`、JSON `"k":"v"`），
///   整段匹配替换为 `***`（与原引擎输出一致）；
/// - 分隔符形态信用卡号（`1234-5678-9012-3456`，原 `credit_card`
///   值模式语义；连续数字形态交由内置 `bank_card` 部分掩码）。
#[cfg(feature = "log-redaction")]
const KV_MASK_RULES: [(&str, &str); 7] = [
    (
        "kv_password",
        r#"(?i)("?password"?\s*[:=]\s*"?\s*)([^"\s,\}]+)"#,
    ),
    (
        "kv_api_key",
        r#"(?i)("?api[_-]?key"?\s*[:=]\s*"?\s*)([^"\s,\}]+)"#,
    ),
    ("kv_token", r#"(?i)("?token"?\s*[:=]\s*"?\s*)([^"\s,\}]+)"#),
    (
        "kv_secret",
        r#"(?i)("?secret"?\s*[:=]\s*"?\s*)([^"\s,\}]+)"#,
    ),
    (
        "kv_credential",
        r#"(?i)("?credential"?\s*[:=]\s*"?\s*)([^"\s,\}]+)"#,
    ),
    (
        "kv_authorization",
        r#"(?i)("?authorization"?\s*[:=]\s*"?\s*)([^"\s,\}]+)"#,
    ),
    (
        "credit_card_separated",
        r"(?<![0-9])\d{4}[-\s]\d{4}[-\s]\d{4}[-\s]\d{4}(?![0-9])",
    ),
];

/// 组装进程级 `DataMasker`：`MaskRuleRegistry::with_builtins()`（21 条
/// 内置规则）+ limiteron 六类键值对补充规则
#[cfg(feature = "log-redaction")]
fn build_data_masker() -> DataMasker {
    let mut registry = MaskRuleRegistry::with_builtins();
    for (name, pattern) in KV_MASK_RULES {
        let rule = MaskRule::builder(name)
            .pattern(pattern)
            .replacement("***")
            .priority(90)
            .build()
            .unwrap_or_else(|e| panic!("invalid limiteron masking rule {name}: {e}"));
        registry
            .register(rule)
            .expect("limiteron kv rule name collision with builtins");
    }
    DataMasker::builder().with_registry(registry).build()
}

/// 判断字段名是否为敏感字段（词边界匹配，修复误脱敏）
///
/// 将字段名切分为词元（按 `_`/`-`/`.`/空格等分隔符与 camelCase 边界），
/// 命中规则：任一词元等于敏感关键词，或以高危关键词为前缀
/// （`keychain` → 前缀 `key`；`tokenizer` → 前缀 `token`，保守方向）。
/// `monkey` 之类仅含关键词子串的无关字段不再误伤。
///
/// 零分配实现：词元用 `&str` 切片迭代，关键词匹配用 `matches!`。
#[cfg(feature = "log-redaction")]
fn is_sensitive_field_name(field_name: &str) -> bool {
    /// 高危前缀：以这些词开头的词元按敏感处理（`keychain`/`tokenizer`）
    const SENSITIVE_PREFIXES: [&str; 3] = ["key", "secret", "token"];
    /// 敏感关键词（词元大小写不敏感精确匹配）
    const SENSITIVE_WORDS: [&str; 7] = [
        "password",
        "passwd",
        "secret",
        "token",
        "key",
        "credential",
        "authorization",
    ];

    split_field_tokens(field_name).any(|token| {
        let exact = SENSITIVE_WORDS
            .iter()
            .any(|&word| token.eq_ignore_ascii_case(word));
        let prefixed = SENSITIVE_PREFIXES
            .iter()
            .any(|&prefix| starts_with_ignore_ascii_case(token, prefix));
        exact || prefixed
    })
}

/// 词元是否以给定 ASCII 前缀开头（按字符逐个比较）
///
/// 词元可含多字节 UTF-8（CJK 汉字等字母类字符的 `char::is_alphanumeric` 为真；
/// emoji 属符号类、被 `split_field_tokens` 视为分隔符），定长字节切片会落在字符内部触发 panic，故此处按字符比较。
#[cfg(feature = "log-redaction")]
fn starts_with_ignore_ascii_case(token: &str, prefix: &str) -> bool {
    let mut token_chars = token.chars();
    let mut prefix_chars = prefix.chars();
    loop {
        match (prefix_chars.next(), token_chars.next()) {
            (None, _) => return true,
            (Some(p), Some(t)) => {
                if !p.eq_ignore_ascii_case(&t) {
                    return false;
                }
            }
            (Some(_), None) => return false,
        }
    }
}

/// 将字段名切分为小写词元切片（零分配：借用原串的子切片）
#[cfg(feature = "log-redaction")]
fn split_field_tokens(name: &str) -> impl Iterator<Item = &str> {
    let mut indices = name.char_indices().peekable();
    let mut start: Option<usize> = None;
    let mut prev_was_lowercase = false;
    let mut done = false;

    std::iter::from_fn(move || {
        if done {
            return None;
        }
        loop {
            let (idx, ch) = match indices.next() {
                Some(pair) => pair,
                None => {
                    done = true;
                    let s = start.take()?;
                    return Some(&name[s..]);
                }
            };
            let is_sep = !ch.is_alphanumeric();
            let camel_break = start.is_some() && ch.is_uppercase() && prev_was_lowercase;
            if is_sep {
                // 分隔符：`_`/`-`/`.`/空格等
                if let Some(s) = start.take() {
                    return Some(&name[s..idx]);
                }
                prev_was_lowercase = false;
                continue;
            }
            if camel_break {
                // camelCase 边界：`myToken` → `my` + `Token`
                let s = start.take().unwrap();
                start = Some(idx);
                prev_was_lowercase = ch.is_lowercase();
                return Some(&name[s..idx]);
            }
            if start.is_none() {
                start = Some(idx);
            }
            prev_was_lowercase = ch.is_lowercase();
        }
    })
}

/// 增强版脱敏函数 - 需要 log-redaction feature
///
/// 语义（design D2）：① 已知敏感字段名整段脱敏（`is_sensitive_field_name`
/// 词边界门控，入口前置）；② 短值（≤4 字符）整段脱敏；③ 其余值经 inklog
/// `DataMasker` 掩码——命中规则的值被掩码，非敏感值原样返回（除命中
/// 规则外无副作用改写）。
#[cfg(feature = "log-redaction")]
#[inline]
pub fn redact_advanced(value: Option<&str>, field_name: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    // 检查是否是已知的敏感字段
    //
    // 词边界匹配（修复误脱敏）：将字段名按分隔符（`_`/`-`/`.`/空格）
    // 与 camelCase 边界切分为词元，仅当任一词元命中敏感关键词（或以
    // 高危词元开头，如 `keychain` → `key`）时才整体脱敏。inklog 的
    // `is_sensitive_field` 为词边界正则，`keychain`/`tokenizer` 不判
    // 敏感，与既有契约冲突，故保留本纯函数作入口前置判断。
    if let Some(name) = field_name
        && is_sensitive_field_name(name)
    {
        return "***".to_string();
    }

    // 短值完全脱敏（与引擎无关的安全网）；阈值按字符计，多字节 UTF-8
    // 的字节数会越过 4 字符的真实长度
    if value.chars().count() <= 4 {
        return "***".to_string();
    }

    DATA_MASKER.mask(value)
}

/// 敏感信息检测 - 需要 log-redaction feature
///
/// 经 `DataMasker::detect` 检测面判定：任一规则（21 条内置 + limiteron
/// 六类键值对补充）命中即返回 true。与 `redact_advanced` 判定一致——
/// 能掩码的必能检出（R-log-redaction-002）。
#[cfg(feature = "log-redaction")]
#[inline]
pub fn contains_sensitive_info(value: &str) -> bool {
    !DATA_MASKER.detect(value).is_empty()
}

/// HTTP请求/响应脱敏 - 需要 log-redaction feature
///
/// 经 inklog `DataMasker` 掩码：六类键值对（password/token/authorization/
/// api_key/secret/credential，三格式）整段替换；内置值模式（email/phone/
/// id_card/bank_card/ip/mac/jwt/aws_key 等）原地掩码；非敏感内容原样保留。
#[cfg(feature = "log-redaction")]
#[inline]
pub fn redact_http_content(content: &str) -> String {
    DATA_MASKER.mask(content)
}

/// 批量脱敏结构体字段 - 需要 log-redaction feature
#[cfg(feature = "log-redaction")]
pub struct RedactionConfig<'a> {
    pub fields: Vec<(&'a str, bool)>, // (字段名, 是否为敏感字段)
}

#[cfg(feature = "log-redaction")]
impl<'a> RedactionConfig<'a> {
    /// 创建新的脱敏配置
    #[inline]
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// 添加要脱敏的字段
    #[inline]
    pub fn add_field(mut self, field_name: &'a str, is_sensitive: bool) -> Self {
        self.fields.push((field_name, is_sensitive));
        self
    }

    /// 构建脱敏后的字符串表示
    #[inline]
    pub fn format<F>(&self, get_field: F) -> String
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut parts = Vec::new();
        for (field_name, is_sensitive) in &self.fields {
            if let Some(value) = get_field(field_name) {
                let redacted_value = if *is_sensitive {
                    redact_advanced(Some(&value), Some(field_name))
                } else {
                    value
                };
                parts.push(format!("{}={}", field_name, redacted_value));
            }
        }
        format!("{{{}}}", parts.join(", "))
    }
}

#[cfg(feature = "log-redaction")]
impl<'a> Default for RedactionConfig<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// 脱敏 BanTarget - 可用于所有场景
#[inline]
#[cfg(feature = "ban-manager")]
pub fn redact_ban_target(target: &crate::storage::BanTarget) -> String {
    match target {
        crate::storage::BanTarget::Ip(ip) => redact_ip(Some(ip)),
        crate::storage::BanTarget::UserId(user_id) => redact_user_id(Some(user_id)),
        crate::storage::BanTarget::Mac(mac) => redact_basic(Some(mac)),
        crate::storage::BanTarget::Geo { country_code } => {
            format!("Geo({})", redact_basic(Some(country_code)))
        }
        // CIDR 网段本身是聚合信息（非个人标识），仅需基本长度脱敏
        crate::storage::BanTarget::Cidr(cidr) => format!("Cidr({})", redact_basic(Some(cidr))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_basic() {
        assert_eq!(redact_basic(None), "unknown");
        assert_eq!(redact_basic(Some("")), "unknown");
        assert_eq!(redact_basic(Some("   ")), "unknown");
        assert_eq!(redact_basic(Some("abc")), "***");
        assert_eq!(redact_basic(Some("user123")), "us***23");
        assert_eq!(redact_basic(Some("192.168.1.1")), "19***.1");
    }

    #[test]
    fn test_redact_basic_boundaries() {
        assert_eq!(redact_basic(Some("abcd")), "ab***cd");
        assert_eq!(redact_basic(Some("abcde")), "ab***de");
        assert_eq!(redact_basic(Some("ab")), "***");
        assert_eq!(redact_basic(Some("a")), "***");
    }

    /// URL 脱敏：path / query / fragment 不得外泄
    #[test]
    fn test_redact_url_drops_path_and_query() {
        assert_eq!(
            redact_url(Some("https://hooks.example.com/services/T00/B00/token")),
            "https://hooks.example.com"
        );
        assert_eq!(
            redact_url(Some("https://hooks.example.com/cb?token=secret")),
            "https://hooks.example.com"
        );
        assert_eq!(
            redact_url(Some("https://hooks.example.com:8443/path#frag")),
            "https://hooks.example.com:8443"
        );
        // userinfo 属凭据，一并剔除
        assert_eq!(
            redact_url(Some("https://user:pass@hooks.example.com/x")),
            "https://hooks.example.com"
        );
    }

    #[test]
    fn test_redact_url_fallbacks() {
        assert_eq!(redact_url(None), "unknown");
        // 非 URL 串退回基础脱敏，不整条回显
        let fallback = redact_url(Some("not-a-url-at-all"));
        assert!(
            !fallback.contains("not-a-url-at-all"),
            "a non-URL value must not be echoed in full, got: {fallback}"
        );
    }

    #[test]
    fn test_redact_user_id() {
        assert_eq!(redact_user_id(None), "unknown");
        assert_eq!(redact_user_id(Some("")), "unknown");
        assert_eq!(redact_user_id(Some("user123")), "us***23");
        assert_eq!(redact_user_id(Some("abc")), "***");
    }

    #[test]
    fn test_redact_ip() {
        assert_eq!(redact_ip(None), "unknown");
        assert_eq!(redact_ip(Some("192.168.1.1")), "192.168.***.***");
        assert_eq!(redact_ip(Some("::1")), ":***:***");
    }

    #[test]
    fn test_redact_ip_more() {
        assert_eq!(redact_ip(Some("")), "unknown");
        assert_eq!(redact_ip(Some("   ")), "unknown");
        assert_eq!(redact_ip(Some("2001:db8::1")), "2001:***:***");
        assert_eq!(redact_ip(Some("not_an_ip")), "no***ip");
        assert_eq!(redact_ip(Some("ab")), "***");
    }

    #[test]
    fn test_redact_email() {
        assert_eq!(redact_email(None), "unknown");
        assert_eq!(redact_email(Some("test@example.com")), "t***@example.com");
        assert_eq!(redact_email(Some("ab@example.com")), "***@example.com");
    }

    #[test]
    fn test_redact_email_more() {
        assert_eq!(redact_email(Some("")), "unknown");
        assert_eq!(redact_email(Some("   ")), "unknown");
        assert_eq!(redact_email(Some("abc@example.com")), "a***@example.com");
        assert_eq!(redact_email(Some("abcde@example.com")), "ab***@example.com");
        assert_eq!(redact_email(Some("noatsign")), "no***gn");
    }

    #[test]
    fn test_redact_email_local_part_boundaries() {
        assert_eq!(redact_email(Some("a@b.co")), "***@b.co");
        assert_eq!(redact_email(Some("ab@b.co")), "***@b.co");
        assert_eq!(redact_email(Some("abc@b.co")), "a***@b.co");
        assert_eq!(redact_email(Some("abcd@b.co")), "a***@b.co");
        assert_eq!(redact_email(Some("abcde@b.co")), "ab***@b.co");
        assert_eq!(redact_email(Some("abcdef@b.co")), "ab***@b.co");
    }

    #[test]
    fn test_redact_ip_extra_segments() {
        assert_eq!(redact_ip(Some("1.2.3.4.5")), "1.***.5");
        assert_eq!(redact_ip(Some("1.2.3")), "1.***.3");
        assert_eq!(redact_ip(Some("1.2.3.4.5.6")), "1.***.6");
    }

    #[test]
    fn test_redact_ip_single_colon() {
        assert_eq!(redact_ip(Some(":")), ":***:***");
        assert_eq!(redact_ip(Some("a:")), "a:***:***");
        assert_eq!(redact_ip(Some(":b")), ":***:***");
    }

    #[test]
    fn test_redact_ip_double_colon() {
        assert_eq!(redact_ip(Some("::")), ":***:***");
        assert_eq!(redact_ip(Some(":::")), ":***:***");
    }

    #[test]
    fn test_redact_email_at_start() {
        assert_eq!(redact_email(Some("@example.com")), "***@example.com");
    }

    #[test]
    fn test_redact_email_multiple_at() {
        assert_eq!(redact_email(Some("a@b@c.com")), "***@b@c.com");
        assert_eq!(redact_email(Some("@a@b.com")), "***@a@b.com");
    }

    #[test]
    fn test_redact_email_dots_in_local() {
        assert_eq!(
            redact_email(Some("first.last@example.com")),
            "fi***@example.com"
        );
    }

    #[test]
    fn test_redact_basic_very_long() {
        let long = "abcdefghijklmnopqrstuvwxyz1234567890";
        assert_eq!(redact_basic(Some(long)), "ab***90");
        assert_eq!(redact_basic(Some("a")), "***");
        assert_eq!(redact_basic(Some("xy")), "***");
    }

    #[test]
    fn test_redact_basic_whitespace_mixed() {
        assert_eq!(redact_basic(Some("  a  ")), "***");
        assert_eq!(redact_basic(Some("  ab  ")), "***");
        assert_eq!(redact_basic(Some("  abc  ")), "***");
        assert_eq!(redact_basic(Some("  abcd  ")), "ab***cd");
    }

    #[test]
    fn test_redact_basic_non_ascii_no_panic() {
        // 多字节 UTF-8 按字节切片会 panic（end byte index is not a char boundary）
        assert_eq!(redact_basic(Some("张三")), "***");
        assert_eq!(redact_basic(Some("😀ab")), "***");
        assert_eq!(redact_basic(Some("abcd张")), "ab***d张");
        assert_eq!(redact_basic(Some("密钥")), "***");
        assert_eq!(redact_basic(Some("密钥值")), "***");
    }

    #[test]
    fn test_redact_basic_char_boundary_slicing() {
        // 前缀/后缀按字符边界切片：多字节 UTF-8 不落在字符内部
        assert_eq!(redact_basic(Some("张三abc")), "张三***bc");
        assert_eq!(redact_basic(Some("密钥abc")), "密钥***bc");
        assert_eq!(redact_basic(Some("张三李四")), "张三***李四");
        assert_eq!(redact_basic(Some("张三abcde")), "张三***de");
    }

    #[test]
    fn test_redact_ip_non_ascii_no_panic() {
        assert_eq!(redact_ip(Some("张三abc")), "张三***bc");
    }

    #[test]
    fn test_redact_email_non_ascii_no_panic() {
        assert_eq!(redact_email(Some("张@example.com")), "***@example.com");
        assert_eq!(redact_email(Some("钥@x.com")), "***@x.com");
        assert_eq!(redact_email(Some("张abc@example.com")), "张***@example.com");
        assert_eq!(
            redact_email(Some("张abcde@example.com")),
            "张a***@example.com"
        );
    }

    #[test]
    fn test_redact_email_char_boundary_slicing() {
        // 本地部分前缀按字符边界切片：多字节 UTF-8 不落在字符内部
        assert_eq!(
            redact_email(Some("密钥abc@example.com")),
            "密钥***@example.com"
        );
        assert_eq!(
            redact_email(Some("张三abcde@example.com")),
            "张三***@example.com"
        );
    }

    #[test]
    fn test_redact_user_id_delegates() {
        assert_eq!(redact_user_id(Some("long_user_id_123")), "lo***23");
        assert_eq!(redact_user_id(Some("xy")), "***");
    }

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_redact_ban_target() {
        use crate::storage::BanTarget;

        let ip_target = BanTarget::Ip("192.168.1.1".to_string());
        assert_eq!(redact_ban_target(&ip_target), "192.168.***.***");

        let user_target = BanTarget::UserId("user123".to_string());
        assert_eq!(redact_ban_target(&user_target), "us***23");

        let mac_target = BanTarget::Mac("00:1a:2b:3c:4d:5e".to_string());
        assert_eq!(redact_ban_target(&mac_target), "00***5e");

        // Geo 国家代码为 2 字母，redact_basic 对 <4 字符返回 "***"
        let geo_target = BanTarget::Geo {
            country_code: "CN".to_string(),
        };
        assert_eq!(redact_ban_target(&geo_target), "Geo(***)");
    }

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_redact_ban_target_short_values() {
        use crate::storage::BanTarget;

        let ip_target = BanTarget::Ip("1.2.3.4".to_string());
        assert_eq!(redact_ban_target(&ip_target), "1.2.***.***");

        let user_target = BanTarget::UserId("ab".to_string());
        assert_eq!(redact_ban_target(&user_target), "***");

        let mac_target = BanTarget::Mac("ab".to_string());
        assert_eq!(redact_ban_target(&mac_target), "***");
    }

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_redact_ban_target_empty_values() {
        use crate::storage::BanTarget;

        assert_eq!(redact_ban_target(&BanTarget::Ip("".to_string())), "unknown");
        assert_eq!(
            redact_ban_target(&BanTarget::UserId("".to_string())),
            "unknown"
        );
        assert_eq!(
            redact_ban_target(&BanTarget::Mac("".to_string())),
            "unknown"
        );
        // Geo 空国家代码 → redact_basic 返回 "unknown"
        assert_eq!(
            redact_ban_target(&BanTarget::Geo {
                country_code: "".to_string()
            }),
            "Geo(unknown)"
        );
    }

    #[cfg(feature = "log-redaction")]
    mod log_redaction_tests {
        use super::*;

        // 引擎无关锚（inklog DataMasker 迁移前后均应绿）

        #[test]
        fn test_redact_advanced_field_name_gate() {
            // 敏感字段名门控（is_sensitive_field_name 入口前置）整段脱敏
            assert_eq!(redact_advanced(None, None), "unknown");
            assert_eq!(redact_advanced(Some(""), None), "unknown");
            assert_eq!(redact_advanced(Some("   "), None), "unknown");
            assert_eq!(redact_advanced(Some("val"), Some("password")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("my_secret")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("auth_token")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("api_key")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("credential")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("authorization")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("Password")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("SECRET_KEY")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("keychain")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("tokenizer")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("PASSWORD")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("SECRET")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("TOKEN")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("KEY")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("CREDENTIAL")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("AUTHORIZATION")), "***");
        }

        #[test]
        fn test_redact_advanced_short_value_fully_masked() {
            // 短值（≤4 字符）整段脱敏：与引擎无关的安全网
            assert_eq!(redact_advanced(Some("abc"), None), "***");
            assert_eq!(redact_advanced(Some("abcd"), None), "***");
            assert_eq!(redact_advanced(Some("hi"), None), "***");
            assert_eq!(redact_advanced(Some("bye"), Some("tag")), "***");
            assert_eq!(redact_advanced(Some("test"), None), "***");
        }

        #[test]
        fn test_redact_advanced_short_value_threshold_counts_chars() {
            // 短值阈值按字符计（非字节）：多字节 UTF-8 的字节数会越过
            // 4 字符的真实长度，这类短值须整段脱敏
            assert_eq!(redact_advanced(Some("张三"), None), "***");
            assert_eq!(redact_advanced(Some("密钥"), None), "***");
            assert_eq!(redact_advanced(Some("令牌"), None), "***");
            assert_eq!(redact_advanced(Some("密钥值"), None), "***");
            // 5 字符的非 ASCII 值越过短值阈值，不再被整段遮蔽
            assert_eq!(redact_advanced(Some("张三李四王"), None), "张三李四王");
        }

        #[test]
        fn test_redact_advanced_monkey_not_over_masked() {
            // 修复回归：词边界匹配。`monkey` 仅含 `key` 子串，
            // 不整段脱敏（不依赖引擎的字段名门控行为）
            let masked = redact_advanced(Some("value"), Some("monkey"));
            assert_ne!(masked, "***", "monkey must not be fully masked");
        }

        #[test]
        fn test_is_sensitive_field_name_boundaries() {
            // (d) 词边界行为钉住：高危前缀命中、纯子串不误伤
            assert!(is_sensitive_field_name("keychain"));
            assert!(is_sensitive_field_name("tokenizer"));
            assert!(is_sensitive_field_name("db_password"));
            assert!(is_sensitive_field_name("authToken"));
            assert!(is_sensitive_field_name("my_token_123"));
            assert!(is_sensitive_field_name("myToken"));
            assert!(is_sensitive_field_name("SECRET_KEY"));
            assert!(!is_sensitive_field_name("monkey"));
            assert!(!is_sensitive_field_name("username"));
            assert!(!is_sensitive_field_name("message"));
            assert!(!is_sensitive_field_name("role"));
        }

        #[test]
        fn test_is_sensitive_field_name_non_ascii_no_panic() {
            // 词元可含多字节字符（CJK 汉字等字母类字符的 is_alphanumeric 为真；
            // emoji 为分隔符不入词元）：定长字节前缀切片会落在字符内部触发 panic
            assert!(!is_sensitive_field_name("密钥"));
            assert!(!is_sensitive_field_name("令牌"));
            assert!(!is_sensitive_field_name("密码字段"));
            // 混合词元仍按 ASCII 前缀命中
            assert!(is_sensitive_field_name("key密钥"));
        }

        #[test]
        fn test_is_sensitive_field_name_emoji_is_separator() {
            // emoji 属符号类（is_alphanumeric 为假），被 split_field_tokens
            // 视为分隔符不入词元；CJK 汉字才是字母类字符
            assert!(!'\u{1F600}'.is_alphanumeric());
            assert!('密'.is_alphanumeric());
            assert!(!is_sensitive_field_name("😀abc"));
            assert!(!is_sensitive_field_name("abc😀"));
            assert!(!is_sensitive_field_name("🇨🇳🇺🇸"));
        }

        #[test]
        fn test_redact_http_content_key_value_forms() {
            // (a) password/token/authorization/api_key/secret/credential
            // 键值对三格式（URL k=v / YAML k: v / JSON "k":"v"）值被掩码，
            // 相邻非敏感参数保留
            for key in [
                "password",
                "token",
                "authorization",
                "api_key",
                "secret",
                "credential",
            ] {
                let url = format!("GET /api?{}=s3cr3tV4lue user=bob", key);
                let redacted = redact_http_content(&url);
                assert!(
                    !redacted.contains("s3cr3tV4lue"),
                    "{key} URL form is not redacted: {redacted}"
                );
                assert!(
                    redacted.contains("user=bob"),
                    "{key} URL form harms an adjacent parameter: {redacted}"
                );

                let yaml = format!("{}: s3cr3tV4lue\nusername: bob", key);
                let redacted = redact_http_content(&yaml);
                assert!(
                    !redacted.contains("s3cr3tV4lue"),
                    "{key} YAML form is not redacted: {redacted}"
                );
                assert!(
                    redacted.contains("username: bob"),
                    "{key} YAML form harms an adjacent key: {redacted}"
                );

                let json = format!(r#"{{"{}": "s3cr3tV4lue", "username": "bob"}}"#, key);
                let redacted = redact_http_content(&json);
                assert!(
                    !redacted.contains("s3cr3tV4lue"),
                    "{key} JSON form is not redacted: {redacted}"
                );
                assert!(
                    redacted.contains("bob"),
                    "{key} JSON form harms an adjacent key: {redacted}"
                );
            }
        }

        #[test]
        fn test_redact_http_content_value_patterns() {
            // (b) email/phone/id_card/credit_card 值模式脱敏
            let cases = [
                ("email=test@example.com", "test@example.com"),
                ("phone=13800138000", "13800138000"),
                ("id_card=110101199001011234", "110101199001011234"),
                ("credit_card=1234-5678-9012-3456", "1234-5678-9012-3456"),
                ("card 1234567890123456", "1234567890123456"),
            ];
            for (input, secret) in cases {
                let redacted = redact_http_content(input);
                assert!(
                    !redacted.contains(secret),
                    "the value pattern is not redacted: {input} -> {redacted}"
                );
            }
        }

        #[test]
        fn test_redact_http_content_non_sensitive_unchanged() {
            // (c) 非敏感输入原样返回
            assert_eq!(redact_http_content(""), "");
            assert_eq!(
                redact_http_content("username=user123, role=admin"),
                "username=user123, role=admin"
            );
            let content = "line1\nusername=user123\nline2";
            assert_eq!(redact_http_content(content), content);
        }

        #[test]
        fn test_redact_http_content_multiple_sensitive() {
            let content = "password=abc123 token=xyz789";
            assert_eq!(redact_http_content(content), "*** ***");
        }

        #[test]
        fn test_redact_http_content_mixed_newlines() {
            let content = "line1\npassword=secret123\nline2\ntoken=abc";
            let redacted = redact_http_content(content);
            assert!(redacted.contains("line1"));
            assert!(redacted.contains("line2"));
            assert!(!redacted.contains("secret123"));
            assert!(!redacted.contains("token=abc"));
        }

        #[test]
        fn test_redact_http_content_special_chars_in_value() {
            let content = r#"password=abc123!@#$%"#;
            let redacted = redact_http_content(content);
            assert!(!redacted.contains("abc123!@#$%"));
        }

        #[test]
        fn test_contains_sensitive_info_detects() {
            // (e) 检出语义：敏感输入返回 true
            assert!(contains_sensitive_info("password=secret123"));
            assert!(contains_sensitive_info("api_key=abc123xyz"));
            assert!(contains_sensitive_info(
                "token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"
            ));
            assert!(contains_sensitive_info("secret=mysecret"));
            assert!(contains_sensitive_info("credential=mycred"));
            assert!(contains_sensitive_info("authorization=bearer token"));
            assert!(contains_sensitive_info("password: secret123"));
            assert!(contains_sensitive_info("token : abcdef"));
            assert!(contains_sensitive_info("api_key=value123"));
            assert!(contains_sensitive_info("SECRET:myvalue"));
            assert!(contains_sensitive_info("user=admin, password=secret"));
            assert!(contains_sensitive_info(
                "header contains authorization: bearer xyz"
            ));
            assert!(contains_sensitive_info(
                "email: test@example.com, password: secret"
            ));
            assert!(contains_sensitive_info("email me at test@example.com"));
            assert!(contains_sensitive_info("call 13800138000"));
            assert!(contains_sensitive_info("my id is 110101199001011234"));
            assert!(contains_sensitive_info("card 1234567890123456"));

            assert!(!contains_sensitive_info("username=user123"));
            assert!(!contains_sensitive_info("hello world"));
            assert!(!contains_sensitive_info("user=admin, role=editor"));
            assert!(!contains_sensitive_info(""));
            assert!(!contains_sensitive_info(" "));
            assert!(!contains_sensitive_info("\t"));
            assert!(!contains_sensitive_info("a"));
            assert!(!contains_sensitive_info("ab"));
        }

        #[test]
        fn test_redaction_config_field_name_gate() {
            // RedactionConfig 经 redact_advanced 字段名门控（引擎无关）
            let config = RedactionConfig::new()
                .add_field("password", true)
                .add_field("username", false);

            let result = config.format(|field| match field {
                "password" => Some("secret123".to_string()),
                "username" => Some("alice".to_string()),
                _ => None,
            });
            assert_eq!(result, "{password=***, username=alice}");

            let config = RedactionConfig::new()
                .add_field("pwd", true)
                .add_field("tok", true);
            let result = config.format(|field| match field {
                "pwd" => Some("ab".to_string()),
                "tok" => Some("abc".to_string()),
                _ => None,
            });
            assert_eq!(result, "{pwd=***, tok=***}");

            let config = RedactionConfig::new()
                .add_field("password", true)
                .add_field("secret_token", true)
                .add_field("role", false);
            let result = config.format(|field| match field {
                "password" => Some("secret123".to_string()),
                "secret_token" => Some("myvalue".to_string()),
                "role" => Some("admin".to_string()),
                _ => None,
            });
            assert_eq!(result, "{password=***, secret_token=***, role=admin}");
        }

        #[test]
        fn test_redaction_config_default_and_empty() {
            let config: RedactionConfig = Default::default();
            assert_eq!(config.fields.len(), 0);
            let config = RedactionConfig::new();
            assert_eq!(config.format(|_| None), "{}");
        }

        // 新引擎覆盖锚（inklog DataMasker 迁移后转绿）

        #[test]
        fn test_redact_advanced_non_sensitive_value_unchanged() {
            // 新契约：非敏感值原样返回（除命中规则外无副作用改写）
            assert_eq!(redact_advanced(Some("user123"), None), "user123");
            assert_eq!(
                redact_advanced(Some("user123"), Some("username")),
                "user123"
            );
            assert_eq!(
                redact_advanced(Some("plain text value"), None),
                "plain text value"
            );
        }

        #[test]
        fn test_redact_advanced_masks_value_pattern_in_place() {
            // 值模式命中：原地掩码值本身并保留上下文（非整段替换）
            let masked = redact_advanced(Some("contact me at test@example.com"), None);
            assert!(!masked.contains("test@example.com"));
            assert!(masked.contains("contact me at"));
        }

        #[test]
        fn test_contains_sensitive_info_bare_field_name_not_sensitive() {
            // 新契约：裸字段名（无值）不构成敏感信息，判定与 redact_advanced 一致
            assert!(!contains_sensitive_info("password"));
            assert!(!contains_sensitive_info("token"));
            assert!(!contains_sensitive_info("secret"));
            assert!(!contains_sensitive_info("api_key"));
            assert!(!contains_sensitive_info("credential"));
            assert!(!contains_sensitive_info("authorization"));
        }

        #[test]
        fn test_new_engine_extra_categories_masked() {
            // inklog 21 条内置规则新增类别（aws_key/jwt/ipv4/mac）：
            // 旧正则引擎不覆盖，迁移后由 DataMasker 免费获得
            let masked = redact_http_content("aws key AKIAIOSFODNN7EXAMPLE rotated");
            assert!(!masked.contains("AKIAIOSFODNN7EXAMPLE"));

            let masked = redact_http_content(
                "jwt eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N",
            );
            assert!(!masked.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));

            let masked = redact_http_content("client ip 10.0.0.5 disconnected");
            assert!(!masked.contains("10.0.0.5"));

            let masked = redact_http_content("mac 00:1a:2b:3c:4d:5e seen");
            assert!(!masked.contains("00:1a:2b:3c:4d:5e"));

            assert!(contains_sensitive_info("AKIAIOSFODNN7EXAMPLE"));
            assert!(contains_sensitive_info("client 192.168.1.100 connected"));
            assert!(contains_sensitive_info("mac 00:1a:2b:3c:4d:5e seen"));
        }
    }
}
