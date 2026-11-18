// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! `limiteron doctor --security` 安全姿态自审计
//!
//! 从编译期 feature 面 + 可选运行期配置静态检查安全姿态，不连接运行实例
//! （运行期自省由 Admin API `/api/v1/introspect` 承担）。
//! 检查项三态：Pass / Warn / Fail；退出码映射 0/1/2。

use crate::config::FlowControlConfig;
use serde::Serialize;

/// 检查项三态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorStatus {
    /// 防护到位
    Pass,
    /// 需关注（非阻断）
    Warn,
    /// 失败（阻断部署）
    Fail,
}

/// 单条检查结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorCheck {
    /// 检查项名称（稳定标识，如 `input-validation`）
    pub name: String,
    /// 判定状态
    pub status: DoctorStatus,
    /// 人类可读详情（已本地化）
    pub detail: String,
}

/// doctor 检查报告
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DoctorReport {
    /// 全部检查项
    pub checks: Vec<DoctorCheck>,
    /// 安全 profile 摘要（编译期可见默认值）
    pub profile: String,
}

impl DoctorReport {
    /// 汇总判定：任一 Fail → Fail；否则任一 Warn → Warn；否则 Pass。
    pub fn overall(&self) -> DoctorStatus {
        if self.checks.iter().any(|c| c.status == DoctorStatus::Fail) {
            DoctorStatus::Fail
        } else if self.checks.iter().any(|c| c.status == DoctorStatus::Warn) {
            DoctorStatus::Warn
        } else {
            DoctorStatus::Pass
        }
    }

    /// 三态计数 (pass, warn, fail)
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut p = 0;
        let mut w = 0;
        let mut f = 0;
        for c in &self.checks {
            match c.status {
                DoctorStatus::Pass => p += 1,
                DoctorStatus::Warn => w += 1,
                DoctorStatus::Fail => f += 1,
            }
        }
        (p, w, f)
    }
}

fn check(name: &str, status: DoctorStatus, detail: String) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        status,
        detail,
    }
}

/// 执行 10 项安全姿态检查
///
/// `config` 为 `--config` 加载的规则配置；`None` 时仅做编译期 feature 面
/// 与静态默认值检查。admin 凭证/绑定 host 不在规则配置面内，按 feature
/// 编译态与默认配置静态判定（详见各检查项 detail）。
pub fn run_security_checks(config: Option<&FlowControlConfig>) -> DoctorReport {
    use crate::i18n::t_simple;

    let mut checks = Vec::with_capacity(10);

    // 1. input-validation
    checks.push(check(
        "input-validation",
        if cfg!(feature = "validation") {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Fail
        },
        t_simple("doctor-input-validation"),
    ));

    // 2. admin-bind-host（默认绑定回环；无 admin-api 即未暴露管理面）
    checks.push(check(
        "admin-bind-host",
        if cfg!(feature = "admin-api") {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Warn
        },
        t_simple("doctor-admin-bind-host"),
    ));

    // 3. admin-api-key（凭证不在规则配置面，默认静态面给出告警）
    checks.push(check(
        "admin-api-key",
        if cfg!(feature = "admin-api") {
            DoctorStatus::Warn
        } else {
            DoctorStatus::Pass
        },
        t_simple("doctor-admin-api-key"),
    ));

    // 4. admin-rbac
    checks.push(check(
        "admin-rbac",
        if cfg!(feature = "admin-api") {
            DoctorStatus::Warn
        } else {
            DoctorStatus::Pass
        },
        t_simple("doctor-admin-rbac"),
    ));

    // 5. web-ui-access
    checks.push(check(
        "web-ui-access",
        if cfg!(feature = "admin-ui") {
            DoctorStatus::Warn
        } else {
            DoctorStatus::Pass
        },
        t_simple("doctor-web-ui-access"),
    ));

    // 6. audit-chain
    checks.push(check(
        "audit-chain",
        if cfg!(feature = "audit-log") {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Warn
        },
        t_simple("doctor-audit-chain"),
    ));

    // 7. log-redaction
    checks.push(check(
        "log-redaction",
        if cfg!(feature = "log-redaction") {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Warn
        },
        t_simple("doctor-log-redaction"),
    ));

    // 8. webhook-signing
    checks.push(check(
        "webhook-signing",
        if cfg!(feature = "webhook") {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Warn
        },
        t_simple("doctor-webhook-signing"),
    ));

    // 9. distributed-failure-policy：规则未声明（PropagateError）时提示级
    let missing = config.is_some_and(|cfg| {
        cfg.rules
            .iter()
            .any(|r| r.failure_policy == crate::FailurePolicy::PropagateError)
    });
    checks.push(check(
        "distributed-failure-policy",
        if missing {
            DoctorStatus::Warn
        } else {
            DoctorStatus::Pass
        },
        if missing {
            t_simple("doctor-failure-policy-missing")
        } else {
            t_simple("doctor-failure-policy-ok")
        },
    ));

    // 10. layered-control
    let layered = cfg!(feature = "ban-manager")
        && cfg!(feature = "quota-control")
        && cfg!(feature = "circuit-breaker");
    checks.push(check(
        "layered-control",
        if layered {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Warn
        },
        t_simple("doctor-layered-control"),
    ));

    DoctorReport {
        checks,
        profile: "compile-time feature surface".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_security_checks_ten_items_without_config() {
        let report = run_security_checks(None);
        assert_eq!(report.checks.len(), 10);
        let names: Vec<_> = report.checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "input-validation",
                "admin-bind-host",
                "admin-api-key",
                "admin-rbac",
                "web-ui-access",
                "audit-chain",
                "log-redaction",
                "webhook-signing",
                "distributed-failure-policy",
                "layered-control",
            ]
        );
        // 静态 feature 面判定与编译期一致
        let input = &report.checks[0];
        assert_eq!(
            input.status,
            if cfg!(feature = "validation") {
                DoctorStatus::Pass
            } else {
                DoctorStatus::Fail
            }
        );
        // 无配置时 failure-policy 检查无告警
        assert_eq!(
            report.checks[8].status,
            DoctorStatus::Pass,
            "no config → no missing-policy warning"
        );
    }

    #[test]
    fn test_overall_and_counts() {
        let report = DoctorReport {
            checks: vec![
                check("a", DoctorStatus::Pass, "ok".into()),
                check("b", DoctorStatus::Warn, "warn".into()),
            ],
            profile: "test".into(),
        };
        assert_eq!(report.overall(), DoctorStatus::Warn);
        assert_eq!(report.counts(), (1, 1, 0));

        let failing = DoctorReport {
            checks: vec![
                check("a", DoctorStatus::Warn, "warn".into()),
                check("b", DoctorStatus::Fail, "fail".into()),
            ],
            profile: "test".into(),
        };
        assert_eq!(failing.overall(), DoctorStatus::Fail);
    }
}
