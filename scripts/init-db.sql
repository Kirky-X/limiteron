-- Limiteron 测试数据库初始化脚本

-- 创建配额使用表
CREATE TABLE IF NOT EXISTS quota_usage (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    resource_key VARCHAR(255) NOT NULL,
    quota_type VARCHAR(50) NOT NULL,
    consumed BIGINT NOT NULL DEFAULT 0,
    limit_value BIGINT NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(user_id, resource_key, window_start)
);

CREATE INDEX idx_quota_window ON quota_usage(user_id, resource_key, window_start);

-- 创建封禁记录表
CREATE TABLE IF NOT EXISTS ban_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_type VARCHAR(20) NOT NULL,
    target_value VARCHAR(255) NOT NULL,
    reason TEXT,
    ban_times INTEGER NOT NULL DEFAULT 1,
    duration_secs BIGINT NOT NULL,
    banned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    is_manual BOOLEAN NOT NULL DEFAULT false,
    unbanned_at TIMESTAMPTZ,
    unbanned_by VARCHAR(255)
);

CREATE INDEX idx_ban_active ON ban_records(target_type, target_value, expires_at)
    WHERE unbanned_at IS NULL;

-- 添加唯一约束用于增量更新 (使用 IMMUTABLE 函数)
CREATE OR REPLACE FUNCTION current_timestamp_immutable()
RETURNS TIMESTAMPTZ AS $$
BEGIN
    RETURN CURRENT_TIMESTAMP;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE UNIQUE INDEX IF NOT EXISTS idx_ban_active_unique ON ban_records(target_type, target_value)
    WHERE unbanned_at IS NULL AND expires_at > current_timestamp_immutable();

-- 创建通用键值存储表
CREATE TABLE IF NOT EXISTS kv_store (
    key VARCHAR(255) PRIMARY KEY,
    value TEXT NOT NULL,
    expires_at TIMESTAMPTZ
);

CREATE INDEX idx_kv_expires ON kv_store(expires_at)
    WHERE expires_at IS NOT NULL;

-- 插入测试数据
INSERT INTO kv_store (key, value) VALUES
    ('test_key', 'test_value'),
    ('flow_control_config', '{}'),
    ('test_quota_limit', '1000');

-- 创建测试用户配额
INSERT INTO quota_usage (user_id, resource_key, quota_type, consumed, limit_value, window_start, window_end)
VALUES
    ('test_user_1', 'api', 'default', 50, 1000, now(), now() + INTERVAL '1 hour'),
    ('test_user_2', 'api', 'default', 100, 500, now(), now() + INTERVAL '1 hour');

-- 创建测试封禁记录
INSERT INTO ban_records (target_type, target_value, reason, ban_times, duration_secs, expires_at)
VALUES
    ('ip', '192.168.1.100', 'Test ban', 1, 300, now() + INTERVAL '5 minutes'),
    ('user', 'test_banned_user', 'Excessive requests', 2, 1800, now() + INTERVAL '30 minutes');

-- 输出初始化完成信息
DO $$
BEGIN
    RAISE NOTICE 'Limiteron test database initialized successfully';
    RAISE NOTICE 'Created tables: quota_usage, ban_records, kv_store';
    RAISE NOTICE 'Test data inserted';
END $$;