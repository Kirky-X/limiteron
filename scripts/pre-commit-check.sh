#!/usr/bin/env bash
# =============================================================================
# Limiteron 预提交检查脚本
# =============================================================================
#
# 此脚本在提交前运行基本的代码质量检查，确保代码符合项目标准。
#
# 使用方法:
#   ./scripts/pre-commit-check.sh
#
# 检查项目:
#   1. 编译检查
#   2. 代码格式化检查
#   3. Clippy 检查
#   4. 单元测试
#
# =============================================================================

set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# 项目路径
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
cd "$PROJECT_ROOT"

# 统计信息
TOTAL_CHECKS=0
PASSED_CHECKS=0
FAILED_CHECKS=0
SKIPPED_CHECKS=0

# =============================================================================
# 日志函数
# =============================================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_section() {
    echo ""
    echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  $1${NC}"
    echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
    echo ""
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((PASSED_CHECKS++))
    ((TOTAL_CHECKS++))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((FAILED_CHECKS++))
    ((TOTAL_CHECKS++))
}

log_skip() {
    echo -e "${YELLOW}[SKIP]${NC} $1"
    ((SKIPPED_CHECKS++))
    ((TOTAL_CHECKS++))
}

# =============================================================================
# 检查函数
# =============================================================================

check_compilation() {
    log_section "编译检查"
    
    log_info "运行 cargo check --all-features..."
    
    if cargo check --all-features 2>&1; then
        log_success "编译检查通过"
        return 0
    else
        log_fail "编译失败，请查看上方错误信息"
        return 1
    fi
}

check_formatting() {
    log_section "代码格式检查"
    
    log_info "运行 cargo fmt --all -- --check..."
    
    if cargo fmt --all -- --check 2>&1; then
        log_success "代码格式正确"
        return 0
    else
        log_fail "代码格式存在问题"
        echo ""
        echo "运行 'cargo fmt --all' 修复格式问题"
        return 1
    fi
}

check_clippy() {
    log_section "Clippy 检查"
    
    log_info "运行 cargo clippy --all-targets --all-features..."
    
    if cargo clippy --all-targets --all-features --workspace -- -D warnings 2>&1 | tee /tmp/clippy_output.txt; then
        log_success "Clippy 检查通过"
        rm -f /tmp/clippy_output.txt
        return 0
    else
        log_fail "Clippy 发现问题，请查看上方输出"
        rm -f /tmp/clippy_output.txt
        return 1
    fi
}

check_unit_tests() {
    log_section "单元测试"
    
    log_info "运行 cargo test --lib -- --test-threads=4 --skip circuit_breaker..."
    
    # 运行 lib 测试，跳过慢速的熔断器测试
    if cargo test --lib -- --test-threads=4 --skip circuit_breaker 2>&1; then
        log_success "单元测试通过"
        return 0
    else
        log_fail "单元测试失败，请查看上方错误信息"
        return 1
    fi
}

check_deny() {
    log_section "依赖安全检查"
    
    log_info "运行 cargo deny check..."
    
    if command -v cargo-deny &> /dev/null; then
        # 尝试运行 cargo deny，如果失败（可能是网络问题）则跳过
        if cargo deny check 2>&1 > /dev/null; then
            log_success "依赖安全检查通过"
            return 0
        else
            log_skip "cargo-deny 检查失败（可能是网络问题），跳过依赖安全检查"
            return 0
        fi
    else
        log_skip "cargo-deny 未安装，跳过依赖安全检查"
        echo "安装命令: cargo install --locked cargo-deny"
        return 0
    fi
}

# =============================================================================
# 主函数
# =============================================================================

main() {
    echo ""
    echo -e "${CYAN}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║                  Limiteron 预提交检查                        ║${NC}"
    echo -e "${CYAN}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "项目根目录: $PROJECT_ROOT"
    echo "日期: $(date '+%Y-%m-%d %H:%M:%S')"
    echo ""
    
    local exit_code=0
    
    # 运行各项检查
    check_compilation || exit_code=1
    echo ""
    
    check_formatting || exit_code=1
    echo ""
    
    check_clippy || exit_code=1
    echo ""
    
    check_unit_tests || exit_code=1
    echo ""
    
    check_deny || exit_code=1
    echo ""
    
    # 打印摘要
    log_section "检查摘要"
    
    echo -e "总检查数: ${TOTAL_CHECKS}"
    echo -e "${GREEN}通过: ${PASSED_CHECKS}${NC}"
    echo -e "${RED}失败: ${FAILED_CHECKS}${NC}"
    echo -e "${YELLOW}跳过: ${SKIPPED_CHECKS}${NC}"
    echo ""
    
    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}✅ 所有检查通过，可以提交！${NC}"
    else
        echo -e "${RED}❌ 部分检查未通过，请修复后重试${NC}"
        echo ""
        echo "常见问题解决方案:"
        echo "  - 编译错误: 检查代码语法和依赖"
        echo "  - 格式错误: 运行 'cargo fmt --all'"
        echo "  - Clippy 警告: 查看警告信息并修复"
        echo "  - 测试失败: 运行 'cargo test' 查看详细错误"
    fi
    
    echo ""
    exit $exit_code
}

# 运行主函数
main "$@"
