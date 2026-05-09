use crate::pb::common::base::Base;
use crate::pb::service::entry::{Attachment, Comment, Entry, EntryKind};
use chrono::NaiveDateTime;

// ---------------------------------------------------------------------------
// DB row structs
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
pub struct DbEntry {
    pub id: String,
    pub budget_id: String,
    pub category_id: Option<String>,
    pub kind: String,
    pub amount_minor: i64,
    pub description: String,
    pub entry_date: String,
    pub tags: Option<String>,
    pub notes: Option<String>,
    pub is_recurring: bool,
    pub recurrence_rule: Option<String>,
    pub next_occurrence: Option<String>,
    pub split_group_id: Option<String>,
    pub split_total: Option<i64>,
    pub created_by: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    // computed
    pub has_attachment: Option<bool>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct DbComment {
    pub id: String,
    pub entry_id: String,
    pub comment_text: String,
    pub user_id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct DbAttachment {
    pub id: String,
    pub entry_id: String,
    pub file_id: String,
    pub file_name: String,
    pub user_id: String,
    pub created_at: NaiveDateTime,
}

// ---------------------------------------------------------------------------
// Enum helpers
// ---------------------------------------------------------------------------

pub fn kind_to_db(k: EntryKind) -> &'static str {
    match k {
        EntryKind::Income => "income",
        EntryKind::Expense => "expense",
        EntryKind::Unspecified => "expense",
    }
}

pub fn kind_from_db(s: &str) -> EntryKind {
    match s {
        "income" => EntryKind::Income,
        _ => EntryKind::Expense,
    }
}

// ---------------------------------------------------------------------------
// Mappers
// ---------------------------------------------------------------------------

pub fn map_entry(db: DbEntry) -> Entry {
    Entry {
        base: Some(Base {
            id: db.id,
            created_at: db.created_at.and_utc().timestamp(),
            updated_at: db.updated_at.and_utc().timestamp(),
            deleted_at: db.deleted_at.map(|dt| dt.and_utc().timestamp()).unwrap_or(0),
            created_by: db.created_by,
            updated_by: String::new(),
            owner_id: String::new(),
            status: 0,
        }),
        budget_id: db.budget_id,
        category_id: db.category_id.unwrap_or_default(),
        kind: kind_from_db(&db.kind) as i32,
        amount: db.amount_minor,
        description: db.description,
        entry_date: db.entry_date,
        tags: db
            .tags
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string())
            .collect(),
        notes: db.notes.unwrap_or_default(),
        is_recurring: db.is_recurring,
        has_attachment: db.has_attachment.unwrap_or(false),
        recurrence_rule: db.recurrence_rule,
        next_occurrence: db.next_occurrence,
        split_group_id: db.split_group_id,
        split_total: db.split_total,
    }
}

pub fn map_comment(db: DbComment) -> Comment {
    Comment {
        base: Some(Base {
            id: db.id,
            created_at: db.created_at.and_utc().timestamp(),
            updated_at: db.updated_at.and_utc().timestamp(),
            deleted_at: db.deleted_at.map(|dt| dt.and_utc().timestamp()).unwrap_or(0),
            created_by: db.user_id,
            updated_by: String::new(),
            owner_id: String::new(),
            status: 0,
        }),
        entry_id: db.entry_id,
        body: db.comment_text,
    }
}

pub fn map_attachment(db: DbAttachment) -> Attachment {
    Attachment {
        base: Some(Base {
            id: db.id,
            created_at: db.created_at.and_utc().timestamp(),
            updated_at: 0,
            deleted_at: 0,
            created_by: db.user_id,
            updated_by: String::new(),
            owner_id: String::new(),
            status: 0,
        }),
        entry_id: db.entry_id,
        file_id: db.file_id,
        file_name: db.file_name,
    }
}
