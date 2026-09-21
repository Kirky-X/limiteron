// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Locale detection chain and global locale context.
//!
//! Detection priority (unified across projects, see change `unify-rust-i18n`):
//! 1. `LIMITERON_LANG` environment variable (project override)
//! 2. `LC_ALL` → `LC_MESSAGES` → `LANG` (explicit POSIX chain)
//! 3. `sys-locale` system detection
//! 4. `"en"` ultimate fallback
//!
//! Only `en` and `zh` are supported: any unsupported/unknown/malformed value
//! continues down the chain, and the chain always terminates at `en`.
//! Environment/system reads are funneled through `detect_from` so the chain
//! logic is testable as a pure function.

use std::str::FromStr;
use std::sync::{OnceLock, RwLock};

use unic_langid::LanguageIdentifier;

use super::I18nError;

/// Global default locale, initialized once on first access.
static GLOBAL_LOCALE: OnceLock<LanguageIdentifier> = OnceLock::new();

/// Override locale set via [`set_locale()`].
static OVERRIDE_LOCALE: RwLock<Option<LanguageIdentifier>> = RwLock::new(None);

/// Normalize a raw locale tag into a supported [`LanguageIdentifier`].
///
/// Strips `@modifier` and `.codeset` suffixes, maps `_` to `-`, rejects
/// `C`/`POSIX` (the chain continues instead), and collapses every `zh`
/// variant (zh-TW/HK/SG/Hans...) to `zh` and every `en` variant to `en`.
/// Unsupported languages return `None` so the detection chain keeps going.
fn normalize(raw: &str) -> Option<LanguageIdentifier> {
    let s = raw
        .split('@')
        .next()?
        .trim()
        .split('.')
        .next()?
        .replace('_', "-");
    if matches!(s.as_str(), "C" | "POSIX") {
        return None;
    }
    let langid = LanguageIdentifier::from_str(&s).ok()?;
    match langid.language.as_str() {
        "zh" => Some(LanguageIdentifier::from_str("zh").expect("'zh' is a valid langid")),
        "en" => Some(LanguageIdentifier::from_str("en").expect("'en' is a valid langid")),
        _ => None,
    }
}

/// Pure detection chain: resolve the locale from injected env/system lookups.
///
/// `get_env` returns an environment variable's value; `get_sys` returns the
/// system locale. Priority: `LIMITERON_LANG` → `LC_ALL` → `LC_MESSAGES` →
/// `LANG` → system → `en`. An unsupported value does not abort the chain —
/// it falls through to the next source.
fn detect_from(
    get_env: &dyn Fn(&str) -> Option<String>,
    get_sys: &dyn Fn() -> Option<String>,
) -> LanguageIdentifier {
    // 1. 项目覆盖变量
    if let Some(lang) = get_env("LIMITERON_LANG")
        && !lang.trim().is_empty()
        && let Some(l) = normalize(&lang)
    {
        return l;
    }
    // 2. 显式 POSIX 环境链（sys-locale 在 Unix 上内部也读这些；显式读取
    //    是为了 Windows/边缘环境的确定性）
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(lang) = get_env(key)
            && !lang.trim().is_empty()
            && let Some(l) = normalize(&lang)
        {
            return l;
        }
    }
    // 3. sys-locale 系统探测
    if let Some(sys) = get_sys()
        && let Some(l) = normalize(&sys)
    {
        return l;
    }
    // 4. 终极回退
    LanguageIdentifier::from_str("en").expect("'en' is a valid language identifier")
}

/// Detect the locale from the real environment/system.
fn detect_locale() -> LanguageIdentifier {
    detect_from(&|key| std::env::var(key).ok(), &sys_locale::get_locale)
}

/// Warm up the global locale from the detection chain.
///
/// Called once at process start (e.g. by the `limiteron-cli` binary) so the
/// detected locale is fixed before any message formatting happens. Calling
/// `t()`/`translate()` before `init()` is also safe — the global locale is
/// lazily initialized on first access either way.
pub fn init() {
    let _ = current_locale();
}

/// Get the current locale.
///
/// Resolution order: override set via [`set_locale()`] → global default
/// (detected once on first access).
pub fn current_locale() -> LanguageIdentifier {
    if let Ok(guard) = OVERRIDE_LOCALE.read()
        && let Some(locale) = guard.as_ref()
    {
        return locale.clone();
    }
    GLOBAL_LOCALE.get_or_init(detect_locale).clone()
}

/// Set the locale override (explicit entry points such as CLI flags).
///
/// # Errors
/// Returns [`I18nError::InvalidLocale`] if the locale string cannot be parsed.
pub fn set_locale(locale: &str) -> Result<(), I18nError> {
    let parsed = LanguageIdentifier::from_str(locale).map_err(|e| I18nError::InvalidLocale {
        input: locale.to_string(),
        reason: e.to_string(),
    })?;
    *OVERRIDE_LOCALE.write().unwrap_or_else(|e| e.into_inner()) = Some(parsed);
    Ok(())
}

/// Clear the locale override, reverting to the auto-detected locale.
pub fn clear_locale_override() {
    *OVERRIDE_LOCALE.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Get the auto-detected locale (ignoring any override).
pub fn detected_locale() -> LanguageIdentifier {
    GLOBAL_LOCALE.get_or_init(detect_locale).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    fn chain(pairs: &[(&str, &str)], sys: Option<&str>) -> String {
        detect_from(&env_of(pairs), &|| sys.map(str::to_string))
            .language
            .as_str()
            .to_string()
    }

    #[test]
    fn test_detect_chain_zh_variants() {
        assert_eq!(chain(&[("LC_ALL", "zh_CN.UTF-8")], None), "zh");
        assert_eq!(chain(&[("LC_MESSAGES", "zh_TW")], None), "zh");
        assert_eq!(chain(&[("LANG", "zh_SG.UTF-8")], None), "zh");
        assert_eq!(chain(&[], Some("zh-Hans-CN")), "zh");
    }

    #[test]
    fn test_detect_chain_en_and_unsupported() {
        assert_eq!(chain(&[("LC_ALL", "en_US.UTF-8")], None), "en");
        assert_eq!(chain(&[("LANG", "fr_FR.UTF-8")], None), "en");
        assert_eq!(chain(&[("LC_ALL", "ja_JP.UTF-8")], None), "en");
        assert_eq!(chain(&[("LC_ALL", "C")], None), "en");
        assert_eq!(chain(&[("LC_ALL", "POSIX")], None), "en");
        assert_eq!(chain(&[], None), "en");
        assert_eq!(chain(&[("LC_ALL", "")], Some("garbage@@##")), "en");
    }

    #[test]
    fn test_detect_chain_priority() {
        // LIMITERON_LANG 优先于 LC_ALL
        assert_eq!(
            chain(&[("LIMITERON_LANG", "zh_CN"), ("LC_ALL", "en_US")], None),
            "zh"
        );
        // 不支持的 LIMITERON_LANG 回落链继续（→ LC_ALL）
        assert_eq!(
            chain(&[("LIMITERON_LANG", "fr_FR"), ("LC_ALL", "zh_CN")], None),
            "zh"
        );
        // 空的 LIMITERON_LANG 视为未设置
        assert_eq!(
            chain(&[("LIMITERON_LANG", "  "), ("LC_ALL", "en_US")], None),
            "en"
        );
        // POSIX 链顺序：LC_ALL 胜过 LC_MESSAGES/LANG
        assert_eq!(
            chain(
                &[
                    ("LC_ALL", "en_US"),
                    ("LC_MESSAGES", "zh_CN"),
                    ("LANG", "zh_TW")
                ],
                None
            ),
            "en"
        );
    }

    #[test]
    fn test_normalize_rejects_and_collapses() {
        assert_eq!(normalize("zh_CN.UTF-8").unwrap().language.as_str(), "zh");
        assert_eq!(normalize("en-US@euro").unwrap().language.as_str(), "en");
        assert!(normalize("C").is_none());
        assert!(normalize("POSIX").is_none());
        assert!(normalize("").is_none());
        assert!(normalize("not a locale!!").is_none());
        assert!(
            normalize("fr_FR").is_none(),
            "unsupported → chain continues"
        );
    }

    #[test]
    fn test_set_locale_override_and_clear() {
        // 单测试内顺序完成全部断言：全局 override 态在并行测试下存在竞态
        // （set/clear 非原子），合并为单用例消除交叉污染。
        clear_locale_override();
        set_locale("zh-CN").expect("zh-CN is valid");
        assert_eq!(current_locale().language.as_str(), "zh");
        assert!(set_locale("not-a-valid-locale!!!").is_err());
        clear_locale_override();
        clear_locale_override(); // 重复 clear 安全
    }
}
