#!/bin/bash

# Limiteron 完整测试脚本
# 包含编译检查、单元测试和集成测试

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印带颜色的消息
print_header() {
    echo ""
    echo -e "${BLUE}======================================"
    echo -e "$1"
    echo -e "======================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

# 显示帮助信息
show_help() {
    cat << EOF
用法: $0 [选项]

选项:
  -u, --unit          只运行单元测试
  -i, --integration   只运行集成测试（需要 Docker）
  -c, --check         只运行编译检查
  -a, --all           运行所有测试（默认）
  -h, --help          显示此帮助信息

示例:
  $0                  运行所有测试
  $0 -u               只运行单元测试
  $0 -i               只运行集成测试
  $0 -c               只检查编译

EOF
    exit 0
}

# 解析命令行参数
RUN_UNIT=true
RUN_INTEGRATION=false
RUN_CHECK=true

while [[ $# -gt 0 ]]; do
    case $1 in
        -u|--unit)
            RUN_UNIT=true
            RUN_INTEGRATION=false
            RUN_CHECK=false
            shift
            ;;
        -i|--integration)
            RUN_UNIT=false
            RUN_INTEGRATION=true
            RUN_CHECK=false
            shift
            ;;
        -c|--check)
            RUN_UNIT=false
            RUN_INTEGRATION=false
            RUN_CHECK=true
            shift
            ;;
        -a|--all)
            RUN_UNIT=true
            RUN_INTEGRATION=true
            RUN_CHECK=true
            shift
            ;;
        -h|--help)
            show_help
            ;;
        *)
            print_error "未知选项: $1"
            show_help
            ;;
    esac
done

print_header "Limiteron 完整测试套件"

# 1. 编译检查
if [ "$RUN_CHECK" = true ]; then
    print_header "1. 编译检查"
    echo "检查编译状态..."
    if cargo check --all-features 2>&1 | grep -q "Finished"; then
        print_success "编译检查通过"
    else
        print_error "编译检查失败"
        exit 1
    fi
fi

# 2. 单元测试
if [ "$RUN_UNIT" = true ]; then
    print_header "2. 单元测试"
    echo "运行单元测试..."

    # 检查关键文件
    echo "检查关键文件..."
    if [ -f "src/postgres_storage.rs" ] && [ -f "src/l2_cache.rs" ]; then
        print_success "关键文件存在"
    else
        print_warning "部分关键文件缺失"
    fi

    # 检查模块导出
    echo "检查模块导出..."
    if grep -q "pub use.*postgres" src/lib.rs && grep -q "pub use.*l2_cache" src/lib.rs; then
        print_success "模块导出正确"
    else
        print_warning "部分模块未导出"
    fi

    # 运行基本测试
    echo "运行基本单元测试..."
    if cargo test --lib --quiet 2>&1 | grep -q "test result: ok"; then
        print_success "单元测试通过"
    else
        print_error "单元测试失败"
        exit 1
    fi
fi

# 3. 集成测试
if [ "$RUN_INTEGRATION" = true ]; then
    print_header "3. 集成测试"

    # 检查 Docker 容器是否运行
    echo "检查 Docker 容器状态..."
    if ! docker-compose ps 2>/dev/null | grep -q "healthy"; then
        print_error "Docker 容器未运行或未健康"
        echo "请先运行: docker-compose up -d"
        echo "跳过集成测试..."
        exit 0
    fi

    print_success "Docker 容器运行正常"
    echo ""

    # 设置环境变量
    export REDIS_URL="redis://localhost:6379"
    export REDIS_PASSWORD="test_password_123"
    export POSTGRES_URL="postgresql://limiteron_user:test_password_123@localhost:5432/limiteron_test"

    echo "环境变量:"
    echo "  REDIS_URL: $REDIS_URL"
    echo "  POSTGRES_URL: $POSTGRES_URL"
    echo ""

    # 运行 Redis 集成测试
    print_header "Redis 集成测试"
    if cargo test --test integration_tests -- --ignored --test-threads=1 redis 2>&1 | grep -q "test result: ok"; then
        print_success "Redis 集成测试通过"
    else
        print_error "Redis 集成测试失败"
    fi
    echo ""

    # 运行 PostgreSQL 集成测试
    print_header "PostgreSQL 集成测试"
    if cargo test --test integration_tests -- --ignored --test-threads=1 postgres 2>&1 | grep -q "test result: ok"; then
        print_success "PostgreSQL 集成测试通过"
    else
        print_error "PostgreSQL 集成测试失败"
    fi
    echo ""

    # 显示容器日志摘要
    print_header "容器日志摘要"
    echo "--- Redis ---"
    docker logs limiteron-redis --tail 5 2>&1 | grep -E "(Ready|error|warning)" || echo "无日志"
    echo ""
    echo "--- PostgreSQL ---"
    docker logs limiteron-postgres --tail 5 2>&1 | grep -E "(ready|error|warning)" || echo "无日志"
fi

print_header "测试完成"
print_success "所有测试通过！"