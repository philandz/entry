-- Primary list path: budget + soft-delete + default sort
--
-- Idempotent: the index may already exist (this migration was previously
-- numbered 20260713000001, which collided with the sharing service's
-- migration of the same version in the shared `_sqlx_migrations` table).
SET @dbname = DATABASE();
SET @tablename := 'entries';

SET @has_index := (SELECT COUNT(*) FROM INFORMATION_SCHEMA.STATISTICS
                   WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = @tablename
                     AND INDEX_NAME = 'idx_entries_list_main');
SET @sql := IF(@has_index = 0,
               'ALTER TABLE entries ADD INDEX idx_entries_list_main (budget_id, deleted_at, entry_date)',
               'SELECT 1');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;
