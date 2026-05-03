-- Add recurring and split fields to budget_entries
ALTER TABLE budget_entries
    ADD COLUMN recurrence_rule VARCHAR(255) NULL COMMENT 'RRULE string e.g. FREQ=MONTHLY;INTERVAL=1' AFTER is_recurring,
    ADD COLUMN next_occurrence  DATE         NULL COMMENT 'Next scheduled occurrence date'             AFTER recurrence_rule,
    ADD COLUMN split_group_id   VARCHAR(36)  NULL COMMENT 'UUID linking split legs together'           AFTER next_occurrence,
    ADD COLUMN split_total      BIGINT       NULL COMMENT 'Total amount of the split group'            AFTER split_group_id;

-- Index for scheduler: find all active recurring entries due today or earlier
CREATE INDEX idx_entries_next_occurrence
    ON budget_entries (next_occurrence, deleted_at);

-- Index for split leg lookup
CREATE INDEX idx_entries_split_group
    ON budget_entries (split_group_id);
