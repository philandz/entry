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
    pub transfer_group_id: Option<String>,
    pub counterparty_budget_id: Option<String>,
    pub counterparty_kind: Option<String>,
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
        EntryKind::TransferOut => "transfer_out",
        EntryKind::TransferIn => "transfer_in",
        EntryKind::Unspecified => "expense",
    }
}

pub fn kind_from_db(s: &str) -> EntryKind {
    match s {
        "income" => EntryKind::Income,
        "expense" => EntryKind::Expense,
        "transfer_out" => EntryKind::TransferOut,
        "transfer_in" => EntryKind::TransferIn,
        other => {
            // Legacy behaviour: unknown values were treated as expense. Keep it,
            // but make the drift visible instead of silently miscounting.
            tracing::warn!(
                kind = other,
                "unknown entries.kind in DB, treating as expense"
            );
            EntryKind::Expense
        }
    }
}

/// True for the two transfer kinds. Used to keep transfers out of
/// income/expense math and to reject them on the generic entry RPCs.
pub fn is_transfer(k: EntryKind) -> bool {
    matches!(k, EntryKind::TransferOut | EntryKind::TransferIn)
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
            deleted_at: db
                .deleted_at
                .map(|dt| dt.and_utc().timestamp())
                .unwrap_or(0),
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
        transfer_group_id: db.transfer_group_id,
        counterparty_budget_id: db.counterparty_budget_id,
        counterparty_kind: db.counterparty_kind,
    }
}

pub fn map_comment(db: DbComment) -> Comment {
    Comment {
        id: db.id.clone(),
        entry_id: db.entry_id,
        body: db.comment_text,
        created_by: db.user_id.clone(),
        created_at: db.created_at.and_utc().timestamp(),
        updated_at: db.updated_at.and_utc().timestamp(),
        base: Some(Base {
            id: db.id,
            created_at: db.created_at.and_utc().timestamp(),
            updated_at: db.updated_at.and_utc().timestamp(),
            deleted_at: db
                .deleted_at
                .map(|dt| dt.and_utc().timestamp())
                .unwrap_or(0),
            created_by: db.user_id,
            updated_by: String::new(),
            owner_id: String::new(),
            status: 0,
        }),
    }
}

pub fn map_attachment(db: DbAttachment) -> Attachment {
    Attachment {
        id: db.id.clone(),
        entry_id: db.entry_id,
        file_id: db.file_id,
        file_name: db.file_name,
        created_by: db.user_id.clone(),
        created_at: db.created_at.and_utc().timestamp(),
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
    }
}
