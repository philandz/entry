use anyhow::Result;
use philand_time::now_unix;
use sqlx::MySqlPool;

use crate::converters::{kind_to_db, DbAttachment, DbComment, DbEntry};
use crate::pb::service::entry::{EntryKind, ListParams, PageMeta};

pub struct EntryRepository {
    pool: MySqlPool,
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub struct ListResult {
    pub entries: Vec<DbEntry>,
    pub meta: PageMeta,
}

impl EntryRepository {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = sqlx::MySqlPool::connect(database_url).await?;
        let mut migrator =
            sqlx::migrate::Migrator::new(std::path::Path::new("./migrations")).await?;
        migrator.set_ignore_missing(true);
        migrator.run(&pool).await?;
        Ok(Self { pool })
    }

    // -----------------------------------------------------------------------
    // Entry CRUD
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn create_entry(
        &self,
        budget_id: &str,
        category_id: Option<&str>,
        kind: EntryKind,
        amount: i64,
        description: &str,
        entry_date: &str,
        tags: &[String],
        notes: Option<&str>,
        created_by: &str,
    ) -> Result<DbEntry> {
        self.create_entry_full(
            budget_id,
            category_id,
            kind,
            amount,
            description,
            entry_date,
            tags,
            notes,
            created_by,
            None,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_entry_full(
        &self,
        budget_id: &str,
        category_id: Option<&str>,
        kind: EntryKind,
        amount: i64,
        description: &str,
        entry_date: &str,
        tags: &[String],
        notes: Option<&str>,
        created_by: &str,
        recurrence_rule: Option<&str>,
        split_group_id: Option<&str>,
        split_total: Option<i64>,
    ) -> Result<DbEntry> {
        let id = new_id();
        let now = now_unix();
        let tags_str = tags.join(",");
        let is_recurring = recurrence_rule.is_some();
        // Compute next_occurrence from entry_date + rule (simple: same as entry_date for first insert)
        let next_occ = if is_recurring {
            Some(entry_date.to_string())
        } else {
            None
        };
        sqlx::query(
            "INSERT INTO budget_entries (id, budget_id, category_id, kind, amount, description, entry_date, tags, notes, is_recurring, recurrence_rule, next_occurrence, split_group_id, split_total, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id).bind(budget_id).bind(category_id).bind(kind_to_db(kind))
        .bind(amount).bind(description).bind(entry_date)
        .bind(if tags_str.is_empty() { None } else { Some(tags_str) })
        .bind(notes).bind(is_recurring).bind(recurrence_rule)
        .bind(next_occ.as_deref()).bind(split_group_id).bind(split_total)
        .bind(created_by).bind(now).bind(now)
        .execute(&self.pool).await?;
        self.get_entry(&id).await
    }

    pub async fn get_entry(&self, entry_id: &str) -> Result<DbEntry> {
        let row = sqlx::query_as::<_, DbEntry>(
            "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount, e.description,
                    DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date,
                    e.tags, e.notes, e.is_recurring, e.recurrence_rule,
                    DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence,
                    e.split_group_id, e.split_total,
                    e.created_by, e.created_at, e.updated_at, e.deleted_at,
                    (SELECT COUNT(*) > 0 FROM entry_attachments a WHERE a.entry_id = e.id) AS has_attachment
             FROM budget_entries e
             WHERE e.id = ? AND e.deleted_at IS NULL"
        )
        .bind(entry_id)
        .fetch_one(&self.pool).await?;
        Ok(row)
    }

    pub async fn list_entries(
        &self,
        budget_id: Option<&str>,
        budget_ids: &[String],
        user_id: Option<&str>,
        params: &ListParams,
    ) -> Result<ListResult> {
        let page = params.page.unwrap_or(1).max(1);
        let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
        let offset = ((page - 1) * page_size) as i64;

        let mut conditions = vec!["e.deleted_at IS NULL".to_string()];
        let mut binds: Vec<String> = vec![];

        if let Some(bid) = budget_id {
            conditions.push("e.budget_id = ?".to_string());
            binds.push(bid.to_string());
        } else if !budget_ids.is_empty() {
            let placeholders = budget_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conditions.push(format!("e.budget_id IN ({placeholders})"));
            for id in budget_ids {
                binds.push(id.clone());
            }
        } else if let Some(uid) = user_id {
            conditions.push("e.created_by = ?".to_string());
            binds.push(uid.to_string());
        }

        if let Some(ref q) = params.q {
            if !q.is_empty() {
                conditions.push("e.description LIKE ?".to_string());
                binds.push(format!("%{q}%"));
            }
        }
        if let Some(ref kind) = params.kind {
            if !kind.is_empty() {
                conditions.push("e.kind = ?".to_string());
                binds.push(kind.clone());
            }
        }
        if let Some(ref cat) = params.category_id {
            if !cat.is_empty() {
                conditions.push("e.category_id = ?".to_string());
                binds.push(cat.clone());
            }
        }
        if let Some(ref df) = params.date_from {
            if !df.is_empty() {
                conditions.push("e.entry_date >= ?".to_string());
                binds.push(df.clone());
            }
        }
        if let Some(ref dt) = params.date_to {
            if !dt.is_empty() {
                conditions.push("e.entry_date <= ?".to_string());
                binds.push(dt.clone());
            }
        }
        if let Some(min) = params.amount_min {
            conditions.push("e.amount >= ?".to_string());
            binds.push(min.to_string());
        }
        if let Some(max) = params.amount_max {
            conditions.push("e.amount <= ?".to_string());
            binds.push(max.to_string());
        }

        let where_clause = format!("WHERE {}", conditions.join(" AND "));
        let sort_col = match params.sort_by.as_deref() {
            Some("amount") => "e.amount",
            Some("description") => "e.description",
            _ => "e.entry_date",
        };
        let sort_dir = if params.sort_dir.as_deref() == Some("asc") {
            "ASC"
        } else {
            "DESC"
        };

        // Count
        let count_sql = format!("SELECT COUNT(*) as cnt FROM budget_entries e {where_clause}");
        let mut count_q = sqlx::query(&count_sql);
        for b in &binds {
            count_q = count_q.bind(b);
        }
        let total: i64 = count_q
            .fetch_one(&self.pool)
            .await
            .map(|r| sqlx::Row::try_get::<i64, _>(&r, "cnt").unwrap_or(0))
            .unwrap_or(0);

        // Data
        let data_sql = format!(
            "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount, e.description,
                    DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date,
                    e.tags, e.notes, e.is_recurring, e.recurrence_rule,
                    DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence,
                    e.split_group_id, e.split_total,
                    e.created_by, e.created_at, e.updated_at, e.deleted_at,
                    (SELECT COUNT(*) > 0 FROM entry_attachments a WHERE a.entry_id = e.id) AS has_attachment
             FROM budget_entries e {where_clause}
             ORDER BY {sort_col} {sort_dir}
             LIMIT ? OFFSET ?"
        );
        let mut data_q = sqlx::query_as::<_, DbEntry>(&data_sql);
        for b in &binds {
            data_q = data_q.bind(b);
        }
        data_q = data_q.bind(page_size as i64).bind(offset);
        let entries = data_q.fetch_all(&self.pool).await?;

        let total_pages = ((total as f64) / (page_size as f64)).ceil() as i32;
        Ok(ListResult {
            entries,
            meta: PageMeta {
                page,
                page_size,
                total_pages: total_pages.max(1),
                total_rows: total,
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_entry(
        &self,
        entry_id: &str,
        category_id: Option<&str>,
        kind: Option<EntryKind>,
        amount: Option<i64>,
        description: Option<&str>,
        entry_date: Option<&str>,
        tags: Option<&[String]>,
        notes: Option<&str>,
    ) -> Result<DbEntry> {
        let now = now_unix();
        let mut parts: Vec<String> = vec!["updated_at = ?".to_string()];
        if category_id.is_some() {
            parts.push("category_id = ?".to_string());
        }
        if kind.is_some() {
            parts.push("kind = ?".to_string());
        }
        if amount.is_some() {
            parts.push("amount = ?".to_string());
        }
        if description.is_some() {
            parts.push("description = ?".to_string());
        }
        if entry_date.is_some() {
            parts.push("entry_date = ?".to_string());
        }
        if tags.is_some() {
            parts.push("tags = ?".to_string());
        }
        if notes.is_some() {
            parts.push("notes = ?".to_string());
        }
        let sql = format!(
            "UPDATE budget_entries SET {} WHERE id = ? AND deleted_at IS NULL",
            parts.join(", ")
        );
        let mut q = sqlx::query(&sql).bind(now);
        if let Some(v) = category_id {
            q = q.bind(v);
        }
        if let Some(v) = kind {
            q = q.bind(kind_to_db(v));
        }
        if let Some(v) = amount {
            q = q.bind(v);
        }
        if let Some(v) = description {
            q = q.bind(v);
        }
        if let Some(v) = entry_date {
            q = q.bind(v);
        }
        if let Some(v) = tags {
            q = q.bind(v.join(","));
        }
        if let Some(v) = notes {
            q = q.bind(v);
        }
        q.bind(entry_id).execute(&self.pool).await?;
        self.get_entry(entry_id).await
    }

    pub async fn delete_entry(&self, entry_id: &str) -> Result<()> {
        let now = now_unix();
        sqlx::query("UPDATE budget_entries SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(now)
            .bind(entry_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_entry_budget_id(&self, entry_id: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT budget_id FROM budget_entries WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(entry_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(v,)| v))
    }

    // -----------------------------------------------------------------------
    // Comments
    // -----------------------------------------------------------------------

    pub async fn add_comment(
        &self,
        entry_id: &str,
        body: &str,
        created_by: &str,
    ) -> Result<DbComment> {
        let id = new_id();
        let now = now_unix();
        sqlx::query("INSERT INTO entry_comments (id, entry_id, body, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(entry_id).bind(body).bind(created_by).bind(now).bind(now)
            .execute(&self.pool).await?;
        self.get_comment(&id).await
    }

    pub async fn get_comment(&self, comment_id: &str) -> Result<DbComment> {
        let row = sqlx::query_as::<_, DbComment>(
            "SELECT id, entry_id, body, created_by, created_at, updated_at FROM entry_comments WHERE id = ? AND deleted_at IS NULL"
        ).bind(comment_id).fetch_one(&self.pool).await?;
        Ok(row)
    }

    pub async fn edit_comment(&self, comment_id: &str, body: &str) -> Result<DbComment> {
        let now = now_unix();
        sqlx::query("UPDATE entry_comments SET body = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(body).bind(now).bind(comment_id)
            .execute(&self.pool).await?;
        self.get_comment(comment_id).await
    }

    pub async fn delete_comment(&self, comment_id: &str) -> Result<()> {
        let now = now_unix();
        sqlx::query("UPDATE entry_comments SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(now)
            .bind(comment_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_comments(&self, entry_id: &str) -> Result<Vec<DbComment>> {
        let rows = sqlx::query_as::<_, DbComment>(
            "SELECT id, entry_id, body, created_by, created_at, updated_at FROM entry_comments WHERE entry_id = ? AND deleted_at IS NULL ORDER BY created_at ASC"
        ).bind(entry_id).fetch_all(&self.pool).await?;
        Ok(rows)
    }

    pub async fn get_comment_entry_id(&self, comment_id: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT entry_id FROM entry_comments WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(comment_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(v,)| v))
    }

    // -----------------------------------------------------------------------
    // Attachments
    // -----------------------------------------------------------------------

    pub async fn attach_file(
        &self,
        entry_id: &str,
        file_id: &str,
        file_name: &str,
        created_by: &str,
    ) -> Result<DbAttachment> {
        let id = new_id();
        let now = now_unix();
        sqlx::query("INSERT INTO entry_attachments (id, entry_id, file_id, file_name, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(entry_id).bind(file_id).bind(file_name).bind(created_by).bind(now)
            .execute(&self.pool).await?;
        let row = sqlx::query_as::<_, DbAttachment>(
            "SELECT id, entry_id, file_id, file_name, created_by, created_at FROM entry_attachments WHERE id = ?"
        ).bind(&id).fetch_one(&self.pool).await?;
        Ok(row)
    }

    pub async fn remove_attachment(&self, attachment_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM entry_attachments WHERE id = ?")
            .bind(attachment_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_attachments(&self, entry_id: &str) -> Result<Vec<DbAttachment>> {
        let rows = sqlx::query_as::<_, DbAttachment>(
            "SELECT id, entry_id, file_id, file_name, created_by, created_at FROM entry_attachments WHERE entry_id = ? ORDER BY created_at ASC"
        ).bind(entry_id).fetch_all(&self.pool).await?;
        Ok(rows)
    }

    pub async fn get_attachment_entry_id(&self, attachment_id: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT entry_id FROM entry_attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(v,)| v))
    }

    // -----------------------------------------------------------------------
    // Recurring entries
    // -----------------------------------------------------------------------

    pub async fn update_recurrence_rule(
        &self,
        entry_id: &str,
        rule: Option<&str>,
    ) -> Result<DbEntry> {
        let now = now_unix();
        let is_recurring = rule.is_some();
        sqlx::query(
            "UPDATE budget_entries SET recurrence_rule = ?, is_recurring = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL"
        )
        .bind(rule).bind(is_recurring).bind(now).bind(entry_id)
        .execute(&self.pool).await?;
        self.get_entry(entry_id).await
    }

    /// Returns all active recurring entries whose next_occurrence <= today
    pub async fn list_due_recurring(&self, today: &str) -> Result<Vec<DbEntry>> {
        let rows = sqlx::query_as::<_, DbEntry>(
            "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount, e.description,
                    DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date,
                    e.tags, e.notes, e.is_recurring, e.recurrence_rule,
                    DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence,
                    e.split_group_id, e.split_total,
                    e.created_by, e.created_at, e.updated_at, e.deleted_at,
                    0 AS has_attachment
             FROM budget_entries e
             WHERE e.is_recurring = TRUE
               AND e.recurrence_rule IS NOT NULL
               AND e.next_occurrence <= ?
               AND e.deleted_at IS NULL",
        )
        .bind(today)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Advance next_occurrence after spawning a new entry
    pub async fn advance_next_occurrence(&self, entry_id: &str, next_date: &str) -> Result<()> {
        let now = now_unix();
        sqlx::query("UPDATE budget_entries SET next_occurrence = ?, updated_at = ? WHERE id = ?")
            .bind(next_date)
            .bind(now)
            .bind(entry_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Split entries
    // -----------------------------------------------------------------------

    pub async fn list_split_legs(&self, split_group_id: &str) -> Result<Vec<DbEntry>> {
        let rows = sqlx::query_as::<_, DbEntry>(
            "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount, e.description,
                    DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date,
                    e.tags, e.notes, e.is_recurring, e.recurrence_rule,
                    DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence,
                    e.split_group_id, e.split_total,
                    e.created_by, e.created_at, e.updated_at, e.deleted_at,
                    (SELECT COUNT(*) > 0 FROM entry_attachments a WHERE a.entry_id = e.id) AS has_attachment
             FROM budget_entries e
             WHERE e.split_group_id = ? AND e.deleted_at IS NULL
             ORDER BY e.created_at ASC"
        )
        .bind(split_group_id)
        .fetch_all(&self.pool).await?;
        Ok(rows)
    }
}
