-- ============================================================================
-- RUNTIME CONFIGURATION TABLES
-- Stores server configuration that can be changed at runtime via admin UI
-- Changes take effect immediately without server restart
-- ============================================================================

-- Main configuration table
-- Stores key-value pairs with metadata for validation and organization
CREATE TABLE runtime_config (
    key VARCHAR(128) PRIMARY KEY,
    value JSONB NOT NULL,
    category VARCHAR(64) NOT NULL,
    description TEXT,
    value_type VARCHAR(32) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by TEXT,
    version INTEGER NOT NULL DEFAULT 1
);

-- Indexes for efficient lookups
CREATE INDEX idx_runtime_config_category ON runtime_config(category);
CREATE INDEX idx_runtime_config_updated_at ON runtime_config(updated_at);

-- Comments for documentation
COMMENT ON TABLE runtime_config IS 'Stores runtime-configurable server settings that override static config';
COMMENT ON COLUMN runtime_config.key IS 'Unique key identifying the setting (e.g., fhir.search.default_count)';
COMMENT ON COLUMN runtime_config.value IS 'Setting value stored as JSONB for type flexibility';
COMMENT ON COLUMN runtime_config.category IS 'Setting category for UI organization (logging, search, interactions, etc.)';
COMMENT ON COLUMN runtime_config.description IS 'Human-readable description of the setting';
COMMENT ON COLUMN runtime_config.value_type IS 'Type of value (boolean, integer, string, string_enum)';
COMMENT ON COLUMN runtime_config.updated_at IS 'Timestamp of last update';
COMMENT ON COLUMN runtime_config.updated_by IS 'Identifier of who made the change';
COMMENT ON COLUMN runtime_config.version IS 'Optimistic locking version number';

-- ============================================================================
-- RUNTIME CONFIGURATION AUDIT LOG
-- Tracks all changes to runtime configuration for compliance and debugging
-- ============================================================================

CREATE TABLE runtime_config_audit (
    id BIGSERIAL PRIMARY KEY,
    key VARCHAR(128) NOT NULL,
    old_value JSONB,
    new_value JSONB NOT NULL,
    changed_by TEXT,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    change_type VARCHAR(20) NOT NULL  -- 'create', 'update', 'delete', 'reset'
);

-- Indexes for efficient audit log queries
CREATE INDEX idx_runtime_config_audit_key ON runtime_config_audit(key);
CREATE INDEX idx_runtime_config_audit_changed_at ON runtime_config_audit(changed_at DESC);
CREATE INDEX idx_runtime_config_audit_change_type ON runtime_config_audit(change_type);

-- Comments for documentation
COMMENT ON TABLE runtime_config_audit IS 'Audit trail for all runtime configuration changes';
COMMENT ON COLUMN runtime_config_audit.key IS 'Configuration key that was changed';
COMMENT ON COLUMN runtime_config_audit.old_value IS 'Previous value (NULL for create operations)';
COMMENT ON COLUMN runtime_config_audit.new_value IS 'New value after the change';
COMMENT ON COLUMN runtime_config_audit.changed_by IS 'Identifier of who made the change';
COMMENT ON COLUMN runtime_config_audit.changed_at IS 'Timestamp of the change';
COMMENT ON COLUMN runtime_config_audit.change_type IS 'Type of change: create, update, delete, reset';

-- ============================================================================
-- NOTIFY TRIGGER FOR CACHE INVALIDATION
-- Sends PostgreSQL NOTIFY when config changes for multi-instance cache sync
-- ============================================================================

CREATE OR REPLACE FUNCTION notify_runtime_config_change() RETURNS trigger AS $$
BEGIN
    -- Send notification with the changed key
    PERFORM pg_notify('runtime_config_changed', NEW.key);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger on INSERT or UPDATE
CREATE TRIGGER runtime_config_notify
    AFTER INSERT OR UPDATE ON runtime_config
    FOR EACH ROW EXECUTE FUNCTION notify_runtime_config_change();

-- Also notify on DELETE
CREATE OR REPLACE FUNCTION notify_runtime_config_delete() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('runtime_config_changed', OLD.key);
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER runtime_config_notify_delete
    AFTER DELETE ON runtime_config
    FOR EACH ROW EXECUTE FUNCTION notify_runtime_config_delete();

COMMENT ON FUNCTION notify_runtime_config_change() IS 'Triggers LISTEN/NOTIFY for cache invalidation on config changes';
COMMENT ON FUNCTION notify_runtime_config_delete() IS 'Triggers LISTEN/NOTIFY for cache invalidation on config deletion';

-- ============================================================================
-- AUDIT TRIGGER
-- Automatically records all changes to runtime_config_audit
-- ============================================================================

CREATE OR REPLACE FUNCTION audit_runtime_config_change() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO runtime_config_audit (key, old_value, new_value, changed_by, change_type)
        VALUES (NEW.key, NULL, NEW.value, NEW.updated_by, 'create');
        RETURN NEW;
    ELSIF TG_OP = 'UPDATE' THEN
        INSERT INTO runtime_config_audit (key, old_value, new_value, changed_by, change_type)
        VALUES (NEW.key, OLD.value, NEW.value, NEW.updated_by, 'update');
        RETURN NEW;
    ELSIF TG_OP = 'DELETE' THEN
        INSERT INTO runtime_config_audit (key, old_value, new_value, changed_by, change_type)
        VALUES (OLD.key, OLD.value, 'null'::jsonb, NULL, 'delete');
        RETURN OLD;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER runtime_config_audit_trigger
    AFTER INSERT OR UPDATE OR DELETE ON runtime_config
    FOR EACH ROW EXECUTE FUNCTION audit_runtime_config_change();

COMMENT ON FUNCTION audit_runtime_config_change() IS 'Automatically records all configuration changes to audit log';
