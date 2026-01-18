## ADDED Requirements

### Requirement: Examples Directory Structure
项目 SHALL 包含一个独立的 `examples/` 目录作为独立的 Rust 项目，不属于 workspace 成员。

#### Scenario: 独立项目结构
- **WHEN** 用户检查项目结构
- **THEN** 可以看到独立的 `examples/` 目录
- **AND** `examples/` 目录包含自己的 `Cargo.toml`
- **AND** 主项目 `Cargo.toml` 不包含 `examples` 路径引用

### Requirement: Token Bucket Limiter Example
项目 SHALL 提供令牌桶限流器的使用示例。

#### Scenario: 令牌桶基本使用
- **WHEN** 用户运行 token_bucket.rs 示例
- **THEN** 演示令牌桶的创建、令牌补充、请求检查流程

### Requirement: Fixed Window Limiter Example
项目 SHALL 提供固定窗口限流器的使用示例。

#### Scenario: 固定窗口基本使用
- **WHEN** 用户运行 fixed_window.rs 示例
- **THEN** 演示固定窗口限流的创建和周期重置行为

### Requirement: Sliding Window Limiter Example
项目 SHALL 提供滑动窗口限流器的使用示例。

#### Scenario: 滑动窗口基本使用
- **WHEN** 用户运行 sliding_window.rs 示例
- **THEN** 演示滑动窗口限流的时间滑动特性和精确计数

### Requirement: Concurrency Limiter Example
项目 SHALL 提供并发控制限流器的使用示例。

#### Scenario: 并发控制基本使用
- **WHEN** 用户运行 concurrency.rs 示例
- **THEN** 演示并发连接数限制和等待队列机制

### Requirement: Governor Example
项目 SHALL 提供 Governor 流量控制器的使用示例。

#### Scenario: Governor 决策链
- **WHEN** 用户运行 governor.rs 示例
- **THEN** 演示 Governor 如何协调多个限流组件进行决策

### Requirement: Ban Manager Example
项目 SHALL 提供封禁管理器的使用示例。

#### Scenario: IP 封禁管理
- **WHEN** 用户运行 ban_manager.rs 示例
- **THEN** 演示封禁查询、添加、移除和过期处理

### Requirement: Quota Controller Example
项目 SHALL 提供配额控制器的使用示例。

#### Scenario: 配额分配和监控
- **WHEN** 用户运行 quota_controller.rs 示例
- **THEN** 演示配额分配、消耗追踪、告警和透支机制

### Requirement: Circuit Breaker Example
项目 SHALL 提供熔断器的使用示例。

#### Scenario: 熔断状态转换
- **WHEN** 用户运行 circuit_breaker.rs 示例
- **THEN** 演示熔断器 Closed/HalfOpen/Open 状态转换

### Requirement: Identifier Extraction Example
项目 SHALL 提供标识符提取的使用示例。

#### Scenario: 多类型标识符提取
- **WHEN** 用户运行 identifier.rs 示例
- **THEN** 演示从请求中提取 UserId、IP、ApiKey、DeviceId 等标识符

### Requirement: Matcher Example
项目 SHALL 提供规则匹配器的使用示例。

#### Scenario: 复合条件匹配
- **WHEN** 用户运行 matcher.rs 示例
- **THEN** 演示多条件组合规则和优先级匹配

### Requirement: Storage Backend Examples
项目 SHALL 提供所有存储后端的使用示例。

#### Scenario: Memory 存储
- **WHEN** 用户运行 storage_memory.rs 示例
- **THEN** 演示内存存储的使用方式

#### Scenario: PostgreSQL 存储
- **WHEN** 用户运行 storage_postgres.rs 示例
- **THEN** 演示 PostgreSQL 存储后端的配置和操作

#### Scenario: Redis 存储
- **WHEN** 用户运行 storage_redis.rs 示例
- **THEN** 演示 Redis 存储后端的配置和操作

### Requirement: Full Flow Control Example
项目 SHALL 提供完整的流量控制集成示例。

#### Scenario: 端到端流量控制
- **WHEN** 用户运行 full_flow_control.rs 示例
- **THEN** 演示所有组件协同工作的完整流程

### Requirement: Flow Control Macro Example
项目 SHALL 提供过程宏的使用示例。

#### Scenario: 声明式限流
- **WHEN** 用户运行 flow_control_macro.rs 示例
- **THEN** 演示 `#[flow_control]` 宏的声明式使用方式

### Requirement: Decision Chain Example
项目 SHALL 提供决策链的使用示例。

#### Scenario: 自定义决策顺序
- **WHEN** 用户运行 decision_chain.rs 示例
- **THEN** 演示决策链的配置和自定义顺序
