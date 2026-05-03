CREATE TABLE IF NOT EXISTS budget_entries (
    id           VARCHAR(36)   NOT NULL PRIMARY KEY,
    budget_id    VARCHAR(36)   NOT NULL,
    category_id  VARCHAR(36)            DEFAULT NULL,
    kind         VARCHAR(10)   NOT NULL DEFAULT 'expense' COMMENT 'expense | income',
    amount       BIGINT        NOT NULL,
    description  VARCHAR(512)  NOT NULL DEFAULT '',
    entry_date   DATE          NOT NULL,
    tags         TEXT                   DEFAULT NULL COMMENT 'comma-separated',
    notes        TEXT                   DEFAULT NULL,
    is_recurring BOOLEAN       NOT NULL DEFAULT FALSE,
    created_by   VARCHAR(36)   NOT NULL,
    created_at   BIGINT        NOT NULL,
    updated_at   BIGINT        NOT NULL,
    deleted_at   BIGINT                 DEFAULT NULL,
    INDEX idx_entries_budget   (budget_id),
    INDEX idx_entries_category (category_id),
    INDEX idx_entries_date     (entry_date),
    INDEX idx_entries_kind     (kind)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS entry_comments (
    id         VARCHAR(36)  NOT NULL PRIMARY KEY,
    entry_id   VARCHAR(36)  NOT NULL,
    body       TEXT         NOT NULL,
    created_by VARCHAR(36)  NOT NULL,
    created_at BIGINT       NOT NULL,
    updated_at BIGINT       NOT NULL,
    deleted_at BIGINT                DEFAULT NULL,
    INDEX idx_comments_entry (entry_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS entry_attachments (
    id         VARCHAR(36)  NOT NULL PRIMARY KEY,
    entry_id   VARCHAR(36)  NOT NULL,
    file_id    VARCHAR(36)  NOT NULL,
    file_name  VARCHAR(512) NOT NULL DEFAULT '',
    created_by VARCHAR(36)  NOT NULL,
    created_at BIGINT       NOT NULL,
    INDEX idx_attachments_entry (entry_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
