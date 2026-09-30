// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 只读管理 Web UI（`admin-ui` feature）
//!
//! 内嵌单页（无构建链，vanilla HTML+JS）展示限流 / 熔断 / 配额状态。
//!
//! ## 机械只读守卫（三层，新增写端点即红灯）
//!
//! 1. **显式路由表**：[`ROUTE_TABLE`] 是路径清单的唯一事实来源，Router
//!    由它构建且只经 `get()` 注册；守卫测试断言表内全部路由为 GET。
//! 2. **注册完整性**：守卫测试枚举 Router 实际注册的路径集合与
//!    [`ROUTE_TABLE`] 对比，防止数组与注册漂移。
//! 3. **类型窄化**：数据 handler 只接受 `Arc<dyn ReadOnlySnapshotSource>`
//!    （[`AdminService`] 的只读投影 trait，blanket 委托三个只读方法），
//!    本模块类型面上不存在可调用的写方法。
//!
//! 另有请求方法级守卫：对每个注册路径发 POST/PUT/DELETE 断言 405。
//!
//! ## 绑定安全
//!
//! 默认绑定 `127.0.0.1`（仅本机访问）。**本 UI 无认证**——改为非回环
//! 地址会将未鉴权的管理快照（规则、统计、健康、封禁维度元数据）暴露给
//! 同网段所有主机；`WebUiServer::start` 对非回环绑定显性发出
//! `tracing::warn!`，运维必须以前置反向代理（认证 + TLS）暴露。详见
//! README「安全」与 `docs/API_REFERENCE.md` 对应节。
//!
//! **已知边界（登记）**：GET 端点不做 Host/Origin 校验，存在 DNS
//! rebinding 的信息泄露面（攻击者诱导浏览器以恶意域名解析到回环地址
//! 读取快照）。缓解依据：端点全部 GET 只读、无状态变更，泄露面限于
//! 本机快照数据；需要彻底闭合时由前置反向代理强制 Host 白名单，或
//! 在路由层加 Host 校验中间件（引入即须更新本登记与守卫测试）。
//!
//! feature 决策记录：`admin-ui` 默认关闭、不入 `full` preset（UI 面与
//! `openapi`/`admin-client` 同口径——文档与工具面不进默认编译）。

use super::server::LimiteronState;
use super::service::{AdminService, AdminServiceError};
use axum::{Json, Router, response::Html, routing::get};
use std::sync::Arc;

/// 只读快照源：[`AdminService`] 的类型级只读投影
///
/// blanket 委托到 [`AdminService`] 的三个只读方法；Web UI 数据 handler
/// 仅接受本 trait 对象，类型面上无法触达写端点（守卫 3）。
#[async_trait::async_trait]
pub trait ReadOnlySnapshotSource: Send + Sync {
    /// 系统状态
    async fn status(&self) -> super::handlers::SystemStatus;

    /// 运行时自省快照
    async fn introspect(&self) -> serde_json::Value;

    /// 熔断器状态
    #[cfg(feature = "circuit-breaker")]
    async fn circuit_breaker_status(
        &self,
    ) -> Result<super::handlers::CircuitBreakerStatus, AdminServiceError>;
}

#[async_trait::async_trait]
impl<T: AdminService + ?Sized> ReadOnlySnapshotSource for T {
    async fn status(&self) -> super::handlers::SystemStatus {
        AdminService::status(self).await
    }

    async fn introspect(&self) -> serde_json::Value {
        AdminService::introspect(self).await
    }

    #[cfg(feature = "circuit-breaker")]
    async fn circuit_breaker_status(
        &self,
    ) -> Result<super::handlers::CircuitBreakerStatus, AdminServiceError> {
        AdminService::circuit_breaker_status(self).await
    }
}

/// Web UI 路由表（守卫 1：路径清单的唯一事实来源，全部 GET）
///
/// 新增端点必须在此登记且仅可为 GET——守卫测试断言本表不含非 GET
/// 语义，且 Router 实际注册集合与本表一致（守卫 2）。
pub(crate) const ROUTE_TABLE: [&str; 3] = ["/", "/snapshot", "/circuit-breaker"];

/// 构建 Web UI Router（全部 GET；数据源经类型窄化的只读投影）
///
/// 注册完全由 [`ROUTE_TABLE`] 驱动：表是路径清单的唯一事实来源——多注册
/// 结构上不可能（无表外 route 调用），漏注册由守卫测试的 GET 探测红灯。
pub fn create_router(snapshot_source: Arc<dyn ReadOnlySnapshotSource>) -> Router {
    let mut router = Router::new();
    for path in ROUTE_TABLE {
        let source = snapshot_source.clone();
        router = router.route(
            path,
            get(move || async move { serve_snapshot(&source, path).await }),
        );
    }
    router
}

/// 按路径分派的只读响应（`/` 返回内嵌单页，其余返回快照 JSON）
async fn serve_snapshot(
    source: &Arc<dyn ReadOnlySnapshotSource>,
    path: &str,
) -> axum::response::Response {
    match path {
        "/" => axum::response::IntoResponse::into_response(Html(INDEX_HTML)),
        "/snapshot" => axum::response::IntoResponse::into_response(Json(source.introspect().await)),
        #[cfg(feature = "circuit-breaker")]
        "/circuit-breaker" => {
            let body = match source.circuit_breaker_status().await {
                Ok(status) => serde_json::json!({
                    "success": true,
                    "message": "OK",
                    "data": status,
                }),
                Err(e) => serde_json::json!({
                    "success": false,
                    "message": e.to_string(),
                    "data": serde_json::Value::Null,
                }),
            };
            axum::response::IntoResponse::into_response(Json(body))
        }
        #[cfg(not(feature = "circuit-breaker"))]
        "/circuit-breaker" => {
            axum::response::IntoResponse::into_response(Json(serde_json::json!({
                "success": false,
                "message": "circuit-breaker not configured",
                "data": serde_json::Value::Null,
            })))
        }
        // 路由表驱动注册下不可达；显性留痕防表与分派漂移
        other => axum::response::IntoResponse::into_response(Json(serde_json::json!({
            "success": false,
            "message": format!("unhandled route table entry: {other}"),
        }))),
    }
}

/// 内嵌单页（无构建链）：fetch `/snapshot` 与 `/circuit-breaker` 渲染
pub(crate) const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Limiteron 管理快照（只读）</title>
<style>
  :root { color-scheme: light dark; }
  body { font-family: system-ui, sans-serif; margin: 2rem auto; max-width: 60rem; padding: 0 1rem; }
  h1 { font-size: 1.3rem; } h2 { font-size: 1.05rem; margin-top: 1.8rem; }
  table { border-collapse: collapse; width: 100%; margin-top: .6rem; }
  th, td { border: 1px solid #8884; padding: .35rem .55rem; text-align: left; font-size: .88rem; }
  th { background: #8881; }
  .meta { color: #888; font-size: .8rem; }
  .ok { color: #1a7f37; } .bad { color: #c0392b; }
  code { background: #8882; padding: .05rem .3rem; border-radius: 3px; }
</style>
</head>
<body>
<h1>Limiteron 管理快照（只读）</h1>
<p class="meta">本页仅发起 GET 请求；自动刷新 <span id="refresh">5</span>s。数据源：/snapshot、/circuit-breaker。</p>
<h2>聚合统计</h2>
<table id="stats"><thead><tr><th>指标</th><th>值</th></tr></thead><tbody></tbody></table>
<h2>健康</h2>
<table id="health"><thead><tr><th>组件</th><th>状态</th></tr></thead><tbody></tbody></table>
<h2>规则与决策链</h2>
<table id="rules"><thead><tr><th>规则</th><th>优先级</th><th>匹配器</th><th>限流器</th><th>节点</th></tr></thead><tbody></tbody></table>
<h2>熔断器</h2>
<table id="cb"><thead><tr><th>状态</th><th>失败率</th><th>慢调用率</th></tr></thead><tbody></tbody></table>
<script>
function put(id, rows) {
  const tb = document.querySelector(id + ' tbody');
  tb.innerHTML = '';
  for (const [k, v, cls] of rows) {
    const tr = document.createElement('tr');
    const td1 = document.createElement('td'); td1.textContent = k;
    const td2 = document.createElement('td'); td2.textContent = String(v);
    if (cls) td2.className = cls;
    tr.append(td1, td2); tb.append(tr);
  }
}
async function refresh() {
  try {
    const snap = await (await fetch('/snapshot')).json();
    const s = snap.stats || {};
    put('#stats', [
      ['总请求', s.total_requests ?? '-'],
      ['拒绝请求', s.rejected_requests ?? '-'],
      ['封禁请求', s.banned_requests ?? '-'],
      ['活跃限流键', s.active_keys ?? '-'],
      ['配置版本', snap.config_version ?? '-'],
      ['已关闭', snap.is_shutdown ?? '-', snap.is_shutdown ? 'bad' : 'ok'],
    ]);
    const h = snap.health || {};
    const healthy = (v) => v === true || v === 'healthy';
    put('#health', [
      ['storage', h.storage_healthy ?? '-', healthy(h.storage_healthy) ? 'ok' : 'bad'],
      ['ban_storage', h.ban_storage_healthy ?? '-', healthy(h.ban_storage_healthy) ? 'ok' : 'bad'],
      ['cache', h.cache_healthy ?? '-', healthy(h.cache_healthy) ? 'ok' : 'bad'],
      ['background_tasks', h.background_tasks_alive ?? '-', healthy(h.background_tasks_alive) ? 'ok' : 'bad'],
    ]);
    const rules = snap.rules || [];
    const chains = snap.chains || [];
    const chainNodes = (id) => {
      const c = chains.find((x) => x.rule_id === id || x.id === id);
      return c ? (c.node_count ?? (c.nodes || []).length) : '-';
    };
    put('#rules', rules.map((r) => [
      (r.id ?? '') + ' / ' + (r.name ?? ''),
      r.priority ?? '-',
      r.matcher_count ?? (r.matchers || []).length,
      r.limiter_kinds ? r.limiter_kinds.join(', ') : ((r.limiters || []).length),
      chainNodes(r.id),
    ]));
    try {
      const cb = await (await fetch('/circuit-breaker')).json();
      const d = cb.data || {};
      put('#cb', [[
        d.state ?? 'unavailable', d.failure_rate ?? '-',
        d.slow_call_rate ?? '-',
      ].slice(0, 1).map((v) => [ 'state', v, v === 'Open' ? 'bad' : 'ok' ])[0],
      ['failure_rate', d.failure_rate ?? '-'],
      ['slow_call_rate', d.slow_call_rate ?? '-']]);
    } catch { put('#cb', [['state', 'unavailable', 'bad']]); }
  } catch (e) {
    put('#health', [['fetch', String(e), 'bad']]);
  }
}
refresh();
setInterval(refresh, 5000);
</script>
</body>
</html>
"#;

/// Web UI 配置（绑定默认 127.0.0.1，仅本机可见）
#[derive(Debug, Clone)]
pub struct WebUiConfig {
    /// 绑定地址；默认 `127.0.0.1`。改为非回环地址 = 无认证快照对同网段
    /// 暴露，必须前置反向代理（认证 + TLS）
    pub host: String,
    /// 绑定端口（默认 9091，与 admin API 默认端口 9090 区分）
    pub port: u16,
}

impl Default for WebUiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9091,
        }
    }
}

impl WebUiConfig {
    /// 绑定地址是否为回环
    #[must_use]
    pub fn is_loopback_binding(&self) -> bool {
        self.host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(matches!(self.host.as_str(), "localhost"))
    }

    /// 绑定目标 `host:port`
    #[must_use]
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// 只读 Web UI 服务器
pub struct WebUiServer {
    state: LimiteronState,
    config: WebUiConfig,
}

impl WebUiServer {
    /// 以应用状态与配置构造
    #[must_use]
    pub fn new(state: LimiteronState, config: WebUiConfig) -> Self {
        Self { state, config }
    }

    /// 绑定配置
    #[must_use]
    pub fn address(&self) -> String {
        self.config.address()
    }

    /// 启动服务器（阻塞当前任务）
    ///
    /// 非回环绑定发出无认证暴露警示日志（不阻断——部署者可能确有内网
    /// 可信环境需求，但警示必须留痕，见模块文档）。
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.is_loopback_binding() {
            tracing::warn!(
                address = %self.config.address(),
                "web UI bound to a non-loopback address: the snapshot endpoints are UNAUTHENTICATED; \
                 expose only behind an authenticating reverse proxy (TLS terminated there)"
            );
        }
        let router = create_router(Arc::new(self.state.service()));
        let listener = tokio::net::TcpListener::bind(self.config.address()).await?;
        log::info!(
            target: "admin-ui",
            "read-only web UI listening on {} (auth: none; data: GET-only snapshots)",
            self.config.address()
        );
        axum::serve(listener, router).await?;
        Ok(())
    }

    /// 构建 Router（测试与外部 runner 挂载用）
    pub fn into_router(&self) -> Router {
        create_router(Arc::new(self.state.service()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::{LimiteronState, make_state};
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    /// 守卫 1：路由表全部为 GET 语义（表即事实来源，新增非 GET 即红灯）
    #[test]
    fn route_table_is_get_only() {
        // 表中路径即为路由清单；本模块仅经 get() 注册（编译期保证），
        // 此处断言表规模与已知面一致，防路径静默增删
        assert_eq!(ROUTE_TABLE.len(), 3);
        assert!(ROUTE_TABLE.contains(&"/"));
        assert!(ROUTE_TABLE.contains(&"/snapshot"));
        assert!(ROUTE_TABLE.contains(&"/circuit-breaker"));
    }

    /// 守卫 2：路由表每个路径 GET 可达（注册完整性——漏注册即红灯）
    #[tokio::test]
    async fn route_table_paths_are_all_registered_and_serve() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default());
        let router = server.into_router();
        for path in ROUTE_TABLE {
            let response = router
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "路由表路径 {path} 未注册（表与 Router 漂移）"
            );
        }
    }

    /// 方法级守卫：对每个注册路径发 POST/PUT/DELETE，全部 405
    #[tokio::test]
    async fn non_get_methods_are_rejected_on_every_path() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default());
        let router = server.into_router();
        for path in ROUTE_TABLE {
            for method in [Method::POST, Method::PUT, Method::DELETE] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method.clone())
                            .uri(path)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::METHOD_NOT_ALLOWED,
                    "{method} {path} 必须被拒（只读守卫）"
                );
            }
        }
    }

    /// 数据端点语义：/snapshot 返回 introspect 投影；GET / 返回 HTML
    #[tokio::test]
    async fn get_endpoints_serve_readonly_snapshots() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default());
        let router = server.into_router();

        let index = router
            .clone()
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(index.status(), StatusCode::OK);
        let body = axum::body::to_bytes(index.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains("只读"), "内嵌单页应自述只读属性");
        assert!(
            !html.contains("POST") && !html.contains("DELETE"),
            "页面脚本不得包含写方法调用"
        );

        let snapshot = router
            .oneshot(Request::get("/snapshot").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(snapshot.status(), StatusCode::OK);
        let body = axum::body::to_bytes(snapshot.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("stats").is_some(), "快照应含统计面");
    }

    #[test]
    fn binding_defaults_to_loopback_and_flags_non_loopback() {
        let config = WebUiConfig::default();
        assert!(config.is_loopback_binding());
        assert_eq!(config.address(), "127.0.0.1:9091");

        let exposed = WebUiConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
        };
        assert!(!exposed.is_loopback_binding());
    }

    /// 类型窄化烟测：Arc<dyn ReadOnlySnapshotSource> 可用（编译期守卫 3
    /// 的对象存在性确认）
    #[tokio::test]
    async fn readonly_projection_serves_from_state() {
        let state: LimiteronState = make_state().await;
        let source: Arc<dyn ReadOnlySnapshotSource> = Arc::new(state.service());
        let status = source.status().await;
        assert_eq!(status.total_requests, 0);
        let snapshot = source.introspect().await;
        assert!(snapshot.get("stats").is_some());
    }

    /// 启动烟测：随机回环端口真实 serve，HTTP GET 可达
    #[tokio::test]
    async fn server_serves_on_loopback() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let state = make_state().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let config = WebUiConfig {
            host: "127.0.0.1".to_string(),
            port: addr.port(),
        };
        let server = WebUiServer::new(state, config);
        let handle = tokio::spawn(async move { server.start().await });

        // 就绪轮询后发真实 HTTP 请求
        let mut attempt = 0;
        let mut stream = loop {
            attempt += 1;
            match tokio::net::TcpStream::connect(addr).await {
                Ok(s) => break s,
                Err(_) if attempt < 50 => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await
                }
                Err(e) => panic!("server never became reachable: {e}"),
            }
        };
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let text = String::from_utf8_lossy(&buf);
        assert!(text.starts_with("HTTP/1.1 200 OK"), "got: {text}");

        handle.abort();
    }
}
