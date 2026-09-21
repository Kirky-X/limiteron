// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Fluent message catalog for limiteron i18n.
//!
//! FTL resources live on disk at `locales/{en,zh}/messages.ftl` and are
//! embedded at compile time via [`include_str!`]; the disk/embedded pair is
//! kept in sync by construction (same file) plus a guard test
//! `test_locales_dir_matches_embedded`.
//!
//! Bundles are `fluent_bundle::concurrent::FluentBundle` (Send + Sync) cached
//! in [`OnceLock`] statics — safe to share across threads. Lookup chain:
//! current language → `"en"` → the key itself (never panics).

use std::sync::OnceLock;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

/// Embedded English FTL resource (`locales/en/messages.ftl`).
pub(crate) const EN_FTL: &str = include_str!("../../locales/en/messages.ftl");
/// Embedded Chinese FTL resource (`locales/zh/messages.ftl`).
pub(crate) const ZH_FTL: &str = include_str!("../../locales/zh/messages.ftl");

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
    format_from_bundle("en", key, args).unwrap_or_else(|| key.to_string())
}

/// Translate `key` for an explicit language tag (used by callers that carry
/// their own locale, e.g. the ICU-based `LimiterI18nFormatter`).
///
/// Falls back to English for unsupported/unknown languages, then to the key.
pub fn translate_for(lang: &str, key: &str, args: &[(&str, String)]) -> String {
    format_from_bundle(lang, key, args)
        .or_else(|| format_from_bundle("en", key, args))
        .unwrap_or_else(|| key.to_string())
}

/// Shorthand for [`translate`].
pub fn t(key: &str, args: &[(&str, String)]) -> String {
    translate(key, args)
}

/// Convenience: [`translate`] with no dynamic arguments.
pub fn t_simple(key: &str) -> String {
    translate(key, &[])
}

/// Cached concurrent Fluent bundles (thread-safe, built once on first access).
static EN_BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();
static ZH_BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();

/// Format a message from the Fluent catalog for the given language.
///
/// Any language other than `zh` resolves to the English bundle; missing keys
/// return `None` so callers can apply their own fallback.
fn format_from_bundle(lang: &str, key: &str, args: &[(&str, String)]) -> Option<String> {
    let bundle = match lang {
        "zh" => ZH_BUNDLE.get_or_init(build_zh_bundle),
        _ => EN_BUNDLE.get_or_init(build_en_bundle),
    };

    let msg = bundle.get_message(key)?;
    let pattern = msg.value()?;

    let mut fluent_args = FluentArgs::new();
    for (name, value) in args {
        fluent_args.set(*name, FluentValue::from(value.clone()));
    }

    let mut errors = vec![];
    let result = bundle.format_pattern(pattern, Some(&fluent_args), &mut errors);
    Some(result.to_string())
}

fn build_en_bundle() -> FluentBundle<FluentResource> {
    let resource = FluentResource::try_new(EN_FTL.to_string()).unwrap_or_else(|e| e.0);
    let langid: LanguageIdentifier = "en".parse().expect("'en' is a valid language identifier");
    let mut bundle = FluentBundle::new_concurrent(vec![langid]);
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .expect("EN resources should add without conflict");
    bundle
}

fn build_zh_bundle() -> FluentBundle<FluentResource> {
    let resource = FluentResource::try_new(ZH_FTL.to_string()).unwrap_or_else(|e| e.0);
    let langid: LanguageIdentifier = "zh".parse().expect("'zh' is a valid language identifier");
    let mut bundle = FluentBundle::new_concurrent(vec![langid]);
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .expect("ZH resources should add without conflict");
    bundle
}

/// Extract the message-key set from an FTL source (lines of the form
/// `key = value`, ignoring comments and blank lines).
#[cfg(test)]
fn ftl_keys(ftl: &str) -> std::collections::BTreeSet<String> {
    ftl.lines()
        .filter_map(|line| line.split_once(" = "))
        .filter(|(key, _)| !key.contains(' '))
        .map(|(key, _)| key.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_fluent_en_simple() {
        assert_eq!(
            format_from_bundle("en", "rate-limit-exceeded", &[]),
            Some("Rate limit exceeded".to_string())
        );
        assert_eq!(
            format_from_bundle("en", "error-config", &[("message", "x".to_string())]),
            Some("Configuration error: x".to_string())
        );
    }

    #[test]
    fn test_fluent_zh_simple() {
        assert_eq!(
            format_from_bundle("zh", "rate-limit-exceeded", &[]),
            Some("超出速率限制".to_string())
        );
        assert_eq!(
            format_from_bundle(
                "zh",
                "error-storage-not-found",
                &[("message", "k".to_string())]
            ),
            Some("未找到: k".to_string())
        );
    }

    #[test]
    fn test_unknown_lang_falls_back_to_en_bundle() {
        assert_eq!(
            format_from_bundle("ar", "rate-limit-exceeded", &[]),
            Some("Rate limit exceeded".to_string())
        );
        assert_eq!(
            format_from_bundle("ja", "error-config", &[("message", "x".to_string())]),
            Some("Configuration error: x".to_string())
        );
    }

    #[test]
    fn test_missing_key_returns_none() {
        assert_eq!(format_from_bundle("en", "nonexistent-key", &[]), None);
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
}
