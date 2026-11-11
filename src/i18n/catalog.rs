// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Fluent message catalog for limiteron i18n.
//!
//! FTL resources live on disk at `locales/{en,zh}/messages.ftl` and are
//! embedded at compile time via [`include_str!`]; the disk/embedded pair is
//! kept in sync by construction (same file) plus a guard test
//! `test_locales_dir_matches_embedded`.
//!
//! Lookup is delegated to the `sdforge::i18n` host registry (janvi/CodeNexus
//! pattern): [`ensure_registered`] parses the embedded `key = value` lines
//! once and registers both locales; the public [`translate`]/[`t`] family
//! then resolves through `sdforge::i18n::translate_for` (host registration →
//! built-in en → the key itself; never panics). Substitution follows the
//! registry's single-pass `{ $name }` template rules — the catalog is
//! single-line plain values (no selectors/multi-line, guarded by tests), so
//! this is behaviorally identical to the previous Fluent-bundle rendering
//! (isolation marks were already disabled).

use std::sync::OnceLock;

/// Embedded English FTL resource (`locales/en/messages.ftl`).
pub(crate) const EN_FTL: &str = include_str!("../../locales/en/messages.ftl");
/// Embedded Chinese FTL resource (`locales/zh/messages.ftl`).
pub(crate) const ZH_FTL: &str = include_str!("../../locales/zh/messages.ftl");

/// Registry bucket for a language tag: every `zh*` variant resolves to the
/// `zh` bucket, anything else to `en` (unsupported languages terminate in
/// English — the pre-delegation lookup semantics).
fn registry_lang(lang: &str) -> &'static str {
    if lang.trim().to_lowercase().starts_with("zh") {
        "zh"
    } else {
        "en"
    }
}

/// Parse a single-source FTL into `(key, value)` pairs.
///
/// Line-oriented extraction (janvi `catalog.rs` pattern): skip blank lines
/// and `#` comments, split at the first `=`, trim both sides. The catalog is
/// verified single-line-only (no continuation/selector values), so this
/// matches a full Fluent parse for these resources.
fn parse_entries(source: &'static str) -> Vec<(&'static str, &'static str)> {
    source
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (raw_key, raw_value) = trimmed.split_once('=')?;
            let key = raw_key.trim();
            if key.is_empty() || key.contains(' ') {
                return None;
            }
            Some((key, raw_value.trim()))
        })
        .collect()
}

/// Entries for a registry bucket (`"en"` / `"zh"`).
fn entries_for(lang: &str) -> Vec<(&'static str, &'static str)> {
    match lang {
        "zh" => parse_entries(ZH_FTL),
        _ => parse_entries(EN_FTL),
    }
}

/// Key set for a registry bucket (key-parity guards and existence asserts).
#[cfg(test)]
pub(crate) fn keys_for(lang: &str) -> std::collections::BTreeSet<&'static str> {
    entries_for(lang).into_iter().map(|(key, _)| key).collect()
}

static REGISTERED: OnceLock<()> = OnceLock::new();

/// Register the embedded catalog into the `sdforge::i18n` host registry
/// (both locales), once per process. Idempotent and safe to call from any
/// public entry point below — first caller wins, later calls are no-ops.
pub(crate) fn ensure_registered() {
    REGISTERED.get_or_init(|| {
        for lang in ["en", "zh"] {
            for (key, value) in entries_for(lang) {
                sdforge::i18n::register_translation(lang, key, value);
            }
        }
    });
}

/// Translate `key` with the current locale, falling back to English, then to
/// the key itself.
pub fn translate(key: &str, args: &[(&str, String)]) -> String {
    let lang = super::locale::current_locale()
        .language
        .as_str()
        .to_string();
    translate_for(&lang, key, args)
}

/// Translate `key` to English specifically, regardless of current locale.
pub fn translate_en(key: &str, args: &[(&str, String)]) -> String {
    ensure_registered();
    sdforge::i18n::translate_for("en", key, args)
}

/// Translate `key` for an explicit language tag (used by callers that carry
/// their own locale, e.g. the ICU-based `LimiterI18nFormatter`).
///
/// Falls back to English for unsupported/unknown languages, then to the key.
pub fn translate_for(lang: &str, key: &str, args: &[(&str, String)]) -> String {
    ensure_registered();
    sdforge::i18n::translate_for(registry_lang(lang), key, args)
}

/// Shorthand for [`translate`].
pub fn t(key: &str, args: &[(&str, String)]) -> String {
    translate(key, args)
}

/// Convenience: [`translate`] with no dynamic arguments.
pub fn t_simple(key: &str) -> String {
    translate(key, &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Extract the message-key set from an FTL source (lines of the form
    /// `key = value`, ignoring comments and blank lines).
    fn ftl_keys(ftl: &str) -> BTreeSet<String> {
        ftl.lines()
            .filter_map(|line| line.split_once(" = "))
            .filter(|(key, _)| !key.contains(' '))
            .map(|(key, _)| key.trim().to_string())
            .collect()
    }

    /// Extract the `{ $name }` placeholder set from an FTL value.
    fn placeholders(value: &str) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let mut rest = value;
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            let inner = rest[open + 1..open + close].trim();
            if let Some(name) = inner.strip_prefix('$')
                && !name.is_empty()
            {
                names.insert(name.to_string());
            }
            rest = &rest[open + close + 1..];
        }
        names
    }

    #[test]
    fn test_key_parity_en_zh() {
        let en = ftl_keys(EN_FTL);
        let zh = ftl_keys(ZH_FTL);
        assert!(!en.is_empty(), "EN catalog must not be empty");
        let missing_in_zh: Vec<_> = en.difference(&zh).collect();
        let missing_in_en: Vec<_> = zh.difference(&en).collect();
        assert!(
            missing_in_zh.is_empty() && missing_in_en.is_empty(),
            "en/zh key sets diverge: missing_in_zh={missing_in_zh:?} missing_in_en={missing_in_en:?}"
        );
    }

    /// 守卫：双语占位符集合一致（键位齐 + 占位符齐；无 fluent-syntax，
    /// 以轻量解析复刻键位守卫形态）。占位符缺失的一侧会在渲染时把
    /// `{ $name }` 原样漏出。
    #[test]
    fn test_placeholder_parity_en_zh() {
        let en: std::collections::HashMap<_, _> = parse_entries(EN_FTL).into_iter().collect();
        let mismatches: Vec<String> = parse_entries(ZH_FTL)
            .into_iter()
            .filter_map(|(key, zh_value)| {
                let en_value = en.get(key)?;
                let (en_ph, zh_ph) = (placeholders(en_value), placeholders(zh_value));
                (en_ph != zh_ph).then(|| format!("{key}: en={en_ph:?} zh={zh_ph:?}"))
            })
            .collect();
        assert!(
            mismatches.is_empty(),
            "placeholder sets diverge between locales: {mismatches:?}"
        );
    }

    /// 守卫：目录为纯单行 `key = value` 形态（无多行续值/选择器）——
    /// 注册式解析（按行取值）与 Fluent 解析行为一致的前提。
    #[test]
    fn test_catalog_is_single_line_plain_values() {
        for (lang, ftl) in [("en", EN_FTL), ("zh", ZH_FTL)] {
            assert!(
                !ftl.lines()
                    .any(|l| l.starts_with(' ') || l.starts_with('\t')),
                "{lang} catalog must not contain continuation lines"
            );
            assert!(
                !ftl.contains("\n\n\n"),
                "{lang} catalog must not contain stray blank blocks"
            );
        }
    }

    /// 守卫：locales/ 磁盘文件与编译期内嵌常量一致（include_str! 按路径内嵌，
    /// 本测试防止目录被移动/改名后静默回退到旧路径之外的内容）。
    #[test]
    fn test_locales_dir_matches_embedded() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let en_disk = std::fs::read_to_string(format!("{manifest}/locales/en/messages.ftl"))
            .expect("locales/en/messages.ftl must exist on disk");
        let zh_disk = std::fs::read_to_string(format!("{manifest}/locales/zh/messages.ftl"))
            .expect("locales/zh/messages.ftl must exist on disk");
        assert_eq!(en_disk, EN_FTL, "disk en catalog drifted from embedded");
        assert_eq!(zh_disk, ZH_FTL, "disk zh catalog drifted from embedded");
    }

    /// 注册后目录键集即注册表键集：两桶零缺失（全部键经 sdforge 可解析）。
    #[test]
    fn test_registered_buckets_cover_full_catalog() {
        ensure_registered();
        for lang in ["en", "zh"] {
            for key in keys_for(lang) {
                assert_ne!(
                    sdforge::i18n::translate_for(lang, key, &[]),
                    key,
                    "{lang} key {key} must resolve after registration"
                );
            }
        }
    }

    #[test]
    fn test_fluent_en_simple() {
        assert_eq!(
            translate_for("en", "rate-limit-exceeded", &[]),
            "Rate limit exceeded".to_string()
        );
        assert_eq!(
            translate_for("en", "error-config", &[("message", "x".to_string())]),
            "Configuration error: x".to_string()
        );
    }

    #[test]
    fn test_fluent_zh_simple() {
        assert_eq!(
            translate_for("zh", "rate-limit-exceeded", &[]),
            "超出速率限制".to_string()
        );
        assert_eq!(
            translate_for(
                "zh",
                "error-storage-not-found",
                &[("message", "k".to_string())]
            ),
            "未找到: k".to_string()
        );
    }

    #[test]
    fn test_unknown_lang_falls_back_to_en_bundle() {
        assert_eq!(
            translate_for("ar", "rate-limit-exceeded", &[]),
            "Rate limit exceeded".to_string()
        );
        assert_eq!(
            translate_for("ja", "error-config", &[("message", "x".to_string())]),
            "Configuration error: x".to_string()
        );
    }

    #[test]
    fn test_missing_key_returns_none() {
        // translate 链尾回退：缺失键返回键本身，不 panic
        assert_eq!(translate("nonexistent-key", &[]), "nonexistent-key");
        assert_eq!(translate_en("nonexistent-key", &[]), "nonexistent-key");
        assert_eq!(
            translate_for("zh", "nonexistent-key", &[]),
            "nonexistent-key"
        );
    }

    #[test]
    fn test_translate_for_explicit_lang() {
        assert_eq!(
            translate_for("zh", "error-limit", &[("message", "x".to_string())]),
            "限流错误: x"
        );
        assert_eq!(
            translate_for("fr", "error-limit", &[("message", "x".to_string())]),
            "Rate limit error: x",
            "unsupported language must terminate in en"
        );
    }

    /// 单条精确渲染用例：(键, 占位符实参, en 期望, zh 期望)。
    type RenderCase = (
        &'static str,
        &'static [(&'static str, &'static str)],
        &'static str,
        &'static str,
    );

    /// 守卫：配置校验类键的 en 值与构造点内嵌的规范串逐字一致（canonical mirror
    /// 不变式）。构造点改了英文、目录没跟上时本测试红灯。
    #[test]
    fn test_config_validation_keys_are_canonical_mirror() {
        let cases: &[(&str, &str)] = &[
            ("config-version-empty", "Version cannot be empty"),
            ("config-rule-required", "At least one rule is required"),
            ("config-rule-id-empty", "Rule ID cannot be empty"),
            ("config-rule-name-empty", "Rule name cannot be empty"),
            (
                "config-rule-matcher-required",
                "Rule requires at least one matcher",
            ),
            (
                "config-rule-limiter-required",
                "Rule requires at least one limiter",
            ),
            ("config-rule-user-ids-empty", "User ID list cannot be empty"),
            (
                "config-rule-ip-ranges-empty",
                "IP range list cannot be empty",
            ),
            (
                "config-rule-countries-empty",
                "Country list cannot be empty",
            ),
            (
                "config-rule-api-versions-empty",
                "API version list cannot be empty",
            ),
            (
                "config-rule-device-types-empty",
                "Device type list cannot be empty",
            ),
            (
                "config-rule-matcher-name-empty",
                "Custom matcher name cannot be empty",
            ),
            (
                "config-rule-matcher-config-empty",
                "Custom matcher config cannot be empty",
            ),
            ("config-ban-threshold-not-zero", "Ban threshold cannot be 0"),
            (
                "config-backoff-multiplier-positive",
                "Backoff multiplier must be greater than 0",
            ),
            (
                "config-overdraft-max-not-zero",
                "Overdraft enabled: max overdraft cannot be 0",
            ),
            (
                "config-token-bucket-capacity-not-zero",
                "Token bucket capacity cannot be 0",
            ),
            (
                "config-token-bucket-refill-rate-not-zero",
                "Refill rate cannot be 0",
            ),
            ("config-max-requests-not-zero", "Max requests cannot be 0"),
            ("config-quota-limit-not-zero", "Quota limit cannot be 0"),
            (
                "config-max-concurrency-not-zero",
                "Max concurrency cannot be 0",
            ),
            (
                "config-leaky-bucket-capacity-not-zero",
                "Leaky bucket capacity cannot be 0",
            ),
            (
                "config-leaky-bucket-leak-rate-not-zero",
                "Leaky bucket leak_rate cannot be 0",
            ),
            (
                "config-sliding-window-log-max-requests-not-zero",
                "Sliding window log max_requests cannot be 0",
            ),
            (
                "config-priority-queue-total-per-window-not-zero",
                "Priority queue total_per_window cannot be 0",
            ),
            (
                "config-priority-queue-level-weights-invalid",
                "Priority queue level_weights must be non-empty and non-zero",
            ),
            (
                "config-admission-max-concurrent-not-zero",
                "Admission control max_concurrent cannot be 0",
            ),
            (
                "config-admission-max-per-second-not-zero",
                "Admission control max_per_second cannot be 0",
            ),
            (
                "config-custom-limiter-name-empty",
                "Custom limiter name cannot be empty",
            ),
            (
                "config-custom-limiter-config-empty",
                "Custom limiter config cannot be empty",
            ),
            (
                "telemetry-feature-disabled",
                "telemetry feature is disabled",
            ),
            (
                "prometheus-feature-disabled",
                "prometheus feature is disabled",
            ),
        ];
        for (key, expected) in cases {
            assert_eq!(
                translate_en(key, &[]),
                *expected,
                "en value of {key} must stay identical to the construction-site string"
            );
        }
    }

    /// 守卫：配置校验类键在 zh 侧必须渲染出中文（占位符全部解析，不退化为键名）。
    #[test]
    fn test_config_validation_keys_render_in_zh() {
        let cases: &[(&str, &[(&str, &str)])] = &[
            ("config-version-empty", &[]),
            ("config-duplicate-rule-id", &[("rule_id", "r7")]),
            ("config-rule-required", &[]),
            (
                "config-rule-validation-failed",
                &[("index", "2"), ("error", "boom")],
            ),
            ("config-rule-id-empty", &[]),
            ("config-rule-name-empty", &[]),
            ("config-rule-matcher-required", &[]),
            ("config-rule-limiter-required", &[]),
            (
                "config-matcher-validation-failed",
                &[("index", "0"), ("error", "boom")],
            ),
            (
                "config-limiter-validation-failed",
                &[("index", "1"), ("error", "boom")],
            ),
            ("config-rule-user-ids-empty", &[]),
            ("config-rule-ip-ranges-empty", &[]),
            ("config-rule-countries-empty", &[]),
            ("config-rule-api-versions-empty", &[]),
            ("config-rule-device-types-empty", &[]),
            ("config-rule-matcher-name-empty", &[]),
            ("config-rule-matcher-config-empty", &[]),
            ("config-ban-threshold-not-zero", &[]),
            ("config-backoff-multiplier-positive", &[]),
            ("config-overdraft-max-not-zero", &[]),
            ("config-token-bucket-capacity-not-zero", &[]),
            ("config-token-bucket-capacity-exceeds", &[("max", "1000")]),
            ("config-token-bucket-refill-rate-not-zero", &[]),
            ("config-token-bucket-refill-rate-exceeds", &[("max", "100")]),
            ("config-max-requests-not-zero", &[]),
            ("config-max-requests-exceeds", &[("max", "1000")]),
            ("config-quota-limit-not-zero", &[]),
            ("config-alert-threshold-exceeds", &[("max", "100")]),
            ("config-max-concurrency-not-zero", &[]),
            ("config-max-concurrency-exceeds", &[("max", "1000")]),
            ("config-leaky-bucket-capacity-not-zero", &[]),
            ("config-leaky-bucket-capacity-exceeds", &[("max", "1000")]),
            ("config-leaky-bucket-leak-rate-not-zero", &[]),
            ("config-leaky-bucket-leak-rate-exceeds", &[("max", "1000")]),
            ("config-sliding-window-log-max-requests-not-zero", &[]),
            (
                "config-sliding-window-log-max-requests-exceeds",
                &[("max", "1000")],
            ),
            ("config-priority-queue-total-per-window-not-zero", &[]),
            ("config-priority-queue-level-weights-invalid", &[]),
            (
                "config-priority-queue-default-priority-out-of-range",
                &[("priority", "3"), ("levels", "5")],
            ),
            ("config-admission-max-concurrent-not-zero", &[]),
            ("config-admission-max-per-second-not-zero", &[]),
            ("config-custom-limiter-name-empty", &[]),
            ("config-custom-limiter-config-empty", &[]),
            ("telemetry-feature-disabled", &[]),
            ("prometheus-feature-disabled", &[]),
            (
                "telemetry-bind-port-failed",
                &[("port", "9090"), ("error", "denied")],
            ),
            ("integration-guard-internal-error", &[("error", "boom")]),
            (
                "integration-token-bucket-check-failed",
                &[("error", "boom")],
            ),
        ];
        for (key, args) in cases {
            let args: Vec<(&str, String)> =
                args.iter().map(|(k, v)| (*k, (*v).to_string())).collect();
            let rendered = translate_for("zh", key, &args);
            assert_ne!(rendered, *key, "key {key} fell back to its own id");
            assert!(
                rendered
                    .chars()
                    .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "zh rendering of {key} must contain Chinese, got {rendered:?}"
            );
        }
    }

    /// 守卫：带占位符的配置校验键在双语两侧都必须把占位符解析掉。
    #[test]
    fn test_config_validation_placeholders_resolve() {
        let cases: &[RenderCase] = &[
            (
                "config-duplicate-rule-id",
                &[("rule_id", "r7")],
                "Duplicate rule ID: r7",
                "规则 ID 重复: r7",
            ),
            (
                "config-rule-validation-failed",
                &[("index", "2"), ("error", "boom")],
                "Rule[2] validation failed: boom",
                "Rule[2] 校验失败: boom",
            ),
            (
                "config-matcher-validation-failed",
                &[("index", "0"), ("error", "boom")],
                "Matcher[0]: boom",
                "匹配器[0]: boom",
            ),
            (
                "config-limiter-validation-failed",
                &[("index", "1"), ("error", "boom")],
                "Limiter[1]: boom",
                "限流器[1]: boom",
            ),
            (
                "config-token-bucket-capacity-exceeds",
                &[("max", "1000")],
                "Token bucket capacity cannot exceed 1000",
                "令牌桶容量不能超过 1000",
            ),
            (
                "config-alert-threshold-exceeds",
                &[("max", "100")],
                "Alert threshold cannot exceed 100%",
                "告警阈值不能超过 100%",
            ),
            (
                "config-priority-queue-default-priority-out-of-range",
                &[("priority", "3"), ("levels", "5")],
                "Priority queue default_priority 3 out of range (levels: 5)",
                "优先级队列的默认优先级 3 越界（层级数: 5）",
            ),
            (
                "telemetry-bind-port-failed",
                &[("port", "9090"), ("error", "denied")],
                "Failed to bind to port 9090: denied",
                "绑定端口 9090 失败: denied",
            ),
            (
                "integration-guard-internal-error",
                &[("error", "boom")],
                "guard internal error: boom",
                "守卫内部错误: boom",
            ),
            (
                "integration-token-bucket-check-failed",
                &[("error", "boom")],
                "limiteron token bucket check failed: boom",
                "limiteron 令牌桶检查失败: boom",
            ),
        ];
        for (key, args, expected_en, expected_zh) in cases {
            let args: Vec<(&str, String)> =
                args.iter().map(|(k, v)| (*k, (*v).to_string())).collect();
            assert_eq!(
                translate_for("en", key, &args),
                *expected_en,
                "en rendering of {key} drifted"
            );
            assert_eq!(
                translate_for("zh", key, &args),
                *expected_zh,
                "zh rendering of {key} drifted"
            );
        }
    }
}
