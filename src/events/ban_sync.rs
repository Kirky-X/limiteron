// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 封禁跨实例同步：经 oxcache Pub/Sub 语义广播封禁变更。
//!
//! 复用 oxcache 跨实例失效总线的**协议层抽象**
//! （`oxcache::invalidation::PubSubTransport`）：生产为 Redis Pub/Sub，
//! 测试/进程内为 `InMemoryPubSubTransport`（mock 协议层，语义与 Redis
//! 一致：订阅后发布的消息广播给全部订阅者，无离线补投）。
//!
//! - 本实例封禁/解封 → [`BanSyncBus::publish`] 广播信封；
//! - [`BanSyncBus::spawn_listener`] 订阅通道，把**其他实例**的封禁事件
//!   应用到本地（自身消息按 `instance_id` 豁免，防回环）；
//! - 可靠性兜底：Pub/Sub 为 at-most-once，重要封禁同时落 outbox
//!   （持久化兜底），本组件承担实时面。
//!
//! # Example
//!
//! ```rust,ignore
//! use limiteron::events::ban_sync::{BanSyncBus, BanSyncConfig};
//! use oxcache::invalidation::{InMemoryPubSubTransport, RedisPubSubTransport};
//!
//! let transport = std::sync::Arc::new(RedisPubSubTransport::new("redis://…").await?);
//! let bus = BanSyncBus::new(transport.clone(), BanSyncConfig::new("instance-a"));
//! bus.publish(BanSyncMessage::ban_applied("inst-a", "ip:192.0.2.1", "abuse", None)).await?;
//! bus.spawn_listener(Arc::new(local_ban_applier)).await?;
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use oxcache::invalidation::PubSubTransport;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::error::{LimiteronError, StorageError};

/// 默认封禁同步通道
pub const DEFAULT_BAN_SYNC_CHANNEL: &str = "limiteron:ban-sync";

/// 封禁同步消息类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BanSyncKind {
    /// 封禁生效
    BanApplied,
    /// 封禁解除
    BanRemoved,
}

/// 封禁同步信封（Pub/Sub wire 格式为 JSON）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BanSyncMessage {
    /// 消息类型
    pub kind: BanSyncKind,
    /// 封禁目标 key（`ip:1.2.3.4` / `user:u1` / `cidr:10.0.0.0/8`，与
    /// outbox `aggregate_id` 同源）
    pub target_key: String,
    /// 封禁原因（解封消息可为空串）
    #[serde(default)]
    pub reason: String,
    /// 到期时间（Unix 秒；None = 由本地策略决定）
    pub expires_at_epoch: Option<i64>,
    /// 发起实例 ID（自消息豁免依据）
    pub origin: String,
}

impl BanSyncMessage {
    /// 构造封禁生效消息
    pub fn ban_applied(
        origin: &str,
        target_key: &str,
        reason: &str,
        expires_at_epoch: Option<i64>,
    ) -> Self {
        Self {
            kind: BanSyncKind::BanApplied,
            target_key: target_key.to_string(),
            reason: reason.to_string(),
            expires_at_epoch,
            origin: origin.to_string(),
        }
    }

    /// 构造解封消息
    pub fn ban_removed(origin: &str, target_key: &str) -> Self {
        Self {
            kind: BanSyncKind::BanRemoved,
            target_key: target_key.to_string(),
            reason: String::new(),
            expires_at_epoch: None,
            origin: origin.to_string(),
        }
    }

    /// 序列化为 wire 载荷
    pub fn encode(&self) -> Result<String, LimiteronError> {
        serde_json::to_string(self)
            .map_err(|e| StorageError::QueryError(format!("ban-sync encode: {e}")).into())
    }

    /// 从 wire 载荷解码
    pub fn decode(payload: &str) -> Result<Self, LimiteronError> {
        serde_json::from_str(payload)
            .map_err(|e| StorageError::QueryError(format!("ban-sync decode: {e}")).into())
    }
}

/// 总线配置
#[derive(Debug, Clone)]
pub struct BanSyncConfig {
    /// Pub/Sub 通道名
    pub channel: String,
    /// 本实例 ID（自消息豁免）
    pub instance_id: String,
}

impl BanSyncConfig {
    /// 以默认通道创建配置
    pub fn new(instance_id: impl Into<String>) -> Self {
        Self {
            channel: DEFAULT_BAN_SYNC_CHANNEL.to_string(),
            instance_id: instance_id.into(),
        }
    }
}

/// 本地封禁应用端口：监听端把远端封禁事件落到本实例（BanManager / 等）
#[async_trait::async_trait]
pub trait BanSyncApplier: Send + Sync {
    /// 远端封禁生效
    async fn on_remote_ban(&self, msg: &BanSyncMessage);

    /// 远端解封
    async fn on_remote_unban(&self, msg: &BanSyncMessage);
}

/// 监听任务句柄
pub struct BanSyncListenerHandle {
    join: JoinHandle<()>,
    stop: Arc<AtomicBool>,
}

impl BanSyncListenerHandle {
    /// 请求停止（任务在下一条消息或轮询间隙退出）
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// 等待任务退出
    pub async fn join(self) {
        let _ = self.join.await;
    }
}

/// 封禁跨实例同步总线
/// 监听断线重连的退避区间
#[cfg(feature = "ban-sync")]
const LISTENER_RECONNECT_MIN: std::time::Duration = std::time::Duration::from_millis(200);
#[cfg(feature = "ban-sync")]
const LISTENER_RECONNECT_MAX: std::time::Duration = std::time::Duration::from_secs(10);

/// 断线重连：带指数退避的重订阅，直至成功或停止。
///
/// 抽为独立函数以便确定性测试「N 次故障后恢复」——订阅端关闭的
/// recv()=None 路径在真实传输中无法从外部触发。
#[cfg(feature = "ban-sync")]
async fn resubscribe_with_backoff(
    transport: &Arc<dyn PubSubTransport>,
    channel: &str,
    backoff: &mut std::time::Duration,
    stop_flag: &AtomicBool,
) -> Option<oxcache::invalidation::SubscriptionReceiver> {
    loop {
        tokio::time::sleep(*backoff).await;
        if stop_flag.load(Ordering::SeqCst) {
            return None; // 停止信号：退出重连
        }
        *backoff = (*backoff * 2).min(LISTENER_RECONNECT_MAX);
        match transport.subscribe(channel).await {
            Ok(rx) => {
                log::info!(target: "limiteron", "ban-sync resubscribed to {channel}");
                return Some(rx);
            }
            Err(e) => {
                log::warn!(target: "limiteron", "ban-sync resubscribe failed: {e}");
            }
        }
    }
}

pub struct BanSyncBus {
    transport: Arc<dyn PubSubTransport>,
    config: BanSyncConfig,
}

impl BanSyncBus {
    /// 基于传输层与配置创建总线
    pub fn new(transport: Arc<dyn PubSubTransport>, config: BanSyncConfig) -> Self {
        Self { transport, config }
    }

    /// 本实例 ID
    pub fn instance_id(&self) -> &str {
        &self.config.instance_id
    }

    /// 广播封禁/解封事件
    pub async fn publish(&self, msg: BanSyncMessage) -> Result<(), LimiteronError> {
        self.transport
            .publish(&self.config.channel, &msg.encode()?)
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))?;
        Ok(())
    }

    /// 订阅通道并把**其他实例**的封禁事件应用到本地 applier。
    ///
    /// 自消息豁免：`origin == instance_id` 的消息被丢弃。
    pub async fn spawn_listener(
        &self,
        applier: Arc<dyn BanSyncApplier>,
    ) -> Result<BanSyncListenerHandle, LimiteronError> {
        // 首次订阅失败仍视为致命（调用方需要立即知道配置/网络问题）
        let first_rx = self
            .transport
            .subscribe(&self.config.channel)
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))?;
        let instance_id = self.config.instance_id.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let transport = self.transport.clone();
        let channel = self.config.channel.clone();

        let join = tokio::spawn(async move {
            let mut rx = first_rx;
            let mut backoff = LISTENER_RECONNECT_MIN;
            loop {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                // 带超时轮询以便响应 stop（与 oxcache InvalidationBus 同款模式）
                let payload =
                    match tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv())
                        .await
                    {
                        Ok(Some(payload)) => payload,
                        Ok(None) => {
                            // 历史教训：订阅端关闭曾直接永久退出——之后所有
                            // 封禁事件静默失步直到重启。现退避重订阅。
                            log::warn!(
                                target: "limiteron",
                                "ban-sync transport closed; resubscribing in {backoff:?}"
                            );
                            match resubscribe_with_backoff(
                                &transport,
                                &channel,
                                &mut backoff,
                                &stop_flag,
                            )
                            .await
                            {
                                Some(new_rx) => rx = new_rx,
                                None => break, // 不可达（助手不返回 None），防御性收尾
                            }
                            backoff = LISTENER_RECONNECT_MIN;
                            continue;
                        }
                        Err(_) => continue, // 超时：回到 stop 检查
                    };
                let msg = match BanSyncMessage::decode(&payload) {
                    Ok(m) => m,
                    // 无法解析的消息跳过（不 panic），但必须留痕：
                    // 静默丢弃会让通道污染/版本失配完全不可见
                    Err(e) => {
                        log::warn!(target: "limiteron", "dropping undecodable ban-sync message: {e}");
                        continue;
                    }
                };
                // 自消息豁免
                if msg.origin == instance_id {
                    continue;
                }
                match msg.kind {
                    BanSyncKind::BanApplied => applier.on_remote_ban(&msg).await,
                    BanSyncKind::BanRemoved => applier.on_remote_unban(&msg).await,
                }
            }
        });

        Ok(BanSyncListenerHandle { join, stop })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxcache::invalidation::InMemoryPubSubTransport;
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 记录型 applier：收集收到的消息
    #[derive(Default)]
    struct RecordingApplier {
        bans: Mutex<Vec<BanSyncMessage>>,
        unbans: Mutex<Vec<BanSyncMessage>>,
    }

    #[async_trait::async_trait]
    impl BanSyncApplier for RecordingApplier {
        async fn on_remote_ban(&self, msg: &BanSyncMessage) {
            self.bans.lock().push(msg.clone());
        }
        async fn on_remote_unban(&self, msg: &BanSyncMessage) {
            self.unbans.lock().push(msg.clone());
        }
    }

    async fn wait_for(cond: impl Fn() -> bool) {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while !cond() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "5s 内未收到同步消息"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    /// 两实例经 mock Pub/Sub 同步：A 封禁 → B 可见（spec 跨实例场景）
    #[tokio::test]
    async fn test_t616_ban_visible_on_other_instance() {
        let transport = Arc::new(InMemoryPubSubTransport::new());
        let bus_a = BanSyncBus::new(transport.clone(), BanSyncConfig::new("inst-a"));
        let bus_b = BanSyncBus::new(transport.clone(), BanSyncConfig::new("inst-b"));

        let applier_b = Arc::new(RecordingApplier::default());
        let listener_b = bus_b.spawn_listener(applier_b.clone()).await.unwrap();

        bus_a
            .publish(BanSyncMessage::ban_applied(
                "inst-a",
                "ip:192.0.2.7",
                "abuse",
                Some(1_800_000_000),
            ))
            .await
            .unwrap();

        wait_for(|| !applier_b.bans.lock().is_empty()).await;
        let got = applier_b.bans.lock()[0].clone();
        assert_eq!(got.target_key, "ip:192.0.2.7");
        assert_eq!(got.reason, "abuse");
        assert_eq!(got.expires_at_epoch, Some(1_800_000_000));

        // 解封同样跨实例可见
        bus_a
            .publish(BanSyncMessage::ban_removed("inst-a", "ip:192.0.2.7"))
            .await
            .unwrap();
        wait_for(|| !applier_b.unbans.lock().is_empty()).await;
        assert_eq!(applier_b.unbans.lock()[0].target_key, "ip:192.0.2.7");

        listener_b.stop();
        listener_b.join().await;
    }

    /// 自消息豁免：实例不把自己广播的封禁再应用一遍（防回环）
    #[tokio::test]
    async fn test_t616_self_message_is_exempt() {
        let transport = Arc::new(InMemoryPubSubTransport::new());
        let bus_a = BanSyncBus::new(transport.clone(), BanSyncConfig::new("inst-a"));

        let applier_a = Arc::new(RecordingApplier::default());
        let listener_a = bus_a.spawn_listener(applier_a.clone()).await.unwrap();

        bus_a
            .publish(BanSyncMessage::ban_applied(
                "inst-a", "user:u1", "self", None,
            ))
            .await
            .unwrap();

        // 等过轮询窗口：消息已送达但必须被豁免
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(applier_a.bans.lock().is_empty(), "自身实例的消息必须被豁免");

        listener_a.stop();
        listener_a.join().await;
    }

    /// 一对多广播：一条封禁消息到达全部订阅实例
    #[tokio::test]
    async fn test_t616_fanout_to_multiple_instances() {
        let transport = Arc::new(InMemoryPubSubTransport::new());
        let bus_a = BanSyncBus::new(transport.clone(), BanSyncConfig::new("inst-a"));
        let bus_b = BanSyncBus::new(transport.clone(), BanSyncConfig::new("inst-b"));
        let bus_c = BanSyncBus::new(transport.clone(), BanSyncConfig::new("inst-c"));

        let applier_b = Arc::new(RecordingApplier::default());
        let applier_c = Arc::new(RecordingApplier::default());
        let lb = bus_b.spawn_listener(applier_b.clone()).await.unwrap();
        let lc = bus_c.spawn_listener(applier_c.clone()).await.unwrap();

        bus_a
            .publish(BanSyncMessage::ban_applied(
                "inst-a",
                "cidr:10.0.0.0/8",
                "ddos",
                None,
            ))
            .await
            .unwrap();

        wait_for(|| !applier_b.bans.lock().is_empty()).await;
        wait_for(|| !applier_c.bans.lock().is_empty()).await;
        assert_eq!(applier_c.bans.lock()[0].target_key, "cidr:10.0.0.0/8");

        lb.stop();
        lc.stop();
        lb.join().await;
        lc.join().await;
    }

    /// 消息编解码 roundtrip + 非法载荷显性报错
    #[test]
    fn test_t616_message_encode_decode_roundtrip() {
        let msg = BanSyncMessage::ban_applied("i", "ip:1.2.3.4", "r", Some(42));
        let wire = msg.encode().unwrap();
        assert_eq!(BanSyncMessage::decode(&wire).unwrap(), msg);

        assert!(BanSyncMessage::decode("not json").is_err());
    }
    /// 前 N 次订阅返回 Err、随后委托真实传输的 mock：
    /// 模拟传输层故障（Redis 连接抖动）下的重订阅恢复
    struct FlakySubscribeTransport {
        inner: InMemoryPubSubTransport,
        /// 剩余故障次数：此前 N 次 subscribe 返回 Err
        remaining: AtomicU64,
    }

    #[async_trait::async_trait]
    impl oxcache::invalidation::PubSubTransport for FlakySubscribeTransport {
        async fn publish(&self, channel: &str, payload: &str) -> oxcache::error::OxCacheResult<()> {
            self.inner.publish(channel, payload).await
        }

        async fn subscribe(
            &self,
            channel: &str,
        ) -> oxcache::error::OxCacheResult<oxcache::invalidation::SubscriptionReceiver> {
            if self.remaining.fetch_sub(1, Ordering::SeqCst) > 0 {
                return Err(oxcache::error::OxCacheError::Operation(
                    "flaky transport: simulated subscribe failure".to_string(),
                ));
            }
            self.inner.subscribe(channel).await
        }
    }

    #[tokio::test]
    async fn test_resubscribe_with_backoff_recovers_after_failures() {
        // 断线重连回归：订阅端关闭曾永久退出监听任务——之后所有封禁
        // 事件静默失步直到重启。助手在传输故障（此处注入 2 次 Err）下
        // 指数退避重试，最终恢复订阅通道。
        let transport = Arc::new(FlakySubscribeTransport {
            inner: InMemoryPubSubTransport::new(),
            remaining: AtomicU64::new(2),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let mut backoff = LISTENER_RECONNECT_MIN;

        let rx = resubscribe_with_backoff(
            &(transport.clone() as Arc<dyn oxcache::invalidation::PubSubTransport>),
            "ban-sync-reconnect-test",
            &mut backoff,
            &stop,
        )
        .await;
        assert!(rx.is_some(), "两次故障后应恢复订阅");

        // 恢复后的通道真实可用：发布即达
        transport
            .inner
            .publish("ban-sync-reconnect-test", "hello")
            .await
            .unwrap();
        let payload = tokio::time::timeout(std::time::Duration::from_secs(1), rx.unwrap().recv())
            .await
            .expect("1s 内应收到消息");
        assert_eq!(payload, Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_resubscribe_stops_on_stop_flag() {
        // 停止信号优先：重连循环必须响应 stop,不得无限重试
        let transport = Arc::new(FlakySubscribeTransport {
            inner: InMemoryPubSubTransport::new(),
            remaining: AtomicU64::new(u64::MAX), // 永远故障
        });
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let mut backoff = LISTENER_RECONNECT_MIN;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let handle = tokio::spawn(async move {
            resubscribe_with_backoff(
                &(transport.clone() as Arc<dyn oxcache::invalidation::PubSubTransport>),
                "ban-sync-stop-test",
                &mut backoff,
                &stop2,
            )
            .await
        });
        // 在首个退避窗口后发出停止
        tokio::time::sleep(std::time::Duration::from_millis(260)).await;
        stop.store(true, Ordering::SeqCst);

        let result = tokio::time::timeout(std::time::Duration::from_secs(3), handle)
            .await
            .expect("stop 后重连循环必须退出")
            .unwrap();
        assert!(result.is_none(), "stop 后应返回 None");
    }
}
