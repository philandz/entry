CREATE TABLE IF NOT EXISTS entries (
    id              CHAR(36)      NOT NULL PRIMARY KEY,
    budget_id       CHAR(36)      NOT NULL,
    category_id     CHAR(36)      NULL,
    kind            VARCHAR(20)   NOT NULL,
    amount_minor    BIGINT        NOT NULL,
    currency_code   CHAR(3)       NOT NULL DEFAULT 'VND',
    entry_date      DATE          NOT NULL,
    description     TEXT          NULL,
    notes           TEXT          NULL,
    tags            VARCHAR(500)  NULL,
    is_recurring    TINYINT(1)    NOT NULL DEFAULT 0,
    has_attachment  TINYINT(1)    NOT NULL DEFAULT 0,
    recurrence_rule  VARCHAR(255)  NULL,
    next_occurrence  DATE          NULL,
    split_group_id   CHAR(36)       NULL,
    split_total      BIGINT        NULL,
    comment_count   INT           NOT NULL DEFAULT 0,
    attachment_count INT          NOT NULL DEFAULT 0,
    created_by      CHAR(36)      NOT NULL,
    updated_by      CHAR(36)       NULL,
    created_at      DATETIME      NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME      NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    deleted_at      DATETIME      NULL,
    INDEX idx_entries_budget    (budget_id),
    INDEX idx_entries_category  (category_id),
    INDEX idx_entries_date      (entry_date),
    INDEX idx_entries_kind      (kind),
    INDEX idx_entries_recurring (next_occurrence, deleted_at),
    INDEX idx_entries_split     (split_group_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS entry_comments (
    id           CHAR(36)  NOT NULL PRIMARY KEY,
    entry_id     CHAR(36)  NOT NULL,
    comment_text TEXT       NOT NULL,
    user_id      CHAR(36)  NOT NULL,
    created_at   DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at   DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    deleted_at   DATETIME    NULL,
    INDEX idx_comments_entry (entry_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS entry_attachments (
    id         CHAR(36)     NOT NULL PRIMARY KEY,
    entry_id   CHAR(36)     NOT NULL,
    file_id    CHAR(36)     NOT NULL,
    file_name  VARCHAR(512) NOT NULL DEFAULT '',
    user_id    CHAR(36)     NOT NULL,
    created_at DATETIME       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_attachments_entry (entry_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;