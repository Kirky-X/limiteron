// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT

use super::*;
use std::cmp::Ordering;
use std::str::FromStr;

use icu::collator::Collator;
use icu::collator::options::CollatorOptions;
use icu::datetime::DateTimeFormatter;
use icu::datetime::fieldsets::YMD;
use icu::datetime::input::{Date, DateTime, Time};
use icu::decimal::DecimalFormatter;
use icu::decimal::input::Decimal;
use icu::decimal::options::DecimalFormatterOptions;
use icu::locale::Locale;
use icu::plurals::{PluralCategory, PluralRules, PluralRulesOptions};
use writeable::Writeable;

/// Map a [`PluralCategory`] to its capitalized CLDR name (e.g. `"One"`, `"Other"`).
fn plural_category_name(category: PluralCategory) -> &'static str {
    match category {
        PluralCategory::Zero => "Zero",
        PluralCategory::One => "One",
        PluralCategory::Two => "Two",
        PluralCategory::Few => "Few",
        PluralCategory::Many => "Many",
        PluralCategory::Other => "Other",
    }
}

impl LimiterI18nFormatter {
    /// Create a new formatter for the given BCP-47 locale tag.
    ///
    /// # Errors
    /// Returns [`I18nError::InvalidLocale`] if the tag cannot be parsed,
    /// or [`I18nError::FormatError`] if ICU4X lacks compiled data for it.
    pub fn new(locale: &str) -> Result<Self, I18nError> {
        let parsed = Locale::from_str(locale).map_err(|e| I18nError::InvalidLocale {
            input: locale.to_string(),
            reason: e.to_string(),
        })?;

        let decimal_formatter =
            DecimalFormatter::try_new(parsed.clone().into(), DecimalFormatterOptions::default())
                .map_err(|e| I18nError::FormatError(e.to_string()))?;

        let plural_rules =
            PluralRules::try_new(parsed.clone().into(), PluralRulesOptions::default())
                .map_err(|e| I18nError::FormatError(e.to_string()))?;

        let collator = Collator::try_new(parsed.clone().into(), CollatorOptions::default())
            .map_err(|e| I18nError::FormatError(e.to_string()))?;

        Ok(Self {
            locale: parsed,
            decimal_formatter,
            plural_rules,
            collator,
        })
    }

    /// Format a floating-point number with locale-sensitive grouping
    /// and decimal separators.
    ///
    /// # Errors
    /// Returns [`I18nError::InvalidNumber`] for non-finite values or
    /// if the value cannot be parsed into a fixed decimal.
    pub fn format_number(&self, value: f64) -> Result<String, I18nError> {
        if !value.is_finite() {
            return Err(I18nError::InvalidNumber {
                input: value.to_string(),
                reason: "value is not finite (NaN or Infinity)".into(),
            });
        }
        let repr = format!("{value}");
        let decimal = Decimal::from_str(&repr).map_err(|e| I18nError::InvalidNumber {
            input: repr,
            reason: e.to_string(),
        })?;
        let formatted = self.decimal_formatter.format(&decimal);
        Ok(formatted.write_to_string().into_owned())
    }

    /// Return the plural category name for `count` in the formatter's locale
    /// (e.g. `"One"` for English count=1, `"Other"` for count=2).
    ///
    /// Use this to build locale-aware rate-limit messages such as
    /// "1 request" vs "2 requests".
    ///
    /// # Errors
    /// This method does not currently fail, but returns `Result` for API
    /// consistency with the other formatting methods.
    pub fn format_count(&self, count: u64) -> Result<String, I18nError> {
        Ok(plural_category_name(self.plural_rules.category_for(count)).to_string())
    }

    /// Build a locale-aware rate-limit message combining the current
    /// `count`, the configured `limit`, and a human-readable `window`
    /// (e.g. `"minute"`, `"hour"`). Counters are formatted with the
    /// locale's grouping/decimal separators.
    ///
    /// # Errors
    /// Returns [`I18nError::InvalidNumber`] if either counter cannot be
    /// formatted.
    pub fn format_rate_limit_message(
        &self,
        count: u64,
        limit: u64,
        window: &str,
    ) -> Result<String, I18nError> {
        let count_str = self.format_number(count as f64)?;
        let limit_str = self.format_number(limit as f64)?;
        Ok(format!(
            "Rate limit exceeded: {count_str}/{limit_str} requests per {window}"
        ))
    }

    /// Format an ISO calendar date (year / month / day) as a rate-limit
    /// window timestamp using a medium-length locale-specific pattern.
    ///
    /// # Errors
    /// Returns [`I18nError::DateError`] if any component is out of range,
    /// or [`I18nError::FormatError`] if the formatter cannot be constructed.
    pub fn format_window(&self, year: i32, month: u8, day: u8) -> Result<String, I18nError> {
        let date =
            Date::try_new_iso(year, month, day).map_err(|e| I18nError::DateError(e.to_string()))?;
        let time = Time::try_new(0, 0, 0, 0).map_err(|e| I18nError::DateError(e.to_string()))?;
        let datetime = DateTime { date, time };

        let dtf = DateTimeFormatter::try_new(self.locale.clone().into(), YMD::medium())
            .map_err(|e| I18nError::FormatError(e.to_string()))?;
        let formatted = dtf.format(&datetime);
        Ok(formatted.write_to_string().into_owned())
    }

    /// Compare two rate-limit rule names using locale-sensitive collation
    /// rules.
    ///
    /// # Errors
    /// This method does not currently fail, but returns `Result` for API
    /// consistency with the other formatting methods.
    pub fn compare_rules(&self, a: &str, b: &str) -> Result<Ordering, I18nError> {
        Ok(self.collator.compare(a, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_parsing_en() {
        let fmt = LimiterI18nFormatter::new("en-US");
        assert!(fmt.is_ok(), "en-US should parse successfully");
    }

    #[test]
    fn test_locale_parsing_zh() {
        let fmt = LimiterI18nFormatter::new("zh-CN");
        assert!(fmt.is_ok(), "zh-CN should parse successfully");
    }

    #[test]
    fn test_invalid_locale() {
        let result = LimiterI18nFormatter::new("not-a-valid-locale!!!");
        assert!(result.is_err(), "invalid locale should return error");
        match result.err().unwrap() {
            I18nError::InvalidLocale { input, .. } => assert_eq!(input, "not-a-valid-locale!!!"),
            other => panic!("expected InvalidLocale, got {other:?}"),
        }
    }

    #[test]
    fn test_format_count() {
        let fmt = LimiterI18nFormatter::new("en").expect("en locale");
        assert_eq!(
            fmt.format_count(1).expect("plural 1"),
            "One",
            "en: count=1 should be One"
        );
        assert_eq!(
            fmt.format_count(2).expect("plural 2"),
            "Other",
            "en: count=2 should be Other"
        );
    }

    #[test]
    fn test_format_number_en() {
        let fmt = LimiterI18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_number(1_234_567.89_f64).expect("format number");
        // en-US: thousands separator is comma, decimal separator is period
        assert!(
            result.contains(','),
            "en-US number should contain thousands separator: got '{result}'"
        );
        assert!(
            result.contains('.'),
            "en-US number should contain decimal point: got '{result}'"
        );
    }

    #[test]
    fn test_format_number_not_finite() {
        let fmt = LimiterI18nFormatter::new("en-US").expect("en-US locale");
        assert!(fmt.format_number(f64::NAN).is_err());
        assert!(fmt.format_number(f64::INFINITY).is_err());
    }

    #[test]
    fn test_format_rate_limit_message() {
        let fmt = LimiterI18nFormatter::new("en-US").expect("en-US locale");
        let msg = fmt
            .format_rate_limit_message(5, 100, "minute")
            .expect("rate limit message");
        assert!(
            msg.contains("5"),
            "message should contain count: got '{msg}'"
        );
        assert!(
            msg.contains("100"),
            "message should contain limit: got '{msg}'"
        );
        assert!(
            msg.contains("minute"),
            "message should contain window: got '{msg}'"
        );
        assert!(
            msg.contains("Rate limit exceeded"),
            "message should contain prefix: got '{msg}'"
        );
    }

    #[test]
    fn test_compare_rules() {
        let fmt = LimiterI18nFormatter::new("en").expect("en locale");
        assert_eq!(
            fmt.compare_rules("apple", "banana").expect("compare"),
            Ordering::Less,
            "apple < banana"
        );
        assert_eq!(
            fmt.compare_rules("banana", "apple").expect("compare"),
            Ordering::Greater,
            "banana > apple"
        );
        assert_eq!(
            fmt.compare_rules("apple", "apple").expect("compare"),
            Ordering::Equal,
            "apple == apple"
        );
    }

    #[test]
    fn test_format_window() {
        let fmt = LimiterI18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_window(2026, 7, 11).expect("format window");
        assert!(
            result.contains("2026"),
            "window should contain year: got '{result}'"
        );
        assert!(
            !result.is_empty(),
            "window should be non-empty: got '{result}'"
        );
    }
}
