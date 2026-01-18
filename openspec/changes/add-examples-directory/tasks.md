## 1. 目录结构设计
- [ ] 1.1 创建 examples 目录作为独立 Rust 项目
- [ ] 1.2 创建 examples/Cargo.toml（独立配置，不引用 workspace）
- [ ] 1.3 设计模块化示例组织结构

## 2. 核心限流器示例
- [ ] 2.1 创建 TokenBucketLimiter 示例 (examples/token_bucket.rs)
- [ ] 2.2 创建 FixedWindowLimiter 示例 (examples/fixed_window.rs)
- [ ] 2.3 创建 SlidingWindowLimiter 示例 (examples/sliding_window.rs)
- [ ] 2.4 创建 ConcurrencyLimiter 示例 (examples/concurrency.rs)

## 3. 流量控制组件示例
- [ ] 3.1 创建 Governor 使用示例 (examples/governor.rs)
- [ ] 3.2 创建 BanManager 使用示例 (examples/ban_manager.rs)
- [ ] 3.3 创建 QuotaController 使用示例 (examples/quota_controller.rs)
- [ ] 3.4 创建 CircuitBreaker 使用示例 (examples/circuit_breaker.rs)

## 4. 标识符和匹配器示例
- [ ] 4.1 创建 Identifier 提取示例 (examples/identifier.rs)
- [ ] 4.2 创建 Matcher 使用示例 (examples/matcher.rs)

## 5. 存储后端示例
- [ ] 5.1 创建 Memory 存储示例 (examples/storage_memory.rs)
- [ ] 5.2 创建 PostgreSQL 存储示例 (examples/storage_postgres.rs)
- [ ] 5.3 创建 Redis 存储示例 (examples/storage_redis.rs)

## 6. 组合功能示例
- [ ] 6.1 创建完整流量控制示例 (examples/full_flow_control.rs)
- [ ] 6.2 创建宏使用示例 (examples/flow_control_macro.rs)
- [ ] 6.3 创建决策链示例 (examples/decision_chain.rs)

## 7. 验证和文档
- [ ] 7.1 验证所有示例可独立编译运行
- [ ] 7.2 为每个示例添加 README 说明
- [ ] 7.3 确保 examples 不被 workspace Cargo.toml 引用
