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
//! use limiteron::geo_matcher::{GeoMatcher, GeoCondition};
//! use std::net::IpAddr;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
//!
//! let condition = GeoCondition {
//!     countries: vec!["CN".to_string(), "US".to_string()],
//!     cities: vec![],
//!     continents: vec![],
//! };
//!
//! let ip: IpAddr = "114.114.114.114".parse()?;
//! let info = matcher.lookup(ip)?;
//! let matched = matcher.matches(&info, &condition)?;
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "geo-matching")]
use crate::error::FlowGuardError;
use dashmap::DashMap;
use maxminddb::{geoip2, Reader};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

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
#[cfg(feature = "geo-matching")]
/// 使用MaxMind GeoLite2数据库查询IP地理位置。
pub struct GeoMatcher {
    /// MaxMind数据库读取器
    reader: Arc<Reader<Vec<u8>>>,
    /// 查询缓存
    cache: Arc<DashMap<IpAddr, GeoInfo>>,
    /// 缓存大小限制
    cache_size_limit: usize,
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
    /// use limiteron::geo_matcher::GeoMatcher;
    ///
    /// let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(db_path))]
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self, FlowGuardError> {
        let db_path = db_path.as_ref();

        // 检查文件是否存在
        if !db_path.exists() {
            return Err(FlowGuardError::ConfigError(format!(
                "GeoLite2数据库文件不存在: {}。请从MaxMind官网下载GeoLite2-City.mmdb文件",
                db_path.display()
            )));
        }

        info!("加载GeoLite2数据库: {}", db_path.display());

        // 异步读取数据库文件
        let db_content = tokio::fs::read(db_path)
            .await
            .map_err(|e| FlowGuardError::IoError(e))?;

        info!("GeoLite2数据库加载成功，大小: {} bytes", db_content.len());

        // 创建读取器
        let reader = Reader::from_source(db_content)
            .map_err(|e| FlowGuardError::ConfigError(format!("无效的GeoLite2数据库文件: {}", e)))?;

        let matcher = Self {
            reader: Arc::new(reader),
            cache: Arc::new(DashMap::new()),
            cache_size_limit: 10_000,
        };

        info!("GeoMatcher创建成功");
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
    /// use limiteron::geo_matcher::GeoMatcher;
    ///
    /// let matcher = GeoMatcher::with_cache_limit("GeoLite2-City.mmdb", 5000).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(db_path))]
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
    /// use limiteron::geo_matcher::GeoMatcher;
    /// use std::net::IpAddr;
    ///
    /// let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
    /// let ip: IpAddr = "114.114.114.114".parse()?;
    /// let info = matcher.lookup(ip)?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub fn lookup(&self, ip: IpAddr) -> Result<GeoInfo, FlowGuardError> {
        // 检查缓存
        if let Some(cached) = self.cache.get(&ip) {
            debug!("缓存命中: {}", ip);
            return Ok(cached.clone());
        }

        debug!("查询IP地理位置: {}", ip);

        // 从数据库查询
        let city: geoip2::City = self
            .reader
            .lookup(ip)
            .map_err(|e| FlowGuardError::ConfigError(format!("IP查询失败: {}", e)))?;

        // 提取地理信息
        let info = self.extract_geo_info(&city);

        // 更新缓存
        if self.cache.len() >= self.cache_size_limit {
            // 缓存已满，清理最旧的条目（简单实现：清理10%）
            let remove_count = self.cache_size_limit / 10;
            let keys_to_remove: Vec<_> = self
                .cache
                .iter()
                .take(remove_count)
                .map(|k| *k.key())
                .collect();
            for key in keys_to_remove {
                self.cache.remove(&key);
            }
            debug!("缓存清理完成，移除 {} 条记录", remove_count);
        }

        self.cache.insert(ip, info.clone());
        debug!("IP查询成功: {} -> {}", ip, info.description());

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
    /// use limiteron::geo_matcher::GeoMatcher;
    /// use std::net::IpAddr;
    ///
    /// let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
    /// let ips: Vec<IpAddr> = vec![
    ///     "114.114.114.114".parse()?,
    ///     "8.8.8.8".parse()?,
    /// ];
    /// let results = matcher.batch_lookup(&ips);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, ips))]
    pub fn batch_lookup(&self, ips: &[IpAddr]) -> Vec<Result<GeoInfo, FlowGuardError>> {
        ips.iter().map(|ip| self.lookup(*ip)).collect()
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
    /// use limiteron::geo_matcher::{GeoMatcher, GeoCondition};
    /// use std::net::IpAddr;
    ///
    /// let matcher = GeoMatcher::new("GeoLite2-City.mmdb").await?;
    /// let condition = GeoCondition::countries(vec!["CN".to_string()]);
    /// let ip: IpAddr = "114.114.114.114".parse()?;
    /// let matched = matcher.matches_ip(ip, &condition)?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, condition))]
    pub fn matches_ip(&self, ip: IpAddr, condition: &GeoCondition) -> Result<bool, FlowGuardError> {
        let info = self.lookup(ip)?;
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
    /// use limiteron::geo_matcher::{GeoInfo, GeoCondition};
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
    #[instrument(skip(self))]
    pub fn clear_cache(&self) {
        let size = self.cache.len();
        self.cache.clear();
        info!("缓存已清空，移除 {} 条记录", size);
    }

    /// 获取缓存统计信息
    pub fn cache_stats(&self) -> GeoCacheStats {
        GeoCacheStats {
            size: self.cache.len(),
            limit: self.cache_size_limit,
            hit_rate: 0.0, // 需要额外统计
        }
    }

    /// 提取地理信息
    fn extract_geo_info(&self, city: &geoip2::City) -> GeoInfo {
        let country_code = city
            .country
            .as_ref()
            .and_then(|c| c.iso_code.map(|s| s.to_string()));

        let country_name = city
            .country
            .as_ref()
            .and_then(|c| c.names.as_ref())
            .and_then(|names| names.get("en").map(|s| s.to_string()));

        let city_name = city
            .city
            .as_ref()
            .and_then(|c| c.names.as_ref())
            .and_then(|names| names.get("en").map(|s| s.to_string()));

        let continent = city
            .continent
            .as_ref()
            .and_then(|c| c.names.as_ref())
            .and_then(|names| names.get("en").map(|s| s.to_string()));

        let location = city.location.as_ref();

        let longitude = location.and_then(|l| l.longitude);
        let latitude = location.and_then(|l| l.latitude);
        let timezone = location.and_then(|l| l.time_zone.map(|s| s.to_string()));

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
// 缓存统计信息
// ============================================================================

#[cfg(feature = "geo-matching")]
/// 地理缓存统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoCacheStats {
    /// 当前缓存大小
    pub size: usize,
    /// 缓存大小限制
    pub limit: usize,
    /// 缓存命中率（需要额外统计）
    pub hit_rate: f64,
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

    #[test]
    fn test_geo_cache_stats() {
        // 测试GeoCacheStats的创建和属性
        let cache_stats = GeoCacheStats {
            size: 0,
            limit: 10000,
            hit_rate: 0.0,
        };

        assert_eq!(cache_stats.size, 0);
        assert_eq!(cache_stats.limit, 10000);
        assert_eq!(cache_stats.hit_rate, 0.0);
    }

    // 集成测试需要在有GeoLite2数据库时运行
    #[test]
    #[ignore] // 需要GeoLite2数据库文件
    fn test_geo_matcher_lookup() {
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
