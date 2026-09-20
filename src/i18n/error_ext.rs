// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Error dual-track localization (dbnexus `error_ext` pattern).
//!
//! [`LocalizedMsg`] is implemented by error types to declare their message
//! key and arguments; [`I18nExt`] is blanket-implemented for every error that
//! implements [`LocalizedMsg`], providing [`I18nExt::to_localized_string`]
//! (current locale, English fallback) and [`I18nExt::message_en`]
//! (canonical English via the catalog).
//!
//! The `Display`/`#[error]` string stays the canonical English text; the
//! localized output is an *additional* track consumed by presentation layers
//! (CLI, admin HTTP) — the two are never mixed in one output.

use super::I18nError;
use super::catalog;

/// Trait for error types that have localized messages.
///
/// Each variant returns a unique `message_key()` mapping into the FTL
/// catalog and optionally provides dynamic `$name` arguments.
pub trait LocalizedMsg {
    /// Return the message catalog key for this error variant.
    fn message_key(&self) -> &'static str;

    /// Return the dynamic arguments for template substitution; each entry is
    /// a `(name, value)` pair matching a `{ $name }` placeholder.
    fn message_args(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}

/// Extension trait providing localized string conversion for errors.
///
/// Automatically implemented for all types that implement both
/// [`LocalizedMsg`] and [`std::error::Error`].
pub trait I18nExt: LocalizedMsg + std::error::Error {
    /// Return the error message translated to the current locale
    /// (English fallback when the key is missing).
    fn to_localized_string(&self) -> String {
        catalog::translate(self.message_key(), &self.message_args())
    }

    /// Return the error message in English (the canonical fallback),
    /// regardless of the current locale.
    fn message_en(&self) -> String {
        catalog::translate_en(self.message_key(), &self.message_args())
    }
}

// Blanket implementation for all errors that implement LocalizedMsg.
impl<E: LocalizedMsg + std::error::Error> I18nExt for E {}

/// [`I18nError`] itself participates in the dual track (the 4 already-English
/// variants aligned with catalog keys).
impl LocalizedMsg for I18nError {
    fn message_key(&self) -> &'static str {
        match self {
            I18nError::InvalidLocale { .. } => "i18n-error-invalid-locale",
            I18nError::InvalidNumber { .. } => "i18n-error-invalid-number",
            I18nError::DateError(_) => "i18n-error-date",
            I18nError::FormatError(_) => "i18n-error-format",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            I18nError::InvalidLocale { input, reason }
            | I18nError::InvalidNumber { input, reason } => {
                vec![("input", input.clone()), ("reason", reason.clone())]
            }
            I18nError::DateError(message) | I18nError::FormatError(message) => {
                vec![("message", message.clone())]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i18n_error_dual_track() {
        let err = I18nError::InvalidLocale {
            input: "zh-CN".to_string(),
            reason: "nope".to_string(),
        };
        // Display 恒英文规范串
        assert_eq!(err.to_string(), "invalid locale 'zh-CN': nope");
        // message_en 恒英文（经目录）
        assert_eq!(err.message_en(), "invalid locale 'zh-CN': nope");
    }

    #[test]
    fn test_message_args_default_empty() {
        #[derive(Debug)]
        struct E;
        impl std::fmt::Display for E {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "e")
            }
        }
        impl std::error::Error for E {}
        impl LocalizedMsg for E {
            fn message_key(&self) -> &'static str {
                "rate-limit-exceeded"
            }
        }
        let err = E;
        assert!(err.message_args().is_empty());
        // 默认 locale 链尾为 en（未显式设置时 CI 环境通常是 en；此处仅断言
        // 不 panic 且非空）
        assert!(!err.to_localized_string().is_empty());
        assert_eq!(err.message_en(), "Rate limit exceeded");
    }
}
