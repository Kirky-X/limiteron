#!/bin/bash

# Limiteron 集成测试启动脚本

set -e

echo "======================================"
echo "Limiteron 集成测试环境"
echo "======================================"
echo ""

# 检查 Docker 是否运行
if ! docker info > /dev/null 2>&1; then
    echo "❌ Docker 未运行，请先启动 Docker"
    exit 1
fi

echo "✅ Docker 正在运行"
echo ""

# 启动 Docker Compose 服务
echo "启动 Docker Compose 服务..."
cd /home/project/limiteron
docker-compose up -d

echo ""
echo "等待服务启动..."
sleep 10

# 检查服务状态
echo ""
echo "服务状态:"
docker-compose ps

echo ""
echo "======================================"
echo "运行集成测试"
echo "======================================"
echo ""

cd /home/project/limiteron/temp/integration_test
cargo run

echo ""
echo "======================================"
echo "集成测试完成"
echo "======================================"
echo ""
echo "查看日志:"
echo "  docker-compose logs"
echo ""
echo "停止服务:"
echo "  docker-compose down"
echo ""