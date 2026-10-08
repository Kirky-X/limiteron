// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 可控时钟抽象模块
//!
//! 提供时钟 trait 和实现,支持时间注入用于测试。

#[cfg(any(test, feature = "test-clock"))]
use std::sync::Arc;
#[cfg(any(test, feature = "test-clock"))]
use std::time::Duration;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// 时钟 trait
///
/// 提供时间获取接口,支持注入实现用于测试。
///
/// # 示例
///
/// ```rust
/// use limiteron::{Clock, SystemClock};
///
/// let clock = SystemClock;
/// let now = clock.now();
/// let timestamp = clock.unix_timestamp();
/// ```
pub trait Clock: Send + Sync {
    /// 获取当前 `Instant` 时间
    fn now(&self) -> Instant;

    /// 获取当前 UNIX 时间戳(秒)
    fn unix_timestamp(&self) -> u64;

    /// 获取当前 UNIX 时间戳(纳秒)
    fn unix_timestamp_nanos(&self) -> u64;
}

/// 系统时钟实现
///
/// 使用真实的系统时间,生产环境默认使用此实现。
#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn unix_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn unix_timestamp_nanos(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}

/// 模拟时钟实现
///
/// 用于测试,可以手动控制时间。
///
/// # 示例
///
/// ```rust
/// use limiteron::{Clock, MockClock};
/// use std::time::{Duration, Instant};
///
/// let clock = MockClock::new();
/// let start = clock.now();
///
/// // 前进 10 秒
/// clock.advance(Duration::from_secs(10));
///
/// assert_eq!(clock.now().duration_since(start), Duration::from_secs(10));
/// ```
#[cfg(any(test, feature = "test-clock"))]
pub struct MockClock {
    current_time: parking_lot::RwLock<Instant>,
    /// UNIX 时间（纳秒，u128 防溢出）。单一时钟源：
    /// `unix_timestamp` 取整秒、`unix_timestamp_nanos` 携带亚秒，
    /// 二者与 Instant 永远同步推进。
    ///
    /// 历史教训：曾用「秒级 RwLock + advance 只加 `as_secs()`」，
    /// 亚秒前进被丢弃而 Instant 却完整前移——两个内部时钟在亚秒
    /// 操作后互相发散，且 `unix_timestamp_nanos` 恒为秒×1e9，
    /// 无法测试令牌桶 1ms 门限等亚秒行为。
    unix_nanos: parking_lot::RwLock<u128>,
}

#[cfg(any(test, feature = "test-clock"))]
impl Clone for MockClock {
    fn clone(&self) -> Self {
        Self {
            current_time: parking_lot::RwLock::new(*self.current_time.read()),
            unix_nanos: parking_lot::RwLock::new(*self.unix_nanos.read()),
        }
    }
}

#[cfg(any(test, feature = "test-clock"))]
impl MockClock {
    /// 创建新的模拟时钟,初始时间为当前系统时间
    pub fn new() -> Self {
        Self {
            current_time: parking_lot::RwLock::new(Instant::now()),
            unix_nanos: parking_lot::RwLock::new(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            ),
        }
    }

    /// 创建新的模拟时钟,指定初始时间
    pub fn with_instant(instant: Instant, unix_ts: u64) -> Self {
        Self {
            current_time: parking_lot::RwLock::new(instant),
            unix_nanos: parking_lot::RwLock::new((unix_ts as u128) * 1_000_000_000),
        }
    }

    /// 将时间前进指定时长（亚秒精度同步推进 Instant 与 UNIX 时钟）
    pub fn advance(&self, duration: Duration) {
        {
            let mut time = self.current_time.write();
            *time = time.checked_add(duration).unwrap_or(*time);
        }
        let mut nanos = self.unix_nanos.write();
        *nanos = nanos.saturating_add(duration.as_nanos());
    }

    /// 设置当前时间
    pub fn set_time(&self, instant: Instant, unix_ts: u64) {
        {
            let mut time = self.current_time.write();
            *time = instant;
        }
        let mut nanos = self.unix_nanos.write();
        *nanos = (unix_ts as u128) * 1_000_000_000;
    }

    /// 获取包装为 Arc 的时钟实例
    pub fn as_arc(self) -> Arc<dyn Clock> {
        Arc::new(self)
    }
}

#[cfg(any(test, feature = "test-clock"))]
impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-clock"))]
impl Clock for MockClock {
    fn now(&self) -> Instant {
        *self.current_time.read()
    }

    fn unix_timestamp(&self) -> u64 {
        (*self.unix_nanos.read() / 1_000_000_000) as u64
    }

    fn unix_timestamp_nanos(&self) -> u64 {
        // 与 SystemClock 同口径：u64 截断（饱和），纳秒精度
        u64::try_from(*self.unix_nanos.read()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建系统时钟的 Arc 包装
    fn system_clock() -> Arc<dyn Clock> {
        Arc::new(SystemClock)
    }

    #[test]
    fn test_system_clock_now() {
        let clock = SystemClock;
        let now = clock.now();

        // Instant::now() 应该返回有效时间
        let later = clock.now();
        assert!(later >= now);
    }

    #[test]
    fn test_system_clock_unix_timestamp() {
        let clock = SystemClock;
        let ts = clock.unix_timestamp();

        // 应该是合理的时间戳(2024-2030)
        assert!(ts > 1_700_000_000);
        assert!(ts < 2_000_000_000);
    }

    #[test]
    fn test_system_clock_unix_timestamp_nanos() {
        let clock = SystemClock;
        let ts_nanos = clock.unix_timestamp_nanos();
        let ts_secs = clock.unix_timestamp();

        // 纳秒时间戳应该是秒时间戳的 10^9 倍
        let expected_nanos = ts_secs * 1_000_000_000;
        // 允许 1 秒误差(因为两次调用有时间差)
        assert!(ts_nanos >= expected_nanos);
        assert!(ts_nanos < expected_nanos + 2_000_000_000);
    }

    #[test]
    fn test_mock_clock_advance() {
        let clock = MockClock::new();
        let start = clock.now();

        clock.advance(Duration::from_secs(10));
        let elapsed = clock.now().duration_since(start);

        assert_eq!(elapsed, Duration::from_secs(10));
    }

    #[test]
    fn test_mock_clock_set_time() {
        let clock = MockClock::new();
        let custom_instant = Instant::now() + Duration::from_secs(100);
        let custom_unix_ts = 1_700_000_000;

        clock.set_time(custom_instant, custom_unix_ts);

        assert_eq!(clock.now(), custom_instant);
        assert_eq!(clock.unix_timestamp(), custom_unix_ts);
    }

    #[test]
    fn test_mock_clock_unix_timestamp_nanos() {
        // 整秒基座 + 整秒前进：纳秒与秒级时间戳必须精确一致
        //（亚秒语义由 test_mock_clock_advance_subsecond_consistency 覆盖）
        let clock = MockClock::with_instant(Instant::now(), 1_700_000_000);
        clock.advance(Duration::from_secs(5));

        let ts_secs = clock.unix_timestamp();
        let ts_nanos = clock.unix_timestamp_nanos();

        assert_eq!(ts_secs, 1_700_000_005);
        assert_eq!(ts_nanos, 1_700_000_005_000_000_000);
    }

    #[test]
    fn test_mock_clock_thread_safety() {
        let clock = Arc::new(MockClock::new());

        let mut handles = vec![];
        for _ in 0..10 {
            let clock_clone = Arc::clone(&clock);
            handles.push(std::thread::spawn(move || {
                clock_clone.advance(Duration::from_secs(1));
                clock_clone.now()
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 所有线程完成后,时间应该至少前进了 10 秒
        // 但由于并发,实际值可能更大
        let elapsed = clock.now().duration_since(Instant::now());
        // 这里无法精确验证,因为初始时间未知,但至少验证不会 panic
        let _ = elapsed;
    }

    #[test]
    fn test_system_clock_arc() {
        let clock = system_clock();
        let ts = clock.unix_timestamp();
        assert!(ts > 1_700_000_000);
    }

    #[test]
    fn test_mock_clock_with_instant() {
        let instant = Instant::now();
        let unix_ts = 1_700_000_000;
        let clock = MockClock::with_instant(instant, unix_ts);

        assert_eq!(clock.now(), instant);
        assert_eq!(clock.unix_timestamp(), unix_ts);
    }

    #[test]
    fn test_mock_clock_default() {
        let clock = MockClock::default();
        let ts = clock.unix_timestamp();
        assert!(ts > 1_700_000_000);
    }

    #[test]
    fn test_mock_clock_clone() {
        let clock = MockClock::new();
        clock.advance(Duration::from_secs(42));
        let cloned = clock.clone();
        assert_eq!(cloned.unix_timestamp(), clock.unix_timestamp());
        assert_eq!(clock.now(), cloned.now());
    }

    #[test]
    fn test_mock_clock_as_arc() {
        let clock = MockClock::new();
        let arc_clock: Arc<dyn Clock> = clock.as_arc();
        let ts = arc_clock.unix_timestamp();
        assert!(ts > 1_700_000_000);
        let now = arc_clock.now();
        assert!(now.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn test_mock_clock_clone_independent() {
        let clock = MockClock::new();
        clock.advance(Duration::from_secs(10));
        let cloned = clock.clone();
        clock.advance(Duration::from_secs(20));
        assert_eq!(clock.unix_timestamp(), cloned.unix_timestamp() + 20);
    }

    #[test]
    fn test_system_clock_copy_and_clone() {
        let clock1 = SystemClock;
        let clock2 = clock1;
        let ts1 = clock1.unix_timestamp();
        let ts2 = clock2.unix_timestamp();
        assert!(ts1 <= ts2);
    }

    #[test]
    fn test_mock_clock_advance_then_nanos() {
        let clock = MockClock::with_instant(Instant::now(), 100);
        clock.advance(Duration::from_secs(5));
        let nanos = clock.unix_timestamp_nanos();
        assert_eq!(nanos, 105 * 1_000_000_000);
    }

    #[test]
    fn test_mock_clock_advance_subsecond_consistency() {
        // 亚秒一致性回归：advance 的亚秒部分曾只作用于 Instant，
        // UNIX 时间戳丢弃亚秒（as_secs()）且 unix_timestamp_nanos 恒为秒×1e9
        // ——两钟发散、亚秒行为不可测。
        let clock = MockClock::with_instant(Instant::now(), 1_000);
        let start = clock.now();

        clock.advance(Duration::from_millis(900));
        assert_eq!(clock.unix_timestamp(), 1_000, "亚秒前进不得进位秒");
        assert_eq!(
            clock.unix_timestamp_nanos(),
            1_000 * 1_000_000_000 + 900_000_000,
            "unix_timestamp_nanos 应携带亚秒部分"
        );

        clock.advance(Duration::from_millis(600)); // 累计 1.5s
        assert_eq!(clock.unix_timestamp(), 1_001, "跨秒进位");
        assert_eq!(
            clock.unix_timestamp_nanos(),
            1_000 * 1_000_000_000 + 1_500_000_000
        );

        // Instant 与 UNIX 时钟必须同步前进（同刻度）
        assert_eq!(
            clock.now().duration_since(start),
            Duration::from_millis(1500)
        );
    }

    #[test]
    fn test_mock_clock_set_time_subsecond_zeroed() {
        // set_time 以秒为 API 口径，纳秒归零（语义与 with_instant 一致）
        let clock = MockClock::new();
        clock.set_time(Instant::now(), 1_700_000_000);
        assert_eq!(
            clock.unix_timestamp_nanos(),
            1_700_000_000u64 * 1_000_000_000
        );
    }
}
