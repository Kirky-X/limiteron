# Spec — ban-file-loader

> Delta spec for change `cache-consolidation-ban-enhancement`. 覆盖此变更引入/修改的文件加载 ban 能力域需求。

## Requirements

### R-ban-file-loader-001: YAML 文件格式定义

文件加载 ban 使用 YAML 格式，与项目 config 一致。文件结构包含 `bans` 数组，每条 ban 含 target_type/target_value/reason/duration_secs/operator。

**验收标准：**
- YAML 文件示例：
  ```yaml
  bans:
    - target_type: ip
      target_value: "192.168.1.100"
      reason: "恶意请求"
      duration_secs: 3600
      operator: "admin"
    - target_type: geo
      target_value: "CN"
      reason: "地区封禁"
      duration_secs: 86400
      operator: "admin"
  ```
- `BanFileLoader::load_once` 能解析上述 YAML 并返回 2（ban 数量）

### R-ban-file-loader-002: BanFileLoader API

`BanFileLoader` 提供 new/load_once/start_watching/stop_watching 四个方法。

**验收标准：**
- `BanFileLoader::new(path: impl Into<PathBuf>, ban_manager: Arc<BanManager>) -> Self`
- `BanFileLoader::load_once(&self) -> Result<usize, FlowGuardError>` 返回加载的 ban 数量
- `BanFileLoader::start_watching(&mut self) -> Result<(), FlowGuardError>` 启动文件监听
- `BanFileLoader::stop_watching(&mut self)` 停止文件监听
- 单元测试覆盖每个方法的成功和失败路径

### R-ban-file-loader-003: 热重载机制

文件变更时自动触发 `load_once`，使用 `notify` crate 监听文件系统事件。

**验收标准：**
- 修改 YAML 文件后 ≤ 2 秒内触发重载
- 重载时不清除已有 ban（追加模式，重复 ban 由 BanManager 去重）
- 文件删除时不触发崩溃，仅记录警告日志
- 单元测试覆盖热重载触发（使用 tempdir 模拟）

### R-ban-file-loader-004: 集成到 BanManager

`BanManager` 新增 `with_file_loader(path)` builder 方法，启动时加载 + 文件变更重载。

**验收标准：**
- `BanManager::with_file_loader(path)` 返回 `(BanManager, BanFileLoader)` 或类似结构
- 启动时自动调用 `load_once`，失败时返回 Err
- 文件变更时通过 background task 自动重载
- 集成测试覆盖端到端流程

### R-ban-file-loader-005: 错误处理

文件加载错误必须显性化（Rule 12），不吞掉错误。

**验收标准：**
- YAML 解析错误返回 `FlowGuardError::ConfigError` 含行号和字段
- 文件不存在返回 `FlowGuardError::FileNotFound`
- 权限不足返回 `FlowGuardError::PermissionDenied`
- target_value 验证失败返回 `FlowGuardError::ValidationError` 含具体字段
- 跳过的 ban 数量和原因在返回值中明示（不埋在日志里）

## Constraints

- 复用现有 `notify` crate（不引入新依赖）
- 热重载使用 `tokio::spawn` 后台监听
- 文件路径支持相对路径和绝对路径
- 容器环境（Kubernetes ConfigMap）兼容性在文档中说明

## Out of Scope

- 不支持 TOML/JSON 格式（仅 YAML）
- 不实现文件锁机制
- 不实现分布式文件同步
- 不实现 ban 过期后自动从文件移除
