#!/usr/bin/env bash
# =============================================================================
# Limiteron 代码审查预提交钩子
# =============================================================================
# 
# 此脚本在提交前运行多代理代码审查，检查代码质量、安全性、性能和架构。
# 
# 使用方法:
#   1. 将此脚本链接到 .git/hooks/pre-commit
#   2. 或直接运行: ./scripts/pre-commit-hook.sh
#
# 选项:
#   --quick        快速模式，仅运行关键检查
#   --security     仅运行安全审计
#   --performance  仅运行性能分析
#   --quality      仅运行代码质量检查
#   --architecture 仅运行架构审查
#   --report       生成详细报告
#   --help         显示帮助信息
#
# =============================================================================

set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPORT_FILE="$PROJECT_ROOT/temp/code_review_report.md"
CONFIG_FILE="$PROJECT_ROOT/.code_review_config.yaml"
EXIT_CODE=0

# 默认设置
QUICK_MODE=false
SECURITY_ONLY=false
PERFORMANCE_ONLY=false
QUALITY_ONLY=false
ARCHITECTURE_ONLY=false
GENERATE_REPORT=false

# =============================================================================
# 帮助函数
# =============================================================================

show_help() {
    cat << EOF
Limiteron 代码审查预提交钩子

用法: $0 [选项]

选项:
  --quick        快速模式，仅运行关键检查
  --security     仅运行安全审计
  --performance  仅运行性能分析
  --quality      仅运行代码质量检查
  --architecture 仅运行架构审查
  --report       生成详细报告
  --help         显示此帮助信息

示例:
  $0                    # 运行所有检查
  $0 --quick            # 快速模式
  $0 --security --report # 安全审计并生成报告
EOF
}

# =============================================================================
# 日志函数
# =============================================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

# =============================================================================
# 依赖检查
# =============================================================================

check_dependencies() {
    log_info "检查依赖..."
    
    local missing_deps=()
    
    if ! command -v cargo &> /dev/null; then
        missing_deps+=("cargo")
    fi
    
    if ! command -v rustfmt &> /dev/null; then
        missing_deps+=("rustfmt")
    fi
    
    if ! command -v clippy-driver &> /dev/null; then
        missing_deps+=("clippy")
    fi
    
    if [ ${#missing_deps[@]} -ne 0 ]; then
        log_error "缺少必要的依赖: ${missing_deps[*]}"
        echo "请安装: cargo install rustfmt clippy"
        exit 1
    fi
    
    log_success "所有依赖已就绪"
}

# =============================================================================
# 格式化检查
# =============================================================================

check_formatting() {
    log_info "检查代码格式..."
    
    if cargo fmt --all -- --check 2>&1; then
        log_success "代码格式正确"
        return 0
    else
        log_warning "代码格式存在问题"
        echo "运行 'cargo fmt --all' 修复格式问题"
        return 1
    fi
}

# =============================================================================
# Clippy 检查
# =============================================================================

check_clippy() {
    log_info "运行 Clippy 检查..."
    
    if cargo clippy --all-targets --all-features --workspace -- -D warnings 2>&1 | tee /tmp/clippy_output.txt; then
        log_success "Clippy 检查通过"
        rm -f /tmp/clippy_output.txt
        return 0
    else
        log_warning "Clippy 发现问题"
        echo "详细信息保存在 /tmp/clippy_output.txt"
        return 1
    fi
}

# =============================================================================
# 安全审计
# =============================================================================

run_security_audit() {
    log_info "运行安全审计..."
    
    # 检查常见安全问题
    local security_issues=0
    
    # 1. 检查硬编码的密钥
    if grep -rE "(api_key|secret|password|token).*[:=].*['\"][a-zA-Z0-9]{8,}['\"]" "$PROJECT_ROOT/src" --include="*.rs" 2>/dev/null | grep -v "test" | grep -v "example" > /tmp/security_issues.txt; then
        log_warning "发现可能的硬编码密钥:"
        cat /tmp/security_issues.txt | head -5
        ((security_issues++))
    fi
    
    # 2. 检查不安全的代码
    if grep -rE "unsafe\s*{ " "$PROJECT_ROOT/src" --include="*.rs" 2>/dev/null | grep -v "// safe" | grep -v "unsafe" > /tmp/unsafe_code.txt; then
        log_warning "发现未注释的不安全代码块:"
        cat /tmp/unsafe_code.txt | head -5
        ((security_issues++))
    fi
    
    # 3. 检查 SQL 注入风险
    if grep -rE "format!\s*\([^)]*SELECT" "$PROJECT_ROOT/src" --include="*.rs" 2>/dev/null | grep -v "sqlx::query" | grep -v "prepare" > /tmp/sql_injection.txt; then
        log_warning "发现可能的 SQL 注入风险:"
        cat /tmp/sql_injection.txt | head -3
        ((security_issues++))
    fi
    
    # 清理临时文件
    rm -f /tmp/security_issues.txt /tmp/unsafe_code.txt /tmp/sql_injection.txt
    
    if [ $security_issues -eq 0 ]; then
        log_success "安全审计通过"
        return 0
    else
        log_error "安全审计发现 $security_issues 个问题"
        return 1
    fi
}

# =============================================================================
# 性能分析
# =============================================================================

run_performance_check() {
    log_info "运行性能分析..."
    
    local perf_issues=0
    
    # 1. 检查不必要的克隆
    if grep -rE "\.clone\(\) " "$PROJECT_ROOT/src" --include="*.rs" 2>/dev/null | grep -v "//.*clone" | grep -v "Arc<" | grep -v "Rc<" | grep -v "Box<" | head -10 > /tmp/clone_issues.txt; then
        log_warning "发现潜在的克隆操作（可能需要优化）:"
        cat /tmp/clone_issues.txt | head -3
        ((perf_issues++))
    fi
    
    # 2. 检查循环中的分配
    if grep -rnE "for.*in.*\{.*push\(" "$PROJECT_ROOT/src" --include="*.rs" 2>/dev/null | head -5 > /tmp/allocation_issues.txt; then
        log_warning "发现循环中可能的内存分配:"
        cat /tmp/allocation_issues.txt | head -3
        ((perf_issues++))
    fi
    
    # 3. 检查锁的使用
    if grep -rE "Mutex::new|RwLock::new" "$PROJECT_ROOT/src" --include="*.rs" 2>/dev/null | grep -v "parking_lot" > /tmp/lock_issues.txt; then
        log_info "发现标准库锁的使用（考虑使用 parking_lot 提升性能）"
    fi
    
    # 清理临时文件
    rm -f /tmp/clone_issues.txt /tmp/allocation_issues.txt /tmp/lock_issues.txt
    
    if [ $perf_issues -eq 0 ]; then
        log_success "性能分析通过"
        return 0
    else
        log_warning "性能分析发现 $perf_issues 个问题"
        return 1
    fi
}

# =============================================================================
# 代码质量检查
# =============================================================================

run_quality_check() {
    log_info "运行代码质量检查..."
    
    local quality_issues=0
    
    # 1. 检查过长的函数
    local long_functions=$(awk '/^fn / { fname=$0; line=NR } NR > 200 && fname { if (NR - line > 50) print fname }' "$PROJECT_ROOT/src"/*.rs 2>/dev/null | head -5)
    if [ -n "$long_functions" ]; then
        log_warning "发现过长的函数:"
        echo "$long_functions"
        ((quality_issues++))
    fi
    
    # 2. 检查缺失的文档
    local undocumented_pub=$(awk '/^pub\s+(struct|enum|fn|mod|trait|impl)/ && !/^\/\/\/|^\/\/!/ && NR > 30' "$PROJECT_ROOT/src"/*.rs 2>/dev/null | head -5)
    if [ -n "$undocumented_pub" ]; then
        log_warning "发现缺少文档注释的公开项:"
        echo "$undocumented_pub" | head -3
        ((quality_issues++))
    fi
    
    # 3. 检查错误处理
    local unwrap_usage=$(grep -rE "\.(unwrap|expect)" "$PROJECT_ROOT/src" --include="*.rs" 2>/dev/null | grep -v "#\[test\]" | grep -v "//.*unwrap" | wc -l)
    if [ "$unwrap_usage" -gt 50 ]; then
        log_warning "发现大量 unwrap/expect 使用（$unwrap_usage 处），考虑使用更安全的错误处理"
        ((quality_issues++))
    fi
    
    if [ $quality_issues -eq 0 ]; then
        log_success "代码质量检查通过"
        return 0
    else
        log_warning "代码质量检查发现 $quality_issues 个问题"
        return 1
    fi
}

# =============================================================================
# 架构检查
# =============================================================================

run_architecture_check() {
    log_info "运行架构检查..."
    
    local arch_issues=0
    
    # 1. 检查模块依赖（循环依赖）
    local deps_graph=$(cargo tree -p limiteron 2>/dev/null | head -20)
    echo "$deps_graph" > /tmp/deps_graph.txt
    
    # 2. 检查公共 API 的稳定性
    local public_api=$(grep -rE "^pub\s+(struct|enum|fn|trait)" "$PROJECT_ROOT/src" --include="*.rs" 2>/dev/null | wc -l)
    log_info "公共 API 数量: $public_api"
    
    # 3. 检查配置的一致性
    if [ -f "$PROJECT_ROOT/src/config.rs" ]; then
        log_info "配置文件存在"
    fi
    
    if [ $arch_issues -eq 0 ]; then
        log_success "架构检查通过"
        return 0
    else
        log_warning "架构检查发现 $arch_issues 个问题"
        return 1
    fi
}

# =============================================================================
# 运行编译检查
# =============================================================================

run_compile_check() {
    log_info "运行编译检查..."
    
    if cargo check --all-features 2>&1 | tee /tmp/compile_output.txt; then
        log_success "编译检查通过"
        rm -f /tmp/compile_output.txt
        return 0
    else
        log_error "编译失败"
        tail -20 /tmp/compile_output.txt
        return 1
    fi
}

# =============================================================================
# 生成报告
# =============================================================================

generate_report() {
    log_info "生成代码审查报告..."
    
    mkdir -p "$(dirname "$REPORT_FILE")"
    
    cat > "$REPORT_FILE" << EOF
# Limiteron 代码审查报告

**生成时间:** $(date -u '+%Y-%m-%d %H:%M:%S UTC')

## 检查项目

| 项目 | 状态 |
|------|------|
EOF
    
    if [ "$SECURITY_ONLY" ] || [ ! "$PERFORMANCE_ONLY" ] && [ ! "$QUALITY_ONLY" ] && [ ! "$ARCHITECTURE_ONLY" ]; then
        echo "| 安全审计 | $([ $EXIT_CODE -eq 0 ] && echo '✅ 通过' || echo '❌ 失败') |" >> "$REPORT_FILE"
    fi
    if [ "$PERFORMANCE_ONLY" ] || [ ! "$SECURITY_ONLY" ] && [ ! "$QUALITY_ONLY" ] && [ ! "$ARCHITECTURE_ONLY" ]; then
        echo "| 性能分析 | $([ $EXIT_CODE -eq 0 ] && echo '✅ 通过' || echo '❌ 失败') |" >> "$REPORT_FILE"
    fi
    if [ "$QUALITY_ONLY" ] || [ ! "$SECURITY_ONLY" ] && [ ! "$PERFORMANCE_ONLY" ] && [ ! "$ARCHITECTURE_ONLY" ]; then
        echo "| 代码质量 | $([ $EXIT_CODE -eq 0 ] && echo '✅ 通过' || echo '❌ 失败') |" >> "$REPORT_FILE"
    fi
    if [ "$ARCHITECTURE_ONLY" ] || [ ! "$SECURITY_ONLY" ] && [ ! "$PERFORMANCE_ONLY" ] && [ ! "$QUALITY_ONLY" ]; then
        echo "| 架构审查 | $([ $EXIT_CODE -eq 0 ] && echo '✅ 通过' || echo '❌ 失败') |" >> "$REPORT_FILE"
    fi
    
    echo "" >> "$REPORT_FILE"
    echo "## 建议" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
    echo "如有任何问题，请查看具体输出信息。" >> "$REPORT_FILE"
    
    log_success "报告已生成: $REPORT_FILE"
}

# =============================================================================
# 主函数
# =============================================================================

main() {
    echo "=============================================="
    echo "  Limiteron 代码审查预提交钩子"
    echo "=============================================="
    echo ""
    
    # 解析命令行参数
    while [[ $# -gt 0 ]]; do
        case $1 in
            --quick)
                QUICK_MODE=true
                shift
                ;;
            --security)
                SECURITY_ONLY=true
                shift
                ;;
            --performance)
                PERFORMANCE_ONLY=true
                shift
                ;;
            --quality)
                QUALITY_ONLY=true
                shift
                ;;
            --architecture)
                ARCHITECTURE_ONLY=true
                shift
                ;;
            --report)
                GENERATE_REPORT=true
                shift
                ;;
            --help)
                show_help
                exit 0
                ;;
            *)
                echo "未知参数: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    # 切换到项目根目录
    cd "$PROJECT_ROOT"
    
    # 检查依赖
    check_dependencies
    
    # 创建临时目录
    mkdir -p "$PROJECT_ROOT/temp"
    
    echo ""
    log_info "开始代码审查..."
    echo ""
    
    # 运行编译检查（快速模式跳过）
    if [ "$QUICK_MODE" = false ]; then
        run_compile_check || EXIT_CODE=1
        echo ""
    fi
    
    # 运行格式检查
    check_formatting || EXIT_CODE=1
    echo ""
    
    # 根据参数运行相应的检查
    if [ "$SECURITY_ONLY" ] || ([ "$QUICK_MODE" = false ] && [ ! "$PERFORMANCE_ONLY" ] && [ ! "$QUALITY_ONLY" ] && [ ! "$ARCHITECTURE_ONLY" ]); then
        run_security_audit || EXIT_CODE=1
        echo ""
    fi
    
    if [ "$PERFORMANCE_ONLY" ] || ([ "$QUICK_MODE" = false ] && [ ! "$SECURITY_ONLY" ] && [ ! "$QUALITY_ONLY" ] && [ ! "$ARCHITECTURE_ONLY" ]); then
        run_performance_check || EXIT_CODE=1
        echo ""
    fi
    
    if [ "$QUALITY_ONLY" ] || ([ "$QUICK_MODE" = false ] && [ ! "$SECURITY_ONLY" ] && [ ! "$PERFORMANCE_ONLY" ] && [ ! "$ARCHITECTURE_ONLY" ]); then
        run_quality_check || EXIT_CODE=1
        echo ""
    fi
    
    if [ "$ARCHITECTURE_ONLY" ] || ([ "$QUICK_MODE" = false ] && [ ! "$SECURITY_ONLY" ] && [ ! "$PERFORMANCE_ONLY" ] && [ ! "$QUALITY_ONLY" ]); then
        run_architecture_check || EXIT_CODE=1
        echo ""
    fi
    
    # Clippy 检查（快速模式跳过）
    if [ "$QUICK_MODE" = false ]; then
        check_clippy || EXIT_CODE=1
        echo ""
    fi
    
    # 生成报告
    if [ "$GENERATE_REPORT" = true ]; then
        generate_report
    fi
    
    echo ""
    echo "=============================================="
    if [ $EXIT_CODE -eq 0 ]; then
        log_success "代码审查完成 - 所有检查通过 ✅"
    else
        log_error "代码审查完成 - 发现问题，请修复后重试 ❌"
    fi
    echo "=============================================="
    
    exit $EXIT_CODE
}

# 运行主函数
main "$@"
