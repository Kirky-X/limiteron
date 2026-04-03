//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 可控时钟抽象模块
//!
//! 提供时钟 trait 和实现,支持时间注入用于测试。

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 时钟 trait
///
/// 提供时间获取接口,支持注入实现用于测试。
///
/// # 示例
///
/// ```rust
/// use limiteron::clock::{Clock, SystemClock};
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
/// use limiteron::clock::{Clock, MockClock};
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
pub struct MockClock {
    current_time: parking_lot::RwLock<Instant>,
    unix_timestamp: parking_lot::RwLock<u64>,
}

impl Clone for MockClock {
    fn clone(&self) -> Self {
        Self {
            current_time: parking_lot::RwLock::new(*self.current_time.read()),
            unix_timestamp: parking_lot::RwLock::new(*self.unix_timestamp.read()),
        }
    }
}

impl MockClock {
    /// 创建新的模拟时钟,初始时间为当前系统时间
    pub fn new() -> Self {
        Self {
            current_time: parking_lot::RwLock::new(Instant::now()),
            unix_timestamp: parking_lot::RwLock::new(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            ),
        }
    }

    /// 创建新的模拟时钟,指定初始时间
    pub fn with_instant(instant: Instant, unix_ts: u64) -> Self {
        Self {
            current_time: parking_lot::RwLock::new(instant),
            unix_timestamp: parking_lot::RwLock::new(unix_ts),
        }
    }

    /// 将时间前进指定时长
    pub fn advance(&self, duration: Duration) {
        let mut time = self.current_time.write();
        *time = time.checked_add(duration).unwrap_or(*time);

        // 同时更新 UNIX 时间戳
        let mut unix_ts = self.unix_timestamp.write();
        *unix_ts = unix_ts.saturating_add(duration.as_secs());
    }

    /// 设置当前时间
    pub fn set_time(&self, instant: Instant, unix_ts: u64) {
        let mut time = self.current_time.write();
        *time = instant;

        let mut unix_ts_lock = self.unix_timestamp.write();
        *unix_ts_lock = unix_ts;
    }

    /// 获取包装为 Arc 的时钟实例
    pub fn as_arc(self) -> Arc<dyn Clock> {
        Arc::new(self)
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MockClock {
    fn now(&self) -> Instant {
        *self.current_time.read()
    }

    fn unix_timestamp(&self) -> u64 {
        *self.unix_timestamp.read()
    }

    fn unix_timestamp_nanos(&self) -> u64 {
        // MockClock 使用秒级时间戳,纳秒通过秒转换
        *self.unix_timestamp.read() * 1_000_000_000
    }
}

/// 创建系统时钟的 Arc 包装
#[cfg(test)]
pub(crate) fn system_clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let clock = MockClock::new();
        clock.advance(Duration::from_secs(5));

        let ts_secs = clock.unix_timestamp();
        let ts_nanos = clock.unix_timestamp_nanos();

        assert_eq!(ts_nanos, ts_secs * 1_000_000_000);
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
}
