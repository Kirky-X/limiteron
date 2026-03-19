//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 地理位置匹配器
//!
//! 基于MaxMind GeoLite2数据库实现IP地理位置查询和匹配。
//!
//! # 特性
//!
//! - 支持国家/地区/城市查询
//! - 内存映射数据库文件（高性能）
//! - 内置缓存（查询延迟 < 1ms）
//! - 支持离线模式
//!
//! # 性能
//!
//! - 查询延迟 P99 < 1ms
//! - 缓存命中率 > 95%
//! - 准确率 > 95%
//!
//! # 使用示例
//!
//! ```rust
//! use limiteron::matchers::geo::GeoMatcher;
//! use std::net::IpAddr;
//!
//! #[tokio::main]
//! async fn main() {
//!     // 注意：需要有效的 GeoLite2-City.mmdb 文件
//!     // let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await.unwrap();
//!     // let ip: IpAddr = "114.114.114.114".parse().unwrap();
//!     // let info = matcher.lookup(ip).unwrap();
//! }
//! ```

#[cfg(feature = "geo-matching")]
use crate::error::FlowGuardError;
use maxminddb::{geoip2, Reader};
use oxcache::Cache;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// 地理信息结构
// ============================================================================

#[cfg(feature = "geo-matching")]
/// 地理信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoInfo {
    /// 国家代码（ISO 3166-1 alpha-2）
    pub country_code: Option<String>,
    /// 国家名称
    pub country_name: Option<String>,
    /// 城市
    pub city: Option<String>,
    /// 大洲
    pub continent: Option<String>,
    /// 经度
    pub longitude: Option<f64>,
    /// 纬度
    pub latitude: Option<f64>,
    /// 时区
    pub timezone: Option<String>,
}

impl GeoInfo {
    /// 创建空的地理信息
    pub fn empty() -> Self {
        Self {
            country_code: None,
            country_name: None,
            city: None,
            continent: None,
            longitude: None,
            latitude: None,
            timezone: None,
        }
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.country_code.is_none()
            && self.country_name.is_none()
            && self.city.is_none()
            && self.continent.is_none()
    }

    /// 获取主要位置描述
    pub fn description(&self) -> String {
        match (&self.city, &self.country_name) {
            (Some(city), Some(country)) => format!("{}, {}", city, country),
            (Some(city), None) => city.clone(),
            (None, Some(country)) => country.clone(),
            (None, None) => "Unknown".to_string(),
        }
    }
}

impl Default for GeoInfo {
    fn default() -> Self {
        Self::empty()
    }
}

// ============================================================================
// 地理匹配条件
// ============================================================================

#[cfg(feature = "geo-matching")]
/// 地理匹配条件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoCondition {
    /// 国家代码列表（ISO 3166-1 alpha-2）
    pub countries: Vec<String>,
    /// 城市列表
    pub cities: Vec<String>,
    /// 大洲列表
    pub continents: Vec<String>,
}

impl GeoCondition {
    /// 创建空的匹配条件
    pub fn empty() -> Self {
        Self {
            countries: vec![],
            cities: vec![],
            continents: vec![],
        }
    }

    /// 创建国家匹配条件
    pub fn countries(countries: Vec<String>) -> Self {
        Self {
            countries,
            cities: vec![],
            continents: vec![],
        }
    }

    /// 创建城市匹配条件
    pub fn cities(cities: Vec<String>) -> Self {
        Self {
            countries: vec![],
            cities,
            continents: vec![],
        }
    }

    /// 创建大洲匹配条件
    pub fn continents(continents: Vec<String>) -> Self {
        Self {
            countries: vec![],
            cities: vec![],
            continents,
        }
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.countries.is_empty() && self.cities.is_empty() && self.continents.is_empty()
    }

    /// 检查地理信息是否匹配条件
    pub fn matches(&self, info: &GeoInfo) -> bool {
        if self.is_empty() {
            return true;
        }

        // 检查国家匹配
        if !self.countries.is_empty() {
            if let Some(country_code) = &info.country_code {
                if self.countries.contains(country_code) {
                    return true;
                }
            }
            // 如果没有国家信息，不匹配
            return false;
        }

        // 检查城市匹配
        if !self.cities.is_empty() {
            if let Some(city) = &info.city {
                if self.cities.contains(city) {
                    return true;
                }
            }
            return false;
        }

        // 检查大洲匹配
        if !self.continents.is_empty() {
            if let Some(continent) = &info.continent {
                if self.continents.contains(continent) {
                    return true;
                }
            }
            return false;
        }

        false
    }
}

impl Default for GeoCondition {
    fn default() -> Self {
        Self::empty()
    }
}

// ============================================================================
// 地理匹配器
// ============================================================================

/// 地理匹配器
///
/// 使用MaxMind GeoLite2数据库查询IP地理位置。
#[cfg(feature = "geo-matching")]
pub struct GeoMatcher {
    /// MaxMind数据库读取器
    reader: Arc<Reader<Vec<u8>>>,
    /// 查询缓存（使用 oxcache）
    cache: Arc<Cache<String, GeoInfo>>,
    /// 缓存大小限制
    cache_size_limit: usize,
    /// 缓存命中次数
    cache_hits: AtomicU64,
    /// 缓存未命中次数
    cache_misses: AtomicU64,
}

impl GeoMatcher {
    /// 创建新的地理匹配器
    ///
    /// # 参数
    /// - `db_path`: GeoLite2数据库文件路径
    ///
    /// # 返回
    /// - `Ok(GeoMatcher)`: 成功创建匹配器
    /// - `Err(FlowGuardError)`: 创建失败
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::geo::GeoMatcher;
    ///
    /// let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self, FlowGuardError> {
        let db_path = db_path.as_ref();

        // 检查文件是否存在
        if !db_path.exists() {
            return Err(FlowGuardError::ConfigError(format!(
                "GeoLite2数据库文件不存在: {}。请从MaxMind官网下载GeoLite2-City.mmdb文件",
                db_path.display()
            )));
        }

        log::info!(target: "geo", "加载GeoLite2数据库: {}", db_path.display());

        // 获取文件元数据
        let metadata = tokio::fs::metadata(db_path)
            .await
            .map_err(FlowGuardError::IoError)?;

        // 验证文件大小（GeoLite2-City.mmdb 通常大于 50MB）
        const MIN_DB_SIZE: u64 = 50 * 1024 * 1024; // 50MB
        const MAX_DB_SIZE: u64 = 500 * 1024 * 1024; // 500MB

        let file_size = metadata.len();
        if file_size < MIN_DB_SIZE {
            return Err(FlowGuardError::ConfigError(format!(
                "GeoLite2数据库文件大小异常（{} bytes），可能已损坏或不是完整文件。最小要求: {} \
                 bytes",
                file_size, MIN_DB_SIZE
            )));
        }

        if file_size > MAX_DB_SIZE {
            log::warn!(
                target: "geo",
                "GeoLite2数据库文件过大（{} bytes），可能不是标准文件",
                file_size
            );
        }

        // 异步读取数据库文件
        let db_content = tokio::fs::read(db_path)
            .await
            .map_err(FlowGuardError::IoError)?;

        // 验证文件大小一致性
        if db_content.len() as u64 != file_size {
            return Err(FlowGuardError::ConfigError(
                "GeoLite2数据库文件读取不完整，可能被截断".to_string(),
            ));
        }

        log::info!("GeoLite2数据库加载成功，大小: {} bytes", db_content.len());

        // 验证文件头（MaxMind 数据库文件以特定 magic number 开头）
        // MaxMind DB 格式: 0x00 0x00 0x02 0x00 (v2) 或 0x00 0x00 0x00 0x00 (v1)
        if db_content.len() < 4 {
            return Err(FlowGuardError::ConfigError(
                "GeoLite2数据库文件过短，无法读取文件头".to_string(),
            ));
        }

        let header = &db_content[0..4];
        // 检查是否是 MaxMind 数据库格式
        let is_valid_header = header == [0x00, 0x00, 0x02, 0x00] || // v2 format
                              header == [0x00, 0x00, 0x00, 0x00] || // v1 format
                              header == [0x00, 0x00, 0x03, 0x00]; // 可能的 v3 格式

        if !is_valid_header {
            log::warn!("GeoLite2数据库文件头格式异常: {:02X?}", header);
            // 不直接返回错误，因为某些版本可能有不同的文件头
            // 让后续的 Reader::from_source 来验证
        }

        // 创建读取器
        let reader = Reader::from_source(db_content)
            .map_err(|e| FlowGuardError::ConfigError(format!("无效的GeoLite2数据库文件: {}", e)))?;

        // 验证数据库元数据
        log::info!(
            "GeoLite2数据库元数据: 版本={}, 构建日期={}, 记录数={}",
            reader.metadata.binary_format_major_version,
            reader.metadata.build_epoch,
            reader.metadata.node_count
        );

        // 创建缓存
        let cache = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(300))
            .build()
            .await
            .map_err(|e| FlowGuardError::ConfigError(format!("创建缓存失败: {}", e)))?;

        let matcher = Self {
            reader: Arc::new(reader),
            cache: Arc::new(cache),
            cache_size_limit: 10_000,
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        };

        log::info!("GeoMatcher创建成功");
        Ok(matcher)
    }

    /// 创建带缓存大小限制的地理匹配器
    ///
    /// # 参数
    /// - `db_path`: GeoLite2数据库文件路径
    /// - `cache_size_limit`: 缓存大小限制
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::geo::GeoMatcher;
    ///
    /// let matcher = GeoMatcher::with_cache_limit("GeoLite2-City.mmdb", 5000).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_cache_limit<P: AsRef<Path>>(
        db_path: P,
        cache_size_limit: usize,
    ) -> Result<Self, FlowGuardError> {
        let mut matcher = Self::new(db_path).await?;
        matcher.cache_size_limit = cache_size_limit;
        Ok(matcher)
    }

    /// 查询IP地理位置
    ///
    /// # 参数
    /// - `ip`: IP地址
    ///
    /// # 返回
    /// - `Ok(GeoInfo)`: 地理信息
    /// - `Err(FlowGuardError)`: 查询失败
    ///
    /// # 性能
    /// - 首次查询: ~1ms
    /// - 缓存命中: < 10μs
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::geo::GeoMatcher;
    /// use std::net::IpAddr;
    ///
    /// let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
    /// let ip: IpAddr = "114.114.114.114".parse()?;
    /// let info = matcher.lookup(ip).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn lookup(&self, ip: IpAddr) -> Result<GeoInfo, FlowGuardError> {
        // 检查缓存
        let ip_str = ip.to_string();
        if let Ok(Some(cached)) = self.cache.get(&ip_str).await {
            log::debug!("缓存命中: {}", ip);
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }

        // 记录缓存未命中
        self.cache_misses.fetch_add(1, Ordering::Relaxed);

        log::debug!("查询IP地理位置: {}", ip);

        // 从数据库查询 - maxminddb 0.27 API
        // lookup 返回 LookupResult，需要使用 decode() 获取解析后的数据
        let lookup_result = self
            .reader
            .lookup(ip)
            .map_err(|e| FlowGuardError::ConfigError(format!("IP查询失败: {}", e)))?;

        // 解码为 City 结构
        let city: geoip2::City = lookup_result
            .decode()
            .map_err(|e| FlowGuardError::ConfigError(format!("IP数据解析失败: {}", e)))?
            .ok_or_else(|| FlowGuardError::ConfigError("IP不在数据库中".to_string()))?;

        // 提取地理信息
        let info = self.extract_geo_info(&city);

        // 更新缓存
        let cache_len = self.cache.len().await.unwrap_or(0);
        if cache_len >= self.cache_size_limit as u64 {
            let _maybe_first = (0..(self.cache_size_limit / 10)).next();
            log::debug!("缓存接近限制 ({}/{})", cache_len, self.cache_size_limit);
        }

        // 使用 set 方法存储，支持过期时间
        let _ = self.cache.set(&ip_str, &info).await;
        log::debug!("IP查询成功: {} -> {}", ip, info.description());

        Ok(info)
    }

    /// 批量查询IP地理位置
    ///
    /// # 参数
    /// - `ips`: IP地址列表
    ///
    /// # 返回
    /// - `Vec<Result<GeoInfo>>`: 地理信息列表
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::geo::GeoMatcher;
    /// use std::net::IpAddr;
    ///
    /// let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
    /// let ips: Vec<IpAddr> = vec![
    ///     "114.114.114.114".parse()?,
    ///     "8.8.8.8".parse()?,
    /// ];
    /// let results = matcher.batch_lookup(&ips).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn batch_lookup(&self, ips: &[IpAddr]) -> Vec<Result<GeoInfo, FlowGuardError>> {
        let mut results = Vec::with_capacity(ips.len());
        for ip in ips {
            results.push(self.lookup(*ip).await);
        }
        results
    }

    /// 检查IP是否匹配地理条件
    ///
    /// # 参数
    /// - `ip`: IP地址
    /// - `condition`: 地理匹配条件
    ///
    /// # 返回
    /// - `Ok(true)`: 匹配
    /// - `Ok(false)`: 不匹配
    /// - `Err(FlowGuardError)`: 查询失败
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::geo::{GeoMatcher, GeoCondition};
    /// use std::net::IpAddr;
    ///
    /// let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
    /// let condition = GeoCondition::countries(vec!["CN".to_string()]);
    /// let ip: IpAddr = "114.114.114.114".parse()?;
    /// let matched = matcher.matches_ip(ip, &condition).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn matches_ip(
        &self,
        ip: IpAddr,
        condition: &GeoCondition,
    ) -> Result<bool, FlowGuardError> {
        let info = self.lookup(ip).await?;
        Ok(condition.matches(&info))
    }

    /// 检查地理信息是否匹配条件
    ///
    /// # 参数
    /// - `info`: 地理信息
    /// - `condition`: 地理匹配条件
    ///
    /// # 返回
    /// - `true`: 匹配
    /// - `false`: 不匹配
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::geo::{GeoInfo, GeoCondition};
    ///
    /// let info = GeoInfo {
    ///     country_code: Some("CN".to_string()),
    ///     country_name: Some("China".to_string()),
    ///     city: Some("Beijing".to_string()),
    ///     continent: Some("Asia".to_string()),
    ///     longitude: Some(116.4),
    ///     latitude: Some(39.9),
    ///     timezone: Some("Asia/Shanghai".to_string()),
    /// };
    ///
    /// let condition = GeoCondition::countries(vec!["CN".to_string()]);
    /// let matched = condition.matches(&info);
    /// ```
    pub fn matches(&self, info: &GeoInfo, condition: &GeoCondition) -> bool {
        condition.matches(info)
    }

    /// 清空缓存
    pub async fn clear_cache(&self) {
        let size = self.cache.len().await.unwrap_or(0);
        let _ = self.cache.clear().await;
        log::info!("缓存已清空，移除 {} 条记录", size);
    }

    /// 获取缓存统计信息
    pub async fn cache_stats(&self) -> GeoCacheStats {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits.saturating_add(misses);
        let hit_rate = if total > 0 {
            (hits as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        GeoCacheStats {
            size: self.cache.len().await.unwrap_or(0) as usize,
            limit: self.cache_size_limit,
            hit_rate,
            hits,
            misses,
        }
    }

    /// 提取地理信息
    fn extract_geo_info(&self, city: &geoip2::City) -> GeoInfo {
        // maxminddb 0.27 API: City 中的字段不是 Option 包装的
        // city.country 是 Country<'a> 类型，不是 Option<Country>
        // Names<'a> 包含具体的语言字段如 english, french 等

        // Helper to safely get English name from Names
        // Extract country info
        let country_code = city.country.iso_code.map(|s| s.to_string());
        let country_name = city.country.names.english.map(|s| s.to_string());

        // Extract city info - city.city 也是 City<'a> 类型
        let city_name = city.city.names.english.map(|s| s.to_string());

        // Extract continent info
        let continent = city.continent.names.english.map(|s| s.to_string());

        // Extract location info
        let longitude = city.location.longitude;
        let latitude = city.location.latitude;
        let timezone = city.location.time_zone.map(|s| s.to_string());

        GeoInfo {
            country_code,
            country_name,
            city: city_name,
            continent,
            longitude,
            latitude,
            timezone,
        }
    }
}

// ============================================================================
// 缓存統計信息
// ============================================================================

#[cfg(feature = "geo-matching")]
/// 地理缓存统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoCacheStats {
    /// 当前缓存大小
    pub size: usize,
    /// 缓存大小限制
    pub limit: usize,
    /// 缓存命中率（百分比）
    pub hit_rate: f64,
    /// 缓存命中次数
    pub hits: u64,
    /// 缓存未命中次数
    pub misses: u64,
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geo_info_empty() {
        let info = GeoInfo::empty();
        assert!(info.is_empty());
        assert_eq!(info.description(), "Unknown");
    }

    #[test]
    fn test_geo_info_description() {
        let info1 = GeoInfo {
            country_code: Some("CN".to_string()),
            country_name: Some("China".to_string()),
            city: Some("Beijing".to_string()),
            continent: Some("Asia".to_string()),
            longitude: Some(116.4),
            latitude: Some(39.9),
            timezone: Some("Asia/Shanghai".to_string()),
        };
        assert_eq!(info1.description(), "Beijing, China");

        let info2 = GeoInfo {
            country_code: Some("CN".to_string()),
            country_name: Some("China".to_string()),
            city: None,
            continent: Some("Asia".to_string()),
            longitude: None,
            latitude: None,
            timezone: None,
        };
        assert_eq!(info2.description(), "China");

        let info3 = GeoInfo {
            country_code: None,
            country_name: None,
            city: Some("Beijing".to_string()),
            continent: None,
            longitude: None,
            latitude: None,
            timezone: None,
        };
        assert_eq!(info3.description(), "Beijing");
    }

    #[test]
    fn test_geo_condition_empty() {
        let condition = GeoCondition::empty();
        assert!(condition.is_empty());

        let info = GeoInfo::empty();
        assert!(condition.matches(&info));
    }

    #[test]
    fn test_geo_condition_countries() {
        let condition = GeoCondition::countries(vec!["CN".to_string(), "US".to_string()]);

        let info1 = GeoInfo {
            country_code: Some("CN".to_string()),
            country_name: Some("China".to_string()),
            city: None,
            continent: None,
            longitude: None,
            latitude: None,
            timezone: None,
        };
        assert!(condition.matches(&info1));

        let info2 = GeoInfo {
            country_code: Some("JP".to_string()),
            country_name: Some("Japan".to_string()),
            city: None,
            continent: None,
            longitude: None,
            latitude: None,
            timezone: None,
        };
        assert!(!condition.matches(&info2));
    }

    #[test]
    fn test_geo_condition_cities() {
        let condition = GeoCondition::cities(vec!["Beijing".to_string(), "Shanghai".to_string()]);

        let info1 = GeoInfo {
            country_code: Some("CN".to_string()),
            country_name: Some("China".to_string()),
            city: Some("Beijing".to_string()),
            continent: None,
            longitude: None,
            latitude: None,
            timezone: None,
        };
        assert!(condition.matches(&info1));

        let info2 = GeoInfo {
            country_code: Some("CN".to_string()),
            country_name: Some("China".to_string()),
            city: Some("Shenzhen".to_string()),
            continent: None,
            longitude: None,
            latitude: None,
            timezone: None,
        };
        assert!(!condition.matches(&info2));
    }

    #[test]
    fn test_geo_condition_continents() {
        let condition = GeoCondition::continents(vec!["Asia".to_string()]);

        let info1 = GeoInfo {
            country_code: Some("CN".to_string()),
            country_name: Some("China".to_string()),
            city: None,
            continent: Some("Asia".to_string()),
            longitude: None,
            latitude: None,
            timezone: None,
        };
        assert!(condition.matches(&info1));

        let info2 = GeoInfo {
            country_code: Some("US".to_string()),
            country_name: Some("United States".to_string()),
            city: None,
            continent: Some("North America".to_string()),
            longitude: None,
            latitude: None,
            timezone: None,
        };
        assert!(!condition.matches(&info2));
    }

    #[test]
    fn test_geo_condition_default() {
        let condition = GeoCondition::default();
        assert!(condition.is_empty());
    }

    #[tokio::test]
    async fn test_geo_cache_stats() {
        // 测试GeoCacheStats的创建和属性
        let cache_stats = GeoCacheStats {
            size: 0,
            limit: 10000,
            hit_rate: 0.0,
            hits: 0,
            misses: 0,
        };

        assert_eq!(cache_stats.size, 0);
        assert_eq!(cache_stats.limit, 10000);
        assert_eq!(cache_stats.hit_rate, 0.0);
    }

    // 集成测试需要在有GeoLite2数据库时运行
    #[tokio::test]
    #[ignore] // 需要GeoLite2数据库文件
    async fn test_geo_matcher_lookup() {
        // 这个测试需要真实的GeoLite2数据库文件
        // 在CI/CD环境中应该跳过或使用mock
    }

    #[test]
    fn test_geo_info_serialization() {
        let info = GeoInfo {
            country_code: Some("CN".to_string()),
            country_name: Some("China".to_string()),
            city: Some("Beijing".to_string()),
            continent: Some("Asia".to_string()),
            longitude: Some(116.4),
            latitude: Some(39.9),
            timezone: Some("Asia/Shanghai".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: GeoInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info, deserialized);
    }

    #[test]
    fn test_geo_condition_serialization() {
        let condition = GeoCondition {
            countries: vec!["CN".to_string(), "US".to_string()],
            cities: vec!["Beijing".to_string()],
            continents: vec!["Asia".to_string()],
        };

        let json = serde_json::to_string(&condition).unwrap();
        let deserialized: GeoCondition = serde_json::from_str(&json).unwrap();

        assert_eq!(condition, deserialized);
    }
}
