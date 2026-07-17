-- Primary list path: budget + soft-delete + default sort
ALTER TABLE entries
  ADD INDEX idx_entries_list_main (budget_id, deleted_at, entry_date);
