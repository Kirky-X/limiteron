# Design — cache-consolidation-ban-enhancement

## Context

### 现有架构

```
┌─────────────────────────────────────────────────────────────┐
│                    Limiteron Storage Layer                   │
├─────────────────────────────────────────────────────────────┤
│  Storage trait       BanStorage trait     QuotaStorage trait│
│       │                    │                     │          │
│  ┌────┴────┐          ┌────┴────┐          ┌────┴────┐      │
│  │ Memory  │          │ Memory  │          │ Memory  │      │
│  │ Storage │          │BanStore │          │QuotaStr │      │
│  └────┬────┘          └────┬────┘          └────┬────┘      │
│       │                    │                     │          │
│  ┌────┴────┐          ┌────┴────┐          ┌────┴────┐      │
│  │  Redis  │          │  Redis  │          │  Redis  │      │
│  │ Storage │ ←直连redis│BanStore │ ←直连redis│QuotaStr │ ←直连│
│  │(870LOC) │          │(同文件) │          │(同文件) │      │
│  └─────────┘          └─────────┘          └─────────┘      │
│       │                    │                     │          │
│  ┌────┴────────────────────┴─────────────────────┴────┐     │
│  │  cache-storage feature (oxcache-based)              │     │
│  │  CacheStorage / CacheBanStorage / CacheQuotaStorage│      │
│  │  通过 oxcache::Backend 抽象，可切 memory/redis     │      │
│  └─────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

**问题**：RedisStorage 直连 redis crate，绕过 oxcache 抽象，导致：
- 两套 Redis 实现行为不一致（Lua 脚本 vs oxcache 抽象）
- redis crate 版本被 dbnexus 锁定在 1.2
- 维护成本翻倍

### 约束

- **API 现代化规则**（AGENTS.md）：trait-based + Arc<dyn Trait> + Send+Sync
- **依赖注入**：所有组件支持 new()/builder()/with_dependencies() 三模式
- **Rule 11 惯例优先**：保持现有命名/架构惯例
- **Rule 15 TDD**：每个任务组 Red→Green→Commit→Analyze→Next
- **Rule 17 最新版本**：依赖升级到最新稳定版

## Decision

### D1: 完全移除 redis-storage feature

**删除清单：**
- `src/storage/redis.rs`（870 行）
- `tests/integration/real_storage/redis_storage.rs`
- `tests/integration/real_storage/distributed_consistency.rs`（依赖 RedisStorage）
- `examples/src/bin/redis_storage.rs`

**Cargo.toml 变更：**
- 移除 `redis = { version = "1.2", ... }` workspace 依赖
- 移除 `redis = { workspace = true, optional = true }` 包依赖
- 移除 `redis-storage = ["dep:redis"]` feature
- 从 `full` feature 移除 `redis-storage`
- `cache-storage` feature 保留 `oxcache/redis`，这是唯一的 Redis 路径

**lib.rs 变更：**
- 移除 `#[cfg(feature = "redis-storage")] pub use storage::redis::RedisStorage;`
- 移除 `pub mod redis;` 从 storage/mod.rs

**Redis 后端能力保留**：通过 `cache-storage` feature + `oxcache/redis` + `oxcache::backend::redis::RedisBackend`，用户配置 Redis 后端时使用 `CacheStorage::new(Arc::new(RedisBackend::new(...)))`。

### D2: BanTarget 扩展 Geo 变体

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum BanTarget {
    #[serde(rename = "ip")]
    Ip(String),
    #[serde(rename = "user")]
    UserId(String),
    #[serde(rename = "mac")]
    Mac(String),
    #[serde(rename = "geo")]
    Geo { country_code: String },  // 新增
}
```

**BanPriority 扩展：**
```rust
pub enum BanPriority {
    Ip = 1,
    UserId = 2,
    Mac = 3,
    DeviceId = 4,
    ApiKey = 5,
    Geo = 6,  // 新增，最低优先级（geo 是粗粒度封禁）
}
```

**影响文件全量更新：**
- `src/storage/mod.rs` — BanTarget 定义
- `src/ban/types.rs` — BanPriority, from_target, validate_ban_target
- `src/validation.rs` — validate_ban_target 增加 Geo 分支（验证 ISO 3166-1 alpha-2 国家代码）
- `src/logging/redaction.rs` — redact_ban_target 增加 Geo 分支
- `src/admin/handlers.rs` — delete_ban 路径解析 + 新增 create_ban
- `src/adapters/dbnexus_ban_storage.rs` — match 模式
- `src/cache/ban_storage.rs` — match 模式
- `src/storage/redis.rs` — 即将删除，无需改

### D3: 文件加载 Ban（YAML + 热重载）

**新增模块：** `src/ban/file_loader.rs`

**YAML 格式：**
```yaml
# bans.yaml
bans:
  - target_type: ip
    target_value: "192.168.1.100"
    reason: "恶意请求"
    duration_secs: 3600
    operator: "admin"
  - target_type: geo
    target_value: "CN"  # country_code
    reason: "地区封禁"
    duration_secs: 86400
    operator: "admin"
```

**API：**
```rust
pub struct BanFileLoader {
    path: PathBuf,
    ban_manager: Arc<BanManager>,
    watcher_handle: Option<JoinHandle<()>>,
}

impl BanFileLoader {
    pub fn new(path: impl Into<PathBuf>, ban_manager: Arc<BanManager>) -> Self;
    pub async fn load_once(&self) -> Result<usize, FlowGuardError>;
    pub async fn start_watching(&mut self) -> Result<(), FlowGuardError>;
    pub async fn stop_watching(&mut self);
}
```

**热重载机制：** 复用 `notify` crate（已在 workspace deps），文件变更时触发 `load_once`，使用 `tokio::spawn` 后台监听。

**集成到 BanManager：** 新增 `BanManager::with_file_loader(path)` builder 方法。

### D4: HTTP POST /api/v1/ban 端点

**路由：**
```rust
.route("/api/v1/ban", axum::routing::post(handlers::create_ban))
```

**请求体：**
```rust
#[derive(Deserialize)]
pub struct CreateBanRequest {
    pub target_type: String,      // "ip" | "user" | "mac" | "geo"
    pub target_value: String,     // IP地址 / 用户ID / MAC地址 / 国家代码
    pub reason: String,
    #[serde(default)]
    pub duration_secs: Option<u64>,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}
```

**响应：** `ApiResponse<BanDetailResponse>`，包含创建的 ban 详情。

**Handler 逻辑：**
1. 解析 target_type → BanTarget
2. 构造 BanSource::Manual { operator }
3. 调用 `ban_manager.create_ban(target, reason, source, metadata, duration)`
4. 错误映射：ValidationError → 400, AuthorizationError → 403, 其他 → 500

### D5: 依赖升级策略

**升级原则：** Rule 17 - 最新稳定版本

**关键依赖升级：**
- `tokio`: 1.52 → latest 1.x
- `serde`: 1.0 → latest 1.x
- `sea-orm`: 2.0.0-rc.42 → latest stable（如已发布正式版）
- `sqlx`: 0.9 → latest
- `oxcache`: 0.3 → latest
- `dbnexus`: 0.2 → 0.3（如已发布，解决 oxcache 0.2/0.3 冲突）
- `axum`: 0.8 → latest
- 其他依赖按 cargo update 升级到兼容最新

**版本冲突解决：**
- `redis` 移除后，无 1.2/1.3 冲突
- `dbnexus 0.3`（如可用）依赖 `oxcache 0.3`，与项目一致
- 若 `dbnexus 0.3` 未发布，保持 0.2 并在 Cargo.toml 注释说明

**未使用依赖检测：**
- 运行 `cargo machete` 检测编译时未使用依赖
- 运行 `cargo udeps` 检测未使用代码（如可用）
- 手动审查 feature-gated 依赖是否仍需要

## Alternatives Considered

### A1: 保留 RedisStorage 作为 oxcache 适配层

**方案：** 将 RedisStorage 改写为基于 `oxcache::Backend` 的薄包装，保留类型名。

**为什么没选：**
- 用户明确选择"完全删除"
- oxcache 已提供 `RedisBackend`，无需再包装
- 保留类型名会让用户混淆"何时用 RedisStorage vs CacheStorage"
- 增加维护负担，无实际收益

### A2: Geo ban 使用 metadata 字段

**方案：** BanTarget 不变，geo 信息存入 `BanDetail.metadata` JSON。

**为什么没选：**
- 缺乏类型安全，查询 geo ban 需遍历所有 ban 检查 metadata
- 无法在存储层对 geo 建索引
- 与 Ip/UserId/Mac 的类型安全设计不一致
- 用户明确选择"扩展 BanTarget 枚举"

### A3: 按类型分 HTTP 端点

**方案：** POST /api/v1/ban/ip, POST /api/v1/ban/geo 等。

**为什么没选：**
- 端点冗余，与 RESTful 资源导向设计不符
- 用户明确选择"统一 POST /api/v1/ban"
- 现有 DELETE /api/v1/ban/{target} 也是统一端点

## Consequences

### 正面影响

1. **架构统一**：所有存储后端通过 oxcache 抽象，行为一致
2. **依赖解锁**：redis 版本不再被锁定，可自由升级
3. **类型安全**：Geo ban 与 Ip/UserId/Mac 同等类型安全
4. **运维友好**：文件加载支持黑名单批量导入，热重载无需重启
5. **API 对称**：POST/DELETE /api/v1/ban 完整
6. **质量保障**：95%+ 覆盖率 + diting/tiangang 审计

### 负面影响

1. **Breaking change**：移除 `redis-storage` feature 和 `RedisStorage` 类型，下游需迁移到 `cache-storage` + oxcache
2. **Breaking change**：`BanTarget` 新增 Geo 变体，下游 match 需更新（Rust 编译器会强制）
3. **文件加载复杂性**：新增 file_loader 模块增加约 300 行代码
4. **热重载风险**：文件 watcher 在容器环境可能不工作（需文档说明）

### 技术债

- `dbnexus 0.2` vs `oxcache 0.3` 冲突若未解决，继续在 Cargo.toml 标注
- 分布式限流留给 v0.3.0+ 后续变更

### 后续跟进项

- v0.3.0 发布后，评估 `dbnexus 0.3` 发布状态，如可用则升级
- 文件加载 watcher 在 Kubernetes ConfigMap 挂载场景的兼容性测试
- Geo ban 与 GeoMatcher 的深度集成（按 geo 自动封禁）
