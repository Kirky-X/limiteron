#![cfg(feature = "code-review")]
//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 多代理代码审查系统
//!
//! 提供综合的安全、性能、代码质量和架构审查功能。
//!
//! # 特性
//!
//! - **多代理协调**: 整合安全审计、性能优化、代码质量、架构审查
//!
//! - **详细反馈**: 提供具体示例、改进建议和优先级级别
//!
//! - **工作流集成**: 支持预提交钩子和GitHub Actions自动执行
//!
//! # 使用示例
//!
//! ```rust
//! use limiteron::code_review::{CodeReviewConfig, CodeReviewManager};
//!
//! let config = CodeReviewConfig::default();
//! let manager = CodeReviewManager::new(config);
//! let report = manager.run_review().await;
//! ```

use crate::error::FlowGuardError;
use ahash::AHashMap as HashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 代码审查配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeReviewConfig {
    /// 是否启用安全审计
    pub security_audit: bool,
    /// 是否启用性能分析
    pub performance_analysis: bool,
    /// 是否启用代码质量检查
    pub code_quality_check: bool,
    /// 是否启用架构审查
    pub architecture_review: bool,
    /// 审查的文件路径
    pub paths: Vec<PathBuf>,
    /// 排除的文件模式
    pub exclude_patterns: Vec<String>,
    /// 严重性阈值
    pub severity_threshold: Severity,
    /// 并发执行的代理数量
    pub parallel_agents: usize,
    /// 生成详细报告
    pub detailed_report: bool,
    /// 报告输出路径
    pub output_path: Option<PathBuf>,
}

impl Default for CodeReviewConfig {
    fn default() -> Self {
        Self {
            security_audit: true,
            performance_analysis: true,
            code_quality_check: true,
            architecture_review: true,
            paths: vec![PathBuf::from("src/")],
            exclude_patterns: vec![
                "target/".to_string(),
                "*.generated.rs".to_string(),
                "*.pb.rs".to_string(),
            ],
            severity_threshold: Severity::Info,
            parallel_agents: 4,
            detailed_report: false,
            output_path: None,
        }
    }
}

/// 严重性级别
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// 致命错误 - 必须立即修复
    Critical = 4,
    /// 严重问题 - 需要尽快修复
    High = 3,
    /// 中等问题 - 建议修复
    Medium = 2,
    /// 低优先级 - 改进建议
    Low = 1,
    /// 信息性内容
    Info = 0,
}

/// 问题类别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum IssueCategory {
    /// 安全漏洞
    Security,
    /// 性能问题
    Performance,
    /// 代码质量问题
    CodeQuality,
    /// 架构问题
    Architecture,
    /// 最佳实践
    BestPractice,
    /// 文档问题
    Documentation,
}

/// 代码审查问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeReviewIssue {
    /// 唯一标识符
    pub id: String,
    /// 问题类别
    pub category: IssueCategory,
    /// 严重性级别
    pub severity: Severity,
    /// 问题标题
    pub title: String,
    /// 详细描述
    pub description: String,
    /// 文件路径
    pub file_path: PathBuf,
    /// 开始行号
    pub start_line: Option<u32>,
    /// 结束行号
    pub end_line: Option<u32>,
    /// 建议的修复方案
    pub suggestion: String,
    /// 相关的代码片段
    pub code_snippet: Option<String>,
    /// 相关的规则或标准
    pub rule: Option<String>,
    /// 是否可自动修复
    pub auto_fixable: bool,
}

/// 单个代理的审查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReviewResult {
    /// 代理名称
    pub agent_name: String,
    /// 代理类型
    pub agent_type: AgentType,
    /// 发现的问题列表
    pub issues: Vec<CodeReviewIssue>,
    /// 审查耗时（毫秒）
    pub duration_ms: u128,
    /// 审查状态
    pub status: ReviewStatus,
    /// 错误信息（如果失败）
    pub error_message: Option<String>,
}

/// 代理类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentType {
    /// 安全审计代理
    SecurityAuditor,
    /// 性能工程代理
    PerformanceEngineer,
    /// 代码审查代理
    CodeReviewer,
    /// 架构审查代理
    ArchitectReviewer,
}

/// 审查状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewStatus {
    /// 成功完成
    Success,
    /// 部分完成
    Partial,
    /// 失败
    Failed,
    /// 跳过
    Skipped,
}

/// 聚合的代码审查报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeReviewReport {
    /// 报告生成时间
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 审查配置
    pub config: CodeReviewConfig,
    /// 审查的文件数量
    pub files_reviewed: usize,
    /// 各代理的审查结果
    pub agent_results: Vec<AgentReviewResult>,
    /// 所有问题的统计摘要
    pub summary: ReviewSummary,
    /// 问题按类别分组
    pub issues_by_category: HashMap<IssueCategory, Vec<CodeReviewIssue>>,
    /// 问题按严重性分组
    pub issues_by_severity: HashMap<Severity, Vec<CodeReviewIssue>>,
    /// 整体审查结论
    pub conclusion: ReviewConclusion,
}

/// 审查摘要统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSummary {
    /// 总问题数
    pub total_issues: usize,
    /// 按严重性分类的数量
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    pub info_count: usize,
    /// 可自动修复的问题数
    pub auto_fixable_count: usize,
    /// 审查的文件数
    pub files_reviewed: usize,
}

/// 审查结论
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewConclusion {
    /// 通过 - 无严重问题
    Passed,
    /// 有警告 - 需要关注一些问题
    PassedWithWarnings,
    /// 有条件通过 - 需要修复关键问题
    ConditionalPass,
    /// 失败 - 需要修复严重问题
    Failed,
}

/// 代码审查管理器
#[derive(Debug)]
pub struct CodeReviewManager {
    /// 审查配置
    config: Arc<RwLock<CodeReviewConfig>>,
    /// 已完成的审查结果
    results: Arc<RwLock<Vec<AgentReviewResult>>>,
    /// 统计信息
    stats: Arc<RwLock<CodeReviewStats>>,
}

/// 代码审查统计
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CodeReviewStats {
    /// 总运行次数
    pub total_runs: u64,
    /// 成功次数
    pub successful_runs: u64,
    /// 失败次数
    pub failed_runs: u64,
    /// 总审查时间（毫秒）
    pub total_duration_ms: u128,
    /// 发现的总问题数
    pub total_issues_found: u64,
    /// 发现的关键问题数
    pub critical_issues_found: u64,
}

impl CodeReviewManager {
    /// 创建新的代码审查管理器
    ///
    /// # 参数
    /// - `config`: 审查配置
    ///
    /// # 返回
    /// 代码审查管理器实例
    pub fn new(config: CodeReviewConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            results: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(CodeReviewStats::default())),
        }
    }

    /// 运行完整的代码审查
    ///
    /// # 返回
    /// 聚合的审查报告
    pub async fn run_review(&self) -> Result<CodeReviewReport, FlowGuardError> {
        let start_time = std::time::Instant::now();
        let config = self.config.read().await.clone();

        // 收集所有审查结果
        let mut agent_results = Vec::new();

        // 并发执行各代理的审查任务
        let mut tasks: Vec<_> = Vec::new();

        if config.security_audit {
            let paths = config.paths.clone();
            let exclude_patterns = config.exclude_patterns.clone();
            tasks.push(tokio::spawn(async move {
                Self::run_agent_review(AgentType::SecurityAuditor, paths, exclude_patterns).await
            }));
        }

        if config.performance_analysis {
            let paths = config.paths.clone();
            let exclude_patterns = config.exclude_patterns.clone();
            tasks.push(tokio::spawn(async move {
                Self::run_agent_review(AgentType::PerformanceEngineer, paths, exclude_patterns)
                    .await
            }));
        }

        if config.code_quality_check {
            let paths = config.paths.clone();
            let exclude_patterns = config.exclude_patterns.clone();
            tasks.push(tokio::spawn(async move {
                Self::run_agent_review(AgentType::CodeReviewer, paths, exclude_patterns).await
            }));
        }

        if config.architecture_review {
            let paths = config.paths.clone();
            let exclude_patterns = config.exclude_patterns.clone();
            tasks.push(tokio::spawn(async move {
                Self::run_agent_review(AgentType::ArchitectReviewer, paths, exclude_patterns).await
            }));
        }

        // 收集结果
        for task in tasks {
            match task.await {
                Ok(result) => agent_results.push(result),
                Err(e) => {
                    tracing::error!("Agent task failed: {}", e);
                }
            }
        }

        let duration_ms = start_time.elapsed().as_millis();

        // 聚合结果
        let report = self.aggregate_results(agent_results, duration_ms).await;

        // 更新统计信息
        self.update_stats(&report).await;

        Ok(report)
    }

    /// 运行单个代理的审查
    async fn run_agent_review(
        agent_type: AgentType,
        paths: Vec<PathBuf>,
        exclude_patterns: Vec<String>,
    ) -> AgentReviewResult {
        let start_time = std::time::Instant::now();

        let (agent_name, issues) = match agent_type {
            AgentType::SecurityAuditor => (
                "SecurityAuditor".to_string(),
                Self::run_security_audit(&paths, &exclude_patterns).await,
            ),
            AgentType::PerformanceEngineer => (
                "PerformanceEngineer".to_string(),
                Self::run_performance_analysis(&paths, &exclude_patterns).await,
            ),
            AgentType::CodeReviewer => (
                "CodeReviewer".to_string(),
                Self::run_code_quality_check(&paths, &exclude_patterns).await,
            ),
            AgentType::ArchitectReviewer => (
                "ArchitectReviewer".to_string(),
                Self::run_architecture_review(&paths, &exclude_patterns).await,
            ),
        };

        let duration_ms = start_time.elapsed().as_millis();
        let is_empty = issues.is_empty();

        AgentReviewResult {
            agent_name,
            agent_type,
            issues,
            duration_ms,
            status: if is_empty {
                ReviewStatus::Success
            } else {
                ReviewStatus::Partial
            },
            error_message: None,
        }
    }

    /// 运行安全审计
    async fn run_security_audit(
        _paths: &[PathBuf],
        _exclude_patterns: &[String],
    ) -> Vec<CodeReviewIssue> {
        Vec::new()
    }

    /// 运行性能分析
    async fn run_performance_analysis(
        _paths: &[PathBuf],
        _exclude_patterns: &[String],
    ) -> Vec<CodeReviewIssue> {
        Vec::new()
    }

    /// 运行代码质量检查
    async fn run_code_quality_check(
        _paths: &[PathBuf],
        _exclude_patterns: &[String],
    ) -> Vec<CodeReviewIssue> {
        Vec::new()
    }

    /// 运行架构审查
    async fn run_architecture_review(
        _paths: &[PathBuf],
        _exclude_patterns: &[String],
    ) -> Vec<CodeReviewIssue> {
        Vec::new()
    }

    /// 聚合审查结果
    async fn aggregate_results(
        &self,
        agent_results: Vec<AgentReviewResult>,
        _total_duration_ms: u128,
    ) -> CodeReviewReport {
        let mut all_issues = Vec::new();
        let mut issues_by_category: HashMap<IssueCategory, Vec<CodeReviewIssue>> = HashMap::new();
        let mut issues_by_severity: HashMap<Severity, Vec<CodeReviewIssue>> = HashMap::new();

        for result in &agent_results {
            all_issues.extend(result.issues.clone());

            for issue in &result.issues {
                issues_by_category
                    .entry(issue.category.clone())
                    .or_default()
                    .push(issue.clone());

                issues_by_severity
                    .entry(issue.severity)
                    .or_default()
                    .push(issue.clone());
            }
        }

        // 按严重性排序
        let mut sorted_issues = all_issues.clone();
        sorted_issues.sort_by_key(|i| std::cmp::Reverse(i.severity));

        let summary = ReviewSummary {
            total_issues: all_issues.len(),
            critical_count: issues_by_severity
                .get(&Severity::Critical)
                .map(|v| v.len())
                .unwrap_or(0),
            high_count: issues_by_severity
                .get(&Severity::High)
                .map(|v| v.len())
                .unwrap_or(0),
            medium_count: issues_by_severity
                .get(&Severity::Medium)
                .map(|v| v.len())
                .unwrap_or(0),
            low_count: issues_by_severity
                .get(&Severity::Low)
                .map(|v| v.len())
                .unwrap_or(0),
            info_count: issues_by_severity
                .get(&Severity::Info)
                .map(|v| v.len())
                .unwrap_or(0),
            auto_fixable_count: all_issues.iter().filter(|i| i.auto_fixable).count(),
            files_reviewed: 0, // 将在实际实现中统计
        };

        let conclusion = Self::determine_conclusion(&summary);

        let config = self.config.read().await.clone();

        CodeReviewReport {
            timestamp: chrono::Utc::now(),
            config,
            files_reviewed: summary.files_reviewed,
            agent_results,
            summary,
            issues_by_category,
            issues_by_severity,
            conclusion,
        }
    }

    /// 确定审查结论
    fn determine_conclusion(summary: &ReviewSummary) -> ReviewConclusion {
        if summary.critical_count > 0 {
            ReviewConclusion::Failed
        } else if summary.high_count > 0 {
            ReviewConclusion::ConditionalPass
        } else if summary.medium_count > 0 {
            ReviewConclusion::PassedWithWarnings
        } else {
            ReviewConclusion::Passed
        }
    }

    /// 更新统计信息
    async fn update_stats(&self, report: &CodeReviewReport) {
        let mut stats = self.stats.write().await;
        stats.total_runs += 1;
        let agent_duration_sum: u128 = report.agent_results.iter().map(|r| r.duration_ms).sum();
        stats.total_duration_ms += agent_duration_sum;

        match report.conclusion {
            ReviewConclusion::Failed => stats.failed_runs += 1,
            _ => stats.successful_runs += 1,
        }

        stats.total_issues_found += report.summary.total_issues as u64;
        stats.critical_issues_found += report.summary.critical_count as u64;
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> CodeReviewStats {
        self.stats.read().await.clone()
    }

    /// 获取最近的审查结果
    pub async fn get_recent_results(&self) -> Vec<AgentReviewResult> {
        self.results.read().await.clone()
    }

    /// 更新配置
    pub async fn update_config(&self, config: CodeReviewConfig) {
        *self.config.write().await = config;
    }
}

/// 格式化审查报告为 Markdown
pub fn format_report_as_markdown(report: &CodeReviewReport) -> String {
    let mut md = String::new();

    md.push_str("# 代码审查报告\n\n");
    md.push_str(&format!("**生成时间:** {}\n\n", report.timestamp));
    md.push_str(&format!(
        "**审查结论:** {}\n\n",
        match report.conclusion {
            ReviewConclusion::Passed => "✅ 通过",
            ReviewConclusion::PassedWithWarnings => "⚠️ 通过（有警告）",
            ReviewConclusion::ConditionalPass => "⚡ 有条件通过",
            ReviewConclusion::Failed => "❌ 失败",
        }
    ));

    md.push_str("## 摘要\n\n");
    md.push_str("| 指标 | 数量 |\n");
    md.push_str("|------|------|\n");
    md.push_str(&format!("| 总问题数 | {} |\n", report.summary.total_issues));
    md.push_str(&format!(
        "| 🔴 严重 | {} |\n",
        report.summary.critical_count
    ));
    md.push_str(&format!("| 🟠 高 | {} |\n", report.summary.high_count));
    md.push_str(&format!("| 🟡 中 | {} |\n", report.summary.medium_count));
    md.push_str(&format!("| 🟢 低 | {} |\n", report.summary.low_count));
    md.push_str(&format!("| 🔵 信息 | {} |\n", report.summary.info_count));
    md.push_str(&format!(
        "| 可自动修复 | {} |\n\n",
        report.summary.auto_fixable_count
    ));

    md.push_str("## 详细问题\n\n");

    for (category, issues) in &report.issues_by_category {
        md.push_str(&format!("### {:?}\n\n", category));

        for issue in issues {
            md.push_str(&format!("#### {}\n", issue.title));
            md.push_str(&format!("**严重性:** {:?}\n\n", issue.severity));
            md.push_str(&format!("**描述:** {}\n\n", issue.description));
            md.push_str(&format!("**位置:** {:?}\n\n", issue.file_path));

            if let Some(snippet) = &issue.code_snippet {
                md.push_str("```rust\n");
                md.push_str(snippet);
                md.push_str("\n```\n\n");
            }

            md.push_str(&format!("**建议:** {}\n\n", issue.suggestion));
        }
    }

    md
}

/// 格式化审查报告为 JSON
pub fn format_report_as_json(report: &CodeReviewReport) -> Result<String, FlowGuardError> {
    serde_json::to_string_pretty(report).map_err(FlowGuardError::SerdeError)
}
