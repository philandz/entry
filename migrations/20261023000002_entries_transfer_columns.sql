-- Transfer support on entries (plan 2026-10-23, T0.1).
--
-- Adds the columns that link the two legs of a budget-to-budget transfer.
-- Purely additive and idempotent: each column / index is only added when
-- missing, so it is safe on prod databases that drifted from the v1 schema.
--
--   transfer_group_id       UUID shared by both legs of one transfer
--   counterparty_budget_id  budget on the other side of the transfer
--   counterparty_kind       'entry' (both legs are entries) | 'fund_cash'
--                           (the other leg is a cash movement in a saving fund)
--   idempotency_key         client-supplied key; (budget_id, key) is unique

SET @dbname = DATABASE();

-- transfer_group_id
SET @sql = IF((SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
               WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = 'entries'
                 AND COLUMN_NAME = 'transfer_group_id') > 0,
              'SELECT 1',
              'ALTER TABLE entries ADD COLUMN transfer_group_id CHAR(36) NULL');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;

-- counterparty_budget_id
SET @sql = IF((SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
               WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = 'entries'
                 AND COLUMN_NAME = 'counterparty_budget_id') > 0,
              'SELECT 1',
              'ALTER TABLE entries ADD COLUMN counterparty_budget_id CHAR(36) NULL');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;

-- counterparty_kind
SET @sql = IF((SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
               WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = 'entries'
                 AND COLUMN_NAME = 'counterparty_kind') > 0,
              'SELECT 1',
              'ALTER TABLE entries ADD COLUMN counterparty_kind VARCHAR(16) NULL');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;

-- idempotency_key
SET @sql = IF((SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
               WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = 'entries'
                 AND COLUMN_NAME = 'idempotency_key') > 0,
              'SELECT 1',
              'ALTER TABLE entries ADD COLUMN idempotency_key VARCHAR(64) NULL');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;

-- idx_entries_transfer_group
SET @sql = IF((SELECT COUNT(*) FROM INFORMATION_SCHEMA.STATISTICS
               WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = 'entries'
                 AND INDEX_NAME = 'idx_entries_transfer_group') > 0,
              'SELECT 1',
              'ALTER TABLE entries ADD INDEX idx_entries_transfer_group (transfer_group_id)');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;

-- uk_entries_idem (MySQL allows many NULLs in a UNIQUE index, so legacy rows are fine)
SET @sql = IF((SELECT COUNT(*) FROM INFORMATION_SCHEMA.STATISTICS
               WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = 'entries'
                 AND INDEX_NAME = 'uk_entries_idem') > 0,
              'SELECT 1',
              'ALTER TABLE entries ADD UNIQUE KEY uk_entries_idem (budget_id, idempotency_key)');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;
