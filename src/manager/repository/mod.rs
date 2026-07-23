use anyhow::Result;
use chrono::Utc;
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

/// A bind variable for dynamic query conditions — stored as String for sqlx query building.
pub type SqlBind = String;

// ---------------------------------------------------------------------------
// EntryQueryBuilder
// ---------------------------------------------------------------------------

pub struct EntryQueryBuilder<'a> {
    conditions: Vec<String>,
    binds: Vec<SqlBind>,
    params: &'a ListParams,
}

impl<'a> EntryQueryBuilder<'a> {
    pub fn new(params: &'a ListParams) -> Self {
        Self {
            conditions: vec!["e.deleted_at IS NULL".to_string()],
            binds: Vec::new(),
            params,
        }
    }

    pub fn apply_budget(
        mut self,
        bid: Option<&str>,
        bids: &[String],
        creator: Option<&str>,
    ) -> Self {
        if let Some(b) = bid {
            self.conditions.push("e.budget_id = ?".to_string());
            self.binds.push(b.to_string());
        } else if !bids.is_empty() {
            let placeholders = bids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            self.conditions
                .push(format!("e.budget_id IN ({placeholders})"));
            for id in bids {
                self.binds.push(id.clone());
            }
        } else if let Some(uid) = creator {
            self.conditions.push("e.created_by = ?".to_string());
            self.binds.push(uid.to_string());
        }
        self
    }

    pub fn apply_member(mut self) -> Self {
        let member_ids = &self.params.member_ids;
        if member_ids.is_empty() {
            return self;
        }
        let placeholders = member_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        self.conditions
            .push(format!("e.created_by IN ({placeholders})"));
        self.binds.extend(member_ids.iter().cloned());
        self
    }

    pub fn apply_text(mut self) -> Self {
        if let Some(ref q) = self.params.q {
            if !q.is_empty() {
                self.conditions.push("e.description LIKE ?".to_string());
                self.binds.push(format!("%{q}%"));
            }
        }
        self
    }

    pub fn apply_kind(mut self) -> Self {
        if let Some(ref kind) = self.params.kind {
            if !kind.is_empty() {
                self.conditions.push("e.kind = ?".to_string());
                self.binds.push(kind.clone());
            }
        }
        self
    }

    pub fn apply_category(mut self) -> Self {
        // Precedence: singular category_id wins if present; else repeated category_ids.
        let has_singular = self
            .params
            .category_id
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let has_plural = !self.params.category_ids.is_empty();
        if has_singular {
            self.conditions.push("e.category_id = ?".to_string());
            self.binds.push(self.params.category_id.clone().unwrap());
        } else if has_plural {
            let placeholders = self
                .params
                .category_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            self.conditions
                .push(format!("e.category_id IN ({placeholders})"));
            for id in &self.params.category_ids {
                self.binds.push(id.clone());
            }
        }
        self
    }

    pub fn apply_date(mut self) -> Self {
        if let Some(ref df) = self.params.date_from {
            if !df.is_empty() {
                self.conditions.push("e.entry_date >= ?".to_string());
                self.binds.push(df.clone());
            }
        }
        if let Some(ref dt) = self.params.date_to {
            if !dt.is_empty() {
                self.conditions.push("e.entry_date <= ?".to_string());
                self.binds.push(dt.clone());
            }
        }
        self
    }

    pub fn apply_amount(mut self) -> Self {
        if let Some(min) = self.params.amount_min {
            self.conditions.push("e.amount_minor >= ?".to_string());
            self.binds.push(min.to_string());
        }
        if let Some(max) = self.params.amount_max {
            self.conditions.push("e.amount_minor <= ?".to_string());
            self.binds.push(max.to_string());
        }
        self
    }

    pub fn apply_tags(mut self) -> Self {
        if let Some(ref tags) = self.params.tags {
            if !tags.is_empty() {
                self.conditions.push("e.tags LIKE ?".to_string());
                self.binds.push(format!("%{tags}%"));
            }
        }
        self
    }

    pub fn build_count(&self) -> (String, Vec<SqlBind>) {
        let where_clause = format!("WHERE {}", self.conditions.join(" AND "));
        let sql = format!("SELECT COUNT(*) as cnt FROM entries e {where_clause}");
        (sql, self.binds.clone())
    }

    pub fn build_data(
        &self,
        sort_col: &str,
        sort_dir: &str,
        limit: i64,
        offset: i64,
    ) -> (String, Vec<SqlBind>) {
        let where_clause = format!("WHERE {}", self.conditions.join(" AND "));
        let sql = format!(
            "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount_minor, e.description, \
                    DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date, \
                    e.tags, e.notes, e.is_recurring, e.recurrence_rule, \
                    DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence, \
                    e.split_group_id, e.split_total, \
                    e.created_by, e.created_at, e.updated_at, e.deleted_at, \
                    (SELECT COUNT(*) > 0 FROM entry_attachments a WHERE a.entry_id = e.id) AS has_attachment \
             FROM entries e {where_clause} \
             ORDER BY {sort_col} {sort_dir} \
             LIMIT ? OFFSET ?"
        );
        let mut binds = self.binds.clone();
        binds.push(limit.to_string());
        binds.push(offset.to_string());
        (sql, binds)
    }
}

impl EntryRepository {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = sqlx::MySqlPool::connect(database_url).await?;
        let mut migrator =
            sqlx::migrate::Migrator::new(std::path::Path::new("./migrations")).await?;
        migrator.set_ignore_missing(true);
        if let Err(e) = migrator.run(&pool).await {
            let err_str = format!("{}", e);
            if err_str.contains("partially applied") {
                tracing::warn!("Partial migration detected: {}", e);
                if err_str.contains("20260507090228") {
                    let has_avatar: bool = sqlx::query_scalar(
                        "SELECT COUNT(*) > 0 FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'users' AND COLUMN_NAME = 'avatar'"
                    )
                    .fetch_one(&pool)
                    .await.unwrap_or(false);
                    if has_avatar {
                        sqlx::query(
                            "INSERT IGNORE INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time) VALUES (20260507090228, 'add_avatar_to_users', NOW(), true, 0x00, 0)"
                        )
                        .execute(&pool)
                        .await.ok();
                    }
                }
                sqlx::query("DELETE FROM _sqlx_migrations WHERE success = false")
                    .execute(&pool)
                    .await
                    .ok();
            } else {
                return Err(anyhow::anyhow!("{}", e));
            }
        }
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
        let now = Utc::now().naive_utc();
        let tags_str = tags.join(",");
        let is_recurring = recurrence_rule.is_some();
        // Compute next_occurrence from entry_date + rule (simple: same as entry_date for first insert)
        let next_occ = if is_recurring {
            Some(entry_date.to_string())
        } else {
            None
        };
        sqlx::query(
            "INSERT INTO entries (id, budget_id, category_id, kind, amount_minor, description, entry_date, tags, notes, is_recurring, recurrence_rule, next_occurrence, split_group_id, split_total, created_by, created_at, updated_at)
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
            "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount_minor, e.description,
                    DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date,
                    e.tags, e.notes, e.is_recurring, e.recurrence_rule,
                    DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence,
                    e.split_group_id, e.split_total,
                    e.created_by, e.created_at, e.updated_at, e.deleted_at,
                    (SELECT COUNT(*) > 0 FROM entry_attachments a WHERE a.entry_id = e.id) AS has_attachment
             FROM entries e
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

        let sort_col = match params.sort_by.as_deref() {
            Some("amount") => "e.amount_minor",
            Some("description") => "e.description",
            _ => "e.entry_date",
        };
        let sort_dir = if params.sort_dir.as_deref() == Some("asc") {
            "ASC"
        } else {
            "DESC"
        };

        let qb = EntryQueryBuilder::new(params)
            .apply_budget(budget_id, budget_ids, user_id)
            .apply_member()
            .apply_text()
            .apply_kind()
            .apply_category()
            .apply_date()
            .apply_amount()
            .apply_tags();

        // Count
        let (count_sql, count_binds) = qb.build_count();
        let mut count_q = sqlx::query(&count_sql);
        for b in &count_binds {
            count_q = count_q.bind(b);
        }
        let total: i64 = count_q
            .fetch_one(&self.pool)
            .await
            .map(|r| sqlx::Row::try_get::<i64, _>(&r, "cnt").unwrap_or(0))
            .unwrap_or(0);

        // Data
        let (data_sql, data_binds) = qb.build_data(sort_col, sort_dir, page_size as i64, offset);
        let mut data_q = sqlx::query_as::<_, DbEntry>(&data_sql);
        for b in &data_binds {
            data_q = data_q.bind(b);
        }
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
        let now = Utc::now().naive_utc();
        let mut parts: Vec<String> = vec!["updated_at = ?".to_string()];
        if category_id.is_some() {
            parts.push("category_id = ?".to_string());
        }
        if kind.is_some() {
            parts.push("kind = ?".to_string());
        }
        if amount.is_some() {
            parts.push("amount_minor = ?".to_string());
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
            "UPDATE entries SET {} WHERE id = ? AND deleted_at IS NULL",
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
        let now = Utc::now().naive_utc();
        sqlx::query("UPDATE entries SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(now)
            .bind(entry_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_entry_budget_id(&self, entry_id: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT budget_id FROM entries WHERE id = ? AND deleted_at IS NULL")
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
        let now = Utc::now().naive_utc();
        sqlx::query("INSERT INTO entry_comments (id, entry_id, comment_text, user_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(entry_id).bind(body).bind(created_by).bind(now).bind(now)
            .execute(&self.pool).await?;
        self.get_comment(&id).await
    }

    pub async fn get_comment(&self, comment_id: &str) -> Result<DbComment> {
        let row = sqlx::query_as::<_, DbComment>(
            "SELECT id, entry_id, comment_text, user_id, created_at, updated_at, deleted_at FROM entry_comments WHERE id = ? AND deleted_at IS NULL"
        ).bind(comment_id).fetch_one(&self.pool).await?;
        Ok(row)
    }

    pub async fn edit_comment(&self, comment_id: &str, body: &str) -> Result<DbComment> {
        let now = Utc::now().naive_utc();
        sqlx::query("UPDATE entry_comments SET comment_text = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(body).bind(now).bind(comment_id)
            .execute(&self.pool).await?;
        self.get_comment(comment_id).await
    }

    pub async fn delete_comment(&self, comment_id: &str) -> Result<()> {
        let now = Utc::now().naive_utc();
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
            "SELECT id, entry_id, comment_text, user_id, created_at, updated_at, deleted_at FROM entry_comments WHERE entry_id = ? AND deleted_at IS NULL ORDER BY created_at ASC"
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
        let now = Utc::now().naive_utc();
        sqlx::query("INSERT INTO entry_attachments (id, entry_id, file_id, file_name, user_id, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(entry_id).bind(file_id).bind(file_name).bind(created_by).bind(now)
            .execute(&self.pool).await?;
        let row = sqlx::query_as::<_, DbAttachment>(
            "SELECT id, entry_id, file_id, file_name, user_id, created_at FROM entry_attachments WHERE id = ?"
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
            "SELECT id, entry_id, file_id, file_name, user_id, created_at FROM entry_attachments WHERE entry_id = ? ORDER BY created_at ASC"
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
        let now = Utc::now().naive_utc();
        let is_recurring = rule.is_some();
        sqlx::query(
            "UPDATE entries SET recurrence_rule = ?, is_recurring = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL"
        )
        .bind(rule).bind(is_recurring).bind(now).bind(entry_id)
        .execute(&self.pool).await?;
        self.get_entry(entry_id).await
    }

    /// Returns all active recurring entries whose next_occurrence <= today
    pub async fn list_due_recurring(&self, today: &str) -> Result<Vec<DbEntry>> {
        let rows = sqlx::query_as::<_, DbEntry>(
            "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount_minor, e.description,
                    DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date,
                    e.tags, e.notes, e.is_recurring, e.recurrence_rule,
                    DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence,
                    e.split_group_id, e.split_total,
                    e.created_by, e.created_at, e.updated_at, e.deleted_at,
                    0 AS has_attachment
             FROM entries e
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
        let now = Utc::now().naive_utc();
        sqlx::query("UPDATE entries SET next_occurrence = ?, updated_at = ? WHERE id = ?")
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
            "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount_minor, e.description,
                    DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date,
                    e.tags, e.notes, e.is_recurring, e.recurrence_rule,
                    DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence,
                    e.split_group_id, e.split_total,
                    e.created_by, e.created_at, e.updated_at, e.deleted_at,
                    (SELECT COUNT(*) > 0 FROM entry_attachments a WHERE a.entry_id = e.id) AS has_attachment
             FROM entries e
             WHERE e.split_group_id = ? AND e.deleted_at IS NULL
             ORDER BY e.created_at ASC"
        )
        .bind(split_group_id)
        .fetch_all(&self.pool).await?;
        Ok(rows)
    }
}
