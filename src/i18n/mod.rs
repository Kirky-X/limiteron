// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! ICU4X-backed internationalization formatting for rate-limit operations.
//!
//! Provides locale-aware number formatting, date formatting, plural rules,
//! and string collation via the `icu` crate (ICU4X 2.x). Useful for
//! generating locale-sensitive rate-limit messages (e.g. "1 request" vs
//! "2 requests"), formatting rate-limit counters, displaying window expiry
//! times, and sorting rate-limit rules by locale-specific collation rules.
//!
//! Enable with the `i18n` cargo feature:
//! ```toml
//! [dependencies]
//! limiteron = { version = "...", features = ["i18n"] }
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use limiteron::i18n::LimiterI18nFormatter;
//!
//! let fmt = LimiterI18nFormatter::new("en-US")?;
//! let msg = fmt.format_rate_limit_message(5, 100, "minute")?;
//! let plural = fmt.format_count(1)?; // "One"
//! let window = fmt.format_window(2026, 7, 11)?;
//! ```

use icu::collator::CollatorBorrowed;
use icu::decimal::DecimalFormatter;
use icu::locale::Locale;
use icu::plurals::PluralRules;
use thiserror::Error;

/// Errors returned by [`LimiterI18nFormatter`] operations.
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

/// Locale-aware formatter backed by ICU4X compiled data.
///
/// Construct with [`LimiterI18nFormatter::new`] using a BCP-47 locale tag
/// (e.g. `"en-US"`, `"zh-CN"`). All formatters are created eagerly so
/// that repeated formatting calls are allocation-light.
pub struct LimiterI18nFormatter {
    locale: Locale,
    decimal_formatter: DecimalFormatter,
    plural_rules: PluralRules,
    collator: CollatorBorrowed<'static>,
}

mod i18n_impl;
