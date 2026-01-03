#!/bin/bash

# Python 项目本地 CI 预检脚本
# 在提交前运行所有 CI 检查，确保流水线能够通过
# 使用方法: ./pre-commit-check.sh

set -e
set -o pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 图标
CHECK="✓"
CROSS="✗"
ARROW="→"

# 统计变量
TOTAL_CHECKS=0
PASSED_CHECKS=0
FAILED_CHECKS=0

# 开始时间
START_TIME=$(date +%s)

# 打印标题
print_header() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  🐍 Python 项目本地 CI 预检${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

# 打印步骤
print_step() {
    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
    local cmd="$1"
    local description="$2"
    echo -e "${BLUE}[${TOTAL_CHECKS}/${EXPECTED_CHECKS}]${NC} ${ARROW} $description..."
    if [ -n "$cmd" ]; then
        echo -e "  ${YELLOW}运行命令: ${cmd}${NC}"
    fi
    echo ""
}

# 检查命令是否存在
check_command() {
    command -v "$1" &> /dev/null
}

# 总检查数
EXPECTED_CHECKS=8

# 打印标题
print_header

# 检查是否在 git 仓库中
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo -e "${RED}  ${CROSS} 当前目录不是 git 仓库${NC}"
    echo ""
    exit 1
fi

# 检查是否在 Python 项目中
if [ ! -f "pyproject.toml" ] && [ ! -f "setup.py" ] && [ ! -f "setup.cfg" ]; then
    echo -e "${RED}  ${CROSS} 未找到 pyproject.toml、setup.py 或 setup.cfg${NC}"
    echo ""
    echo -e "${BLUE}💡 请在 Python 项目根目录运行此脚本${NC}"
    echo ""
    exit 1
fi

# 检查 Python
if ! check_command python && ! check_command python3; then
    echo -e "${RED}  ${CROSS} 未安装 Python${NC}"
    echo ""
    exit 1
fi

# 使用 python3 或 python
PYTHON_CMD="python3"
if ! check_command python3; then
    PYTHON_CMD="python"
fi

echo -e "${GREEN}  ${CHECK} 环境检查通过${NC}"
echo -e "  Python: $($PYTHON_CMD --version)"
echo ""
echo -e "${BLUE}────────────────────────────────────────────────────────${NC}"
echo ""

# ============================================================================
# 1. 依赖安装检查
# ============================================================================
print_step "$PYTHON_CMD -m pip install -e \".[dev,test]\"" "检查并安装依赖"

if [ -f "pyproject.toml" ]; then
    if $PYTHON_CMD -m pip install -e ".[dev,test]" > /tmp/pip_install.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} 依赖安装成功${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} 依赖安装失败${NC}"
        echo ""
        echo -e "${BLUE}💡 错误日志:${NC}"
        tail -20 /tmp/pip_install.log
        echo ""
        exit 1
    fi
elif [ -f "requirements.txt" ]; then
    if $PYTHON_CMD -m pip install -r requirements.txt > /tmp/pip_install.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} 依赖安装成功${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} 依赖安装失败${NC}"
        echo ""
        echo -e "${BLUE}💡 错误日志:${NC}"
        tail -20 /tmp/pip_install.log
        echo ""
        exit 1
    fi
else
    echo -e "${YELLOW}  ⚠ 未找到依赖配置文件，跳过依赖安装${NC}"
    echo ""
fi

# ============================================================================
# 2. 代码格式检查 (ruff format 或 black)
# ============================================================================
print_step "ruff format --check ." "检查代码格式"

if check_command ruff; then
    if ruff format --check . > /tmp/ruff_format.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} 代码格式检查通过 (ruff)${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} 代码格式检查失败 (ruff)${NC}"
        echo ""
        echo -e "${BLUE}💡 修复命令:${NC}"
        echo -e "  ${YELLOW}ruff format .${NC}"
        echo ""
        echo -e "${BLUE}💡 详细输出:${NC}"
        head -20 /tmp/ruff_format.log
        echo ""
        exit 1
    fi
elif check_command black; then
    if black --check . > /tmp/black.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} 代码格式检查通过 (black)${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} 代码格式检查失败 (black)${NC}"
        echo ""
        echo -e "${BLUE}💡 修复命令:${NC}"
        echo -e "  ${YELLOW}black .${NC}"
        echo ""
        echo -e "${BLUE}💡 详细输出:${NC}"
        head -20 /tmp/black.log
        echo ""
        exit 1
    fi
else
    echo -e "${YELLOW}  ⚠ 未安装 ruff 或 black，跳过格式检查${NC}"
    echo ""
    echo -e "${BLUE}💡 安装命令:${NC}"
    echo -e "  ${YELLOW}pip install ruff${NC}"
    echo ""
fi

# ============================================================================
# 3. Import 排序检查 (ruff 或 isort)
# ============================================================================
print_step "ruff check --select I ." "检查 import 排序"

if check_command ruff; then
    if ruff check --select I . > /tmp/ruff_isort.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} Import 排序检查通过 (ruff)${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} Import 排序检查失败 (ruff)${NC}"
        echo ""
        echo -e "${BLUE}💡 修复命令:${NC}"
        echo -e "  ${YELLOW}ruff check --select I --fix .${NC}"
        echo ""
        exit 1
    fi
elif check_command isort; then
    if isort --check-only . > /tmp/isort.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} Import 排序检查通过 (isort)${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} Import 排序检查失败 (isort)${NC}"
        echo ""
        echo -e "${BLUE}💡 修复命令:${NC}"
        echo -e "  ${YELLOW}isort .${NC}"
        echo ""
        exit 1
    fi
else
    echo -e "${YELLOW}  ⚠ 未安装 ruff 或 isort，跳过 import 排序检查${NC}"
    echo ""
    echo -e "${BLUE}💡 安装命令:${NC}"
    echo -e "  ${YELLOW}pip install ruff${NC}"
    echo ""
fi

# ============================================================================
# 4. Linting 检查 (ruff 或 flake8)
# ============================================================================
print_step "ruff check ." "运行 Lint 检查"

if check_command ruff; then
    if ruff check . > /tmp/ruff_check.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} Ruff lint 检查通过${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} Ruff lint 发现问题${NC}"
        echo ""
        echo -e "${BLUE}💡 详细命令:${NC}"
        echo -e "  ${YELLOW}ruff check .${NC}"
        echo ""
        echo -e "${BLUE}💡 前 20 个问题:${NC}"
        head -20 /tmp/ruff_check.log
        echo ""
        exit 1
    fi
elif check_command flake8; then
    if flake8 . > /tmp/flake8.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} Flake8 检查通过${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} Flake8 发现问题${NC}"
        echo ""
        echo -e "${BLUE}💡 前 20 个问题:${NC}"
        head -20 /tmp/flake8.log
        echo ""
        exit 1
    fi
else
    echo -e "${YELLOW}  ⚠ 未安装 ruff 或 flake8，跳过 lint 检查${NC}"
    echo ""
    echo -e "${BLUE}💡 安装命令:${NC}"
    echo -e "  ${YELLOW}pip install ruff${NC}"
    echo ""
fi

# ============================================================================
# 5. 类型检查 (mypy)
# ============================================================================
print_step "mypy ." "运行类型检查 (mypy)"

if check_command mypy; then
    if mypy . > /tmp/mypy.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} MyPy 类型检查通过${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} MyPy 发现类型问题${NC}"
        echo ""
        echo -e "${BLUE}💡 详细命令:${NC}"
        echo -e "  ${YELLOW}mypy .${NC}"
        echo ""
        echo -e "${BLUE}💡 前 20 个问题:${NC}"
        head -20 /tmp/mypy.log
        echo ""
        exit 1
    fi
else
    echo -e "${YELLOW}  ⚠ 未安装 mypy，跳过类型检查${NC}"
    echo ""
    echo -e "${BLUE}💡 安装命令:${NC}"
    echo -e "  ${YELLOW}pip install mypy${NC}"
    echo ""
fi

# ============================================================================
# 6. 运行测试
# ============================================================================
print_step "pytest -v" "运行所有测试"

if check_command pytest; then
    if pytest -v > /tmp/pytest.log 2>&1; then
        TEST_STATS=$(grep -E "passed|failed" /tmp/pytest.log | tail -1)
        echo -e "${GREEN}  ${CHECK} 所有测试通过${NC}"
        if [ -n "$TEST_STATS" ]; then
            echo -e "  ℹ $TEST_STATS"
            echo ""
        fi
    else
        echo -e "${RED}  ${CROSS} 部分测试失败${NC}"
        echo ""
        echo -e "${BLUE}💡 失败的测试:${NC}"
        grep -A 10 "FAILED" /tmp/pytest.log | head -30
        echo ""
        echo -e "${BLUE}💡 详细命令:${NC}"
        echo -e "  ${YELLOW}pytest -v${NC}"
        echo ""
        exit 1
    fi
else
    echo -e "${YELLOW}  ⚠ 未安装 pytest，跳过测试${NC}"
    echo ""
    echo -e "${BLUE}💡 安装命令:${NC}"
    echo -e "  ${YELLOW}pip install pytest${NC}"
    echo ""
fi

# ============================================================================
# 7. 安全检查
# ============================================================================
print_step "safety scan" "运行安全检查"

SECURITY_SKIP=0

# Safety check
if check_command safety; then
    if safety scan > /tmp/safety.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} Safety 安全检查通过${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} Safety 发现安全漏洞${NC}"
        echo ""
        echo -e "${BLUE}💡 详细命令:${NC}"
        echo -e "  ${YELLOW}safety scan${NC}"
        echo ""
        echo -e "${BLUE}💡 漏洞详情:${NC}"
        head -30 /tmp/safety.log
        echo ""
        exit 1
    fi
else
    echo -e "${YELLOW}  ⚠ 未安装 safety，跳过依赖安全检查${NC}"
    echo ""
    echo -e "${BLUE}💡 安装命令:${NC}"
    echo -e "  ${YELLOW}pip install safety${NC}"
    SECURITY_SKIP=1
fi

# Bandit check
if check_command bandit; then
    print_step "bandit -r . -ll" "运行 Bandit 安全扫描"
    if bandit -r . -ll > /tmp/bandit.log 2>&1; then
        echo -e "${GREEN}  ${CHECK} Bandit 安全扫描通过${NC}"
        echo ""
    else
        echo -e "${RED}  ${CROSS} Bandit 发现安全问题${NC}"
        echo ""
        echo -e "${BLUE}💡 详细命令:${NC}"
        echo -e "  ${YELLOW}bandit -r . -ll${NC}"
        echo ""
        echo -e "${BLUE}💡 安全问题详情:${NC}"
        head -30 /tmp/bandit.log
        echo ""
        exit 1
    fi
else
    echo -e "${YELLOW}  ⚠ 未安装 bandit，跳过代码安全扫描${NC}"
    echo ""
    echo -e "${BLUE}💡 安装命令:${NC}"
    echo -e "  ${YELLOW}pip install bandit${NC}"
    echo ""
fi

# ============================================================================
# 8. 代码覆盖率 (可选)
# ============================================================================
print_step "pytest --cov=. --cov-report=term" "计算代码覆盖率 (可选)"

if check_command pytest && $PYTHON_CMD -c "import pytest_cov" 2>/dev/null; then
    if timeout 300 pytest --cov=. --cov-report=term > /tmp/coverage.log 2>&1; then
        COVERAGE=$(grep -oP '\d+%' /tmp/coverage.log | tail -1)
        if [ -n "$COVERAGE" ]; then
            echo -e "${GREEN}  ${CHECK} 代码覆盖率: $COVERAGE${NC}"
        else
            echo -e "${GREEN}  ${CHECK} 覆盖率计算完成${NC}"
        fi
        echo ""
        grep -A 10 "TOTAL" /tmp/coverage.log || true
        echo ""
    else
        if [ $? -eq 124 ]; then
            echo -e "${YELLOW}  ⚠ 覆盖率计算超时（5分钟），已跳过${NC}"
            echo ""
        else
            echo -e "${YELLOW}  ⚠ 覆盖率计算失败或被跳过${NC}"
            echo ""
        fi
    fi
else
    echo -e "${YELLOW}  ⚠ 未安装 pytest-cov，跳过覆盖率检查${NC}"
    echo ""
    echo -e "${BLUE}💡 安装命令:${NC}"
    echo -e "  ${YELLOW}pip install pytest-cov${NC}"
    echo ""
fi

# ============================================================================
# 总结
# ============================================================================
echo ""
echo -e "${BLUE}────────────────────────────────────────────────────────${NC}"
echo ""

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo -e "${BLUE}📊 检查结果总结${NC}"
echo ""
echo -e "  总检查数: ${BLUE}${TOTAL_CHECKS}${NC}"
echo -e "  耗时: ${BLUE}${DURATION}${NC} 秒"
echo ""

echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  ✨ 所有检查通过！可以安全提交代码${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "${BLUE}推荐的提交流程：${NC}"
echo -e "  1. ${YELLOW}git add .${NC}"
echo -e "  2. ${YELLOW}git commit -m \"your message\"${NC}"
echo -e "  3. ${YELLOW}git push${NC}"
echo ""
