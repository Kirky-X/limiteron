-- Limiteron 测试数据库初始化脚本
--
-- 本文件由 src/dbnexus_entities/* 的 create_table_ddl() 生成的实体 DDL 同步维护，
-- 表名/列与代码实体逐一对应（limiteron_kv / limiteron_bans / limiteron_quotas /
-- limiteron_rate_limits）。若实体 schema 变更，请同步本文件（cmake 由
-- create_all_tables_ddl() 聚合，可作为唯一事实来源）。

-- 键值存储表（key_value.rs）
CREATE TABLE IF NOT EXISTS limiteron_kv (
    key VARCHAR(255) PRIMARY KEY,
    value TEXT NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_kv_expires ON limiteron_kv(expires_at)
    WHERE expires_at IS NOT NULL;

-- 封禁记录表（ban_record.rs）
CREATE TABLE IF NOT EXISTS limiteron_bans (
    id BIGSERIAL PRIMARY KEY,
    target_type VARCHAR(50) NOT NULL,
    target_value TEXT NOT NULL,
    target_key VARCHAR(511) NOT NULL UNIQUE,
    ban_times INTEGER NOT NULL DEFAULT 1,
    duration BIGINT NOT NULL,
    banned_at TIMESTAMP WITH TIME ZONE NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    is_manual BOOLEAN NOT NULL DEFAULT FALSE,
    reason TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ban_expires ON limiteron_bans(expires_at)
    WHERE expires_at IS NOT NULL;

-- 配额记录表（quota_record.rs）
CREATE TABLE IF NOT EXISTS limiteron_quotas (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    resource VARCHAR(255) NOT NULL,
    quota_key VARCHAR(511) NOT NULL UNIQUE,
    "limit" BIGINT NOT NULL,
    consumed BIGINT NOT NULL DEFAULT 0,
    window_start TIMESTAMP WITH TIME ZONE NOT NULL,
    window_end TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_quota_key ON limiteron_quotas(quota_key);
CREATE INDEX IF NOT EXISTS idx_quota_user ON limiteron_quotas(user_id, resource);

-- 限流记录表（rate_limit.rs）
CREATE TABLE IF NOT EXISTS limiteron_rate_limits (
    id BIGSERIAL PRIMARY KEY,
    rate_key VARCHAR(511) NOT NULL UNIQUE,
    count BIGINT NOT NULL DEFAULT 0,
    rate BIGINT NOT NULL,
    capacity BIGINT NOT NULL,
    last_update TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_rate_key ON limiteron_rate_limits(rate_key);

-- 测试种子数据
INSERT INTO limiteron_kv (key, value) VALUES
    ('test_key', 'test_value'),
    ('flow_control_config', '{}'),
    ('test_quota_limit', '1000')
ON CONFLICT (key) DO NOTHING;

DO $$
BEGIN
    RAISE NOTICE 'Created tables: limiteron_kv, limiteron_bans, limiteron_quotas, limiteron_rate_limits';
END $$;
