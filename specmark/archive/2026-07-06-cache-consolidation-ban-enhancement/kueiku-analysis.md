# kueiku 隐性 bug 分析报告

**方法论**：First Principles + Fishbone（7 维度因果分支）
**置信度**：高（基于静态代码分析，关键路径已逐行验证）

## CRITICAL

1. **[src/cache/ban_storage.rs:259 + src/storage/mod.rs:476] 整数下溢 panic**
   `list_bans` 中 `.take(end.saturating_sub(start).min((total - offset) as usize))` 当 `offset > total` 时，`total - offset` 在 u64 上 debug 模式直接 panic、release 模式 wraparound。攻击者构造 `offset=999999999` 即可打崩服务。
   → 改用 `(total.saturating_sub(offset)) as usize`，并在 `start >= total` 时提前返回空 Vec。

2. **[src/cache/ban_storage.rs:115-140 + src/cache/quota_storage.rs:76-159] 分布式后端 read-modify-write 丢失更新**
   `CacheBanStorage::modify_ban` / `CacheQuotaStorage::consume` 都执行"读-改-写"非原子序列。两个并发 `increment_ban_times` 都读到 ban_times=5，各自写回 6，结果丢一次更新。对于 Redis 等多实例后端，封禁次数与配额计数都会偏低。
   → 使用 backend 提供的原子操作（Redis INCR/MULTI），或在 trait 层暴露 CAS 接口。至少在文档中标注"非线程安全，仅单实例可用"。

3. **[src/quota/controller.rs:533-539] 时钟回退导致配额永久失效**
   `let windows_passed = (elapsed.num_seconds() / window_duration.num_seconds()) as u64;` 当 `state.window_start > now`（NTP 回退）时 `elapsed` 为负 i64，`as u64` 转换 wraparound 成巨大值，导致窗口永远不重置。
   → 检测 `elapsed < 0` 时直接返回原 state 或使用 `now` 作为新 `window_start`。

## HIGH

4. **[src/ban/types.rs:223,590-606] BanManager 无 Drop 实现，auto_unban 任务泄漏**
   BanManager 没有 Drop impl，auto_unban 后台任务句柄被丢弃但任务不被 abort，永久运行。
   → 为 BanManager 实现 Drop，在 drop 中 abort 句柄。

5. **[src/quota/controller.rs:297-325] QuotaController 后台清理任务无 Drop 终止**
   QuotaController 没有 Drop impl 调用 `cancel()`，task 永久等待。
   → 为 QuotaController 实现 Drop 调用 `self.cleanup_token.cancel()`。

6. **[src/quota/controller.rs:660-680] 告警 spawn-fire-and-forget 可无限堆积**
   `send_alert` 中 `tokio::spawn` 不存储 JoinHandle，无背压。Webhook 慢/挂时内存与连接无限增长直到 OOM。
   → 使用 Semaphore 限制并发告警数；或用 mpsc channel + 单一消费者任务。

7. **[src/events/dispatcher.rs:52,131-178] EventDispatcher 无 Drop，task 泄漏 + receiver 被任务持有**
   drop 时 JoinHandle 被丢弃但 task 不 abort；receiver 被移入 task 使 broadcast channel 永不关闭。
   → 为 EventDispatcher 实现 Drop，在 drop 中 abort task。

8. **[src/cache/ban_storage.rs:48,181] `as u32` 截断 ban_times**
   `let ban_times = v.get("ban_times")?.as_u64()? as u32;` 若 ban_times > u32::MAX，`as u32` 静默截断。
   → 用 `u32::try_from(...).map_err(...)?` 显式校验。

9. **[src/cache/quota_storage.rs:86,174] `chrono::Duration::from_std(window).unwrap()` panic 风险**
   生产代码不应有 unwrap 路径。
   → 返回 `Result` 并映射 `chrono::OutOfRangeError`。

10. **[src/admin/handlers.rs:147-151] DELETE /ban 无法解封 MAC/Geo 目标**
    `delete_ban` 只解析 IP/UserId，通过 API 封禁的 MAC/Geo 目标无法通过 API 解封。
    → 扩展 URL 格式或接受 query param `?type=mac`。

## MEDIUM

11. [src/logging/audit.rs:886-890] shutdown 后 write_task 仍可运行（timeout 不 abort）
12. [src/cache/ban_storage.rs:189] `unwrap_or_default()` 静默吞掉无效时间戳
13. [src/ban/file_loader.rs:191] 相对路径 watcher 注册可能失败
14. [src/ban/file_loader.rs:225-231] stop_watching 后立即 start_watching 存在竞态
15. [src/quota/controller.rs:537] `windows_passed.min(i32::MAX as u64) as i32` 截断
16. [src/quota/controller.rs:604-608] `(consumed / limit * 100.0) as u8` 截断与 NaN 风险
17. [src/logging/audit.rs:186] 生产代码 `.expect("HMAC can take key of any size")`

## LOW

18. [src/logging/audit.rs:509] `batch.last().unwrap()` 紧跟 push 之后
19. [src/logging/audit.rs:640,651,654,665] `stem.to_str().unwrap()` 4 处
20. [src/logging/redaction.rs:115-145] 11 处 `Regex::new(...).unwrap()`
21. [src/ban/types.rs:583] 显式 drop RwLock 读锁（良好实践，无需修改）
22. [src/admin/routes.rs:13-22] `constant_time_eq` 长度不等时提前返回（可接受）

## 统计

- CRITICAL: 3, HIGH: 7, MEDIUM: 7, LOW: 5

## 关键观察

1. **"测试通过 ≠ 正确"**：read-modify-write 在单线程测试中 100% 通过，但分布式场景下必然丢更新。
2. **"句柄存储 ≠ 资源释放"**：所有 `tokio::spawn` 都把 JoinHandle 存进 `Arc<RwLock<Option<...>>>`，但 BanManager/QuotaController/EventDispatcher 全部缺少 Drop impl。
3. **"防御性编程 ≠ 显性失败"**：`unwrap_or_default()`、`as u32`、`as u64` 等静默兜底在 cache 层反复出现。

## 修复优先级

1. CRITICAL #1（整数下溢 panic）— 直接修，5 分钟
2. CRITICAL #3（时钟回退）— 直接修，5 分钟
3. CRITICAL #2（非原子 read-modify-write）— 文档标注 + 后续架构改造
4. HIGH #4-7（Drop 泄漏）— 批量补齐 Drop impl
