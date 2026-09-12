// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Webhook 外发签名：HMAC-SHA256 签名头 + 时间戳防重放。
//!
//! 复用工作区统一 HMAC-SHA256 模式（与审计链同源算法）：
//!
//! - 签名 = `HMAC-SHA256(secret, "{timestamp}.{payload}")` 的 hex 编码；
//! - 外发请求携带 `X-Limiteron-Timestamp`（Unix 秒）与
//!   `X-Limiteron-Signature: sha256=<hex>` 两个头；
//! - 接收方校验：时间戳偏差在容差窗口（默认 300s）内 + 重算签名
//!   恒等比较，二者同时满足才接受。窗口外的请求以
//!   [`WebhookVerifyError::Expired`] 拒绝（防重放），窗口内签名不匹配以
//!   [`WebhookVerifyError::InvalidSignature`] 拒绝（防篡改/防伪造）。
//!
//! # Example
//!
//! ```
//! use limiteron::WebhookSigner;
//!
//! let signer = WebhookSigner::new("webhook-secret");
//! let payload = r#"{"event":"RateLimitTriggered"}"#;
//! let sig = signer.sign(payload);
//! assert!(signer.verify(payload, sig.timestamp, &sig.signature).is_ok());
//! ```

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// 签名头名
pub const SIGNATURE_HEADER: &str = "X-Limiteron-Signature";
/// 时间戳头名（Unix 秒）
pub const TIMESTAMP_HEADER: &str = "X-Limiteron-Timestamp";
/// 默认防重放容差窗口（秒）
pub const DEFAULT_MAX_AGE_SECS: i64 = 300;

type HmacSha256 = Hmac<Sha256>;

/// 校验失败原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookVerifyError {
    /// 时间戳超出容差窗口（过期或超前过多）——防重放拒绝
    Expired,
    /// 签名与负载不匹配——防篡改拒绝
    InvalidSignature,
    /// 签名格式非法（非 hex / 带无法识别的前缀）
    MalformedSignature,
}

/// 一次签名计算的产物（含时间戳），可直接映射为 HTTP 头
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookSignature {
    /// 签名时的 Unix 秒时间戳
    pub timestamp: i64,
    /// `sha256=<hex>` 形式的签名值
    pub signature: String,
}

impl WebhookSignature {
    /// 签名头取值（已含 `sha256=` 前缀）
    pub fn header_value(&self) -> &str {
        &self.signature
    }

    /// 时间戳头的字符串取值
    pub fn timestamp_header_value(&self) -> String {
        self.timestamp.to_string()
    }
}

/// Webhook HMAC-SHA256 签名器
///
/// `secret` 须与接收方共享；不同接收方可各建一个 signer 实例。
#[derive(Clone)]
pub struct WebhookSigner {
    secret: Vec<u8>,
    max_age_secs: i64,
}

impl std::fmt::Debug for WebhookSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 不输出 secret
        f.debug_struct("WebhookSigner")
            .field("secret", &"<redacted>")
            .field("max_age_secs", &self.max_age_secs)
            .finish()
    }
}

impl WebhookSigner {
    /// 以默认容差窗口（300s）创建签名器
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            secret: secret.into(),
            max_age_secs: DEFAULT_MAX_AGE_SECS,
        }
    }

    /// 自定义防重放容差窗口（秒）
    pub fn with_tolerance(mut self, max_age_secs: i64) -> Self {
        self.max_age_secs = max_age_secs;
        self
    }

    /// 防重放容差窗口
    pub fn max_age_secs(&self) -> i64 {
        self.max_age_secs
    }

    /// 计算负载签名（`HMAC-SHA256(secret, "{timestamp}.{payload}")` hex）
    pub fn sign_payload(&self, payload: &str, timestamp: i64) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("HMAC accepts any key size");
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// 以当前时间签名，产出可直挂 HTTP 头的 [`WebhookSignature`]
    pub fn sign(&self, payload: &str) -> WebhookSignature {
        let timestamp = chrono::Utc::now().timestamp();
        WebhookSignature {
            timestamp,
            signature: format!("sha256={}", self.sign_payload(payload, timestamp)),
        }
    }

    /// 校验签名 + 时间戳防重放
    ///
    /// `signature` 接受带 `sha256=` 前缀（自身 `sign` 的输出格式）或裸 hex。
    pub fn verify(
        &self,
        payload: &str,
        timestamp: i64,
        signature: &str,
    ) -> Result<(), WebhookVerifyError> {
        // 1. 防重放：时间戳必须在容差窗口内（过去或超前均受约束）
        let now = chrono::Utc::now().timestamp();
        let age = now - timestamp;
        if age.abs() > self.max_age_secs {
            return Err(WebhookVerifyError::Expired);
        }

        // 2. 剥离可选前缀
        let hex_sig = signature
            .strip_prefix("sha256=")
            .unwrap_or(signature)
            .trim();
        if hex_sig.is_empty() || !hex_sig.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(WebhookVerifyError::MalformedSignature);
        }

        // 3. 恒等比较重算签名（防时序侧信道）
        let expected = self.sign_payload(payload, timestamp);
        if constant_time_eq(hex_sig.as_bytes(), expected.as_bytes()) {
            Ok(())
        } else {
            Err(WebhookVerifyError::InvalidSignature)
        }
    }
}

/// 恒等比较（与审计链的 constant_time_compare 同构）
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

// ============================================================================
// 进程级签名器（EventDispatcher 外发路径消费；与 GLOBAL_LIMITER_MANAGER
// 同款进程级控制面单例模式，不参与决策热路径）
// ============================================================================

static GLOBAL_WEBHOOK_SIGNER: std::sync::OnceLock<WebhookSigner> = std::sync::OnceLock::new();

/// 设置进程级 webhook 签名器（`EventDispatcher` 外发全部 webhook 均签名）。
///
/// 返回 `false` 表示已有签名器（进程签名密钥只允许设置一次；轮换请重启
/// 或新建 dispatcher 所在进程）。幂等安全：重复设置同实例返回 `true`。
pub fn set_global_webhook_signer(signer: WebhookSigner) -> bool {
    GLOBAL_WEBHOOK_SIGNER.set(signer).is_ok()
}

/// 读取进程级 webhook 签名器（未设置 → None，外发不签名——向后兼容）
pub fn global_webhook_signer() -> Option<&'static WebhookSigner> {
    GLOBAL_WEBHOOK_SIGNER.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = r#"{"id":"evt_1","type":"RateLimitTriggered","key":"192.0.2.1"}"#;

    /// 签名 → 校验 roundtrip 通过，格式带 sha256= 前缀
    #[test]
    fn test_t614_sign_verify_roundtrip() {
        let signer = WebhookSigner::new("secret-abc");
        let sig = signer.sign(PAYLOAD);
        assert!(
            sig.signature.starts_with("sha256="),
            "header form: {}",
            sig.signature
        );
        assert!(
            signer
                .verify(PAYLOAD, sig.timestamp, &sig.signature)
                .is_ok()
        );
        // 裸 hex 亦可校验
        let bare = sig.signature.trim_start_matches("sha256=");
        assert!(signer.verify(PAYLOAD, sig.timestamp, bare).is_ok());
    }

    /// 签名可复现：同 (secret, payload, timestamp) 得同 hex
    #[test]
    fn test_t614_sign_payload_deterministic() {
        let signer = WebhookSigner::new("secret-abc");
        let a = signer.sign_payload(PAYLOAD, 1_700_000_000);
        let b = signer.sign_payload(PAYLOAD, 1_700_000_000);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "SHA-256 hex 长度");
        // 时间戳进入签名消息 → 不同时间戳签名不同
        assert_ne!(a, signer.sign_payload(PAYLOAD, 1_700_000_001));
    }

    /// 载荷被篡改 → InvalidSignature
    #[test]
    fn test_t614_tampered_payload_rejected() {
        let signer = WebhookSigner::new("secret-abc");
        let sig = signer.sign(PAYLOAD);
        let tampered = PAYLOAD.replace("evt_1", "evt_2");
        assert_eq!(
            signer.verify(&tampered, sig.timestamp, &sig.signature),
            Err(WebhookVerifyError::InvalidSignature),
            "any payload mutation must invalidate the signature"
        );
    }

    /// 密钥不匹配 → InvalidSignature（防伪造）
    #[test]
    fn test_t614_wrong_secret_rejected() {
        let signer = WebhookSigner::new("secret-abc");
        let sig = signer.sign(PAYLOAD);
        let attacker = WebhookSigner::new("secret-xyz");
        assert_eq!(
            attacker.verify(PAYLOAD, sig.timestamp, &sig.signature),
            Err(WebhookVerifyError::InvalidSignature)
        );
    }

    /// 时间戳超出容差 → Expired（防重放核心断言）
    #[test]
    fn test_t614_stale_timestamp_rejected_as_replay() {
        let signer = WebhookSigner::new("secret-abc").with_tolerance(300);
        let now = chrono::Utc::now().timestamp();
        let sig_hex = signer.sign_payload(PAYLOAD, now - 301);
        assert_eq!(
            signer.verify(PAYLOAD, now - 301, &sig_hex),
            Err(WebhookVerifyError::Expired),
            "重放 301s 前的签名必须被拒"
        );
        // 超前时间戳同样受窗口约束（防伪造未来窗口）
        let future_hex = signer.sign_payload(PAYLOAD, now + 301);
        assert_eq!(
            signer.verify(PAYLOAD, now + 301, &future_hex),
            Err(WebhookVerifyError::Expired)
        );
        // 窗口边缘内（300s）应通过
        let edge_hex = signer.sign_payload(PAYLOAD, now - 300);
        assert!(signer.verify(PAYLOAD, now - 300, &edge_hex).is_ok());
    }

    /// 非法签名格式 → MalformedSignature
    #[test]
    fn test_t614_malformed_signature_rejected() {
        let signer = WebhookSigner::new("secret-abc");
        let now = chrono::Utc::now().timestamp();
        assert_eq!(
            signer.verify(PAYLOAD, now, "sha256=not-hex!"),
            Err(WebhookVerifyError::MalformedSignature)
        );
        assert_eq!(
            signer.verify(PAYLOAD, now, ""),
            Err(WebhookVerifyError::MalformedSignature)
        );
    }

    /// Debug 输出不得泄露 secret
    #[test]
    fn test_t614_debug_redacts_secret() {
        let signer = WebhookSigner::new("super-secret-value");
        let rendered = format!("{signer:?}");
        assert!(!rendered.contains("super-secret-value"));
        assert!(rendered.contains("<redacted>"));
    }

    /// 常量头名契约（接收方按此对接）
    #[test]
    fn test_t614_header_name_contract() {
        assert_eq!(SIGNATURE_HEADER, "X-Limiteron-Signature");
        assert_eq!(TIMESTAMP_HEADER, "X-Limiteron-Timestamp");
        assert_eq!(DEFAULT_MAX_AGE_SECS, 300);
    }

    /// 进程级签名器：set-once 语义（首次设置成功，重复设置被拒）。
    ///
    /// 注：全局 signer 为进程单例，同测试二进制内其他用例（如 dispatcher
    /// 的签名链路测试）可能先行安装——因此仅在「本次真正安装成功」时断言
    /// 密钥生效；无论谁先安装，重复设置都必须被拒。
    #[test]
    fn test_t614_global_signer_set_once() {
        let installed = super::set_global_webhook_signer(WebhookSigner::new("global-secret"));
        let signer = super::global_webhook_signer().expect("global signer available");
        if installed {
            let sig = signer.sign(PAYLOAD);
            assert!(
                signer
                    .verify(PAYLOAD, sig.timestamp, &sig.signature)
                    .is_ok()
            );
        }
        assert!(!super::set_global_webhook_signer(WebhookSigner::new(
            "other"
        )));
    }
}
