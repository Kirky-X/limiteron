// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Internationalization for limiteron: Fluent message catalog + locale
//! detection + ICU4X-backed formatting.
//!
//! # Layout (unified per change `unify-rust-i18n`)
//!
//! - [`catalog`]: `locales/{en,zh}/messages.ftl` embedded via `include_str!`
//!   into concurrent `FluentBundle`s; `t()`/`translate()` lookup chain
//!   terminates in English.
//! - [`locale`]: detection chain `LIMITERON_LANG` → `LC_ALL` → `LC_MESSAGES`
//!   → `LANG` → `sys-locale` → `en`; only `en`/`zh` are supported.
//! - [`error_ext`]: error dual-track — canonical English `Display` plus
//!   `to_localized_string()` via the catalog (dbnexus pattern).
//! - `i18n_impl` (feature `i18n`): ICU4X DecimalFormatter/PluralRules/
//!   Collator/DateTimeFormatter behind [`LimiterI18nFormatter`].
//!
//! The catalog/locale/error-ext base is always compiled (the error dual
//! track and the CLI/admin exits consume it without feature gating); only
//! the ICU formatter requires the `i18n` feature:
//! ```toml
//! [dependencies]
//! limiteron = { version = "...", features = ["i18n"] }
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use limiteron::i18n::{t, I18nExt};
//!
//! limiteron::i18n::init();
//! let msg = t("rate-limit-exceeded", &[]);
//! let localized = some_error.to_localized_string();
//! ```

use thiserror::Error;

pub mod catalog;
pub mod error_ext;
pub mod locale;

pub use catalog::{t, t_simple, translate, translate_en, translate_for};
pub use error_ext::{I18nExt, LocalizedMsg};
pub use locale::{clear_locale_override, current_locale, detected_locale, init, set_locale};

/// Errors returned by locale handling and (feature `i18n`) formatter
/// operations.
#[derive(Debug, Error)]
pub enum I18nError {
    /// BCP-47 locale string could not be parsed.
    #[error("invalid locale '{input}': {reason}")]
    InvalidLocale { input: String, reason: String },
    /// Number value could not be formatted (e.g. NaN, Infinity, or parse failure).
    #[error("invalid number '{input}': {reason}")]
    InvalidNumber { input: String, reason: String },
    /// Date component out of range or otherwise invalid.
    #[error("date error: {0}")]
    DateError(String),
    /// Underlying ICU4X data or formatting failure.
    #[error("formatting error: {0}")]
    FormatError(String),
}

/// ICU4X-backed formatter module (feature-gated).
#[cfg(feature = "i18n")]
mod i18n_impl;
#[cfg(feature = "i18n")]
pub use i18n_impl::LimiterI18nFormatter;
