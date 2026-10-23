//! Transfer persistence (plan R1): a transfer is two linked rows sharing
//! `transfer_group_id`, written in one transaction.

use anyhow::Result;
use chrono::Utc;

use super::{new_id, EntryRepository};
use crate::converters::{kind_to_db, DbEntry};
use crate::pb::service::entry::EntryKind;

pub struct NewLeg<'a> {
    pub budget_id: &'a str,
    pub kind: EntryKind,
    pub amount: i64,
    pub entry_date: &'a str,
    pub description: &'a str,
    pub notes: Option<&'a str>,
    pub group_id: &'a str,
    pub counterparty_budget_id: &'a str,
    pub counterparty_kind: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub created_by: &'a str,
}

const SELECT_LEG: &str = "SELECT e.id, e.budget_id, e.category_id, e.kind, e.amount_minor, e.description,
        DATE_FORMAT(e.entry_date, '%Y-%m-%d') AS entry_date,
        e.tags, e.notes, e.is_recurring, e.recurrence_rule,
        DATE_FORMAT(e.next_occurrence, '%Y-%m-%d') AS next_occurrence,
        e.split_group_id, e.split_total, e.transfer_group_id, e.counterparty_budget_id, e.counterparty_kind,
        e.created_by, e.created_at, e.updated_at, e.deleted_at,
        (SELECT COUNT(*) > 0 FROM entry_attachments a WHERE a.entry_id = e.id) AS has_attachment
     FROM entries e";

async fn insert_leg(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    leg: &NewLeg<'_>,
) -> Result<String> {
    let id = new_id();
    let now = Utc::now().naive_utc();
    sqlx::query(
        "INSERT INTO entries (id, budget_id, category_id, kind, amount_minor, description, entry_date, notes,
                              transfer_group_id, counterparty_budget_id, counterparty_kind, idempotency_key,
                              created_by, created_at, updated_at)
         VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(leg.budget_id)
    .bind(kind_to_db(leg.kind))
    .bind(leg.amount)
    .bind(leg.description)
    .bind(leg.entry_date)
    .bind(leg.notes)
    .bind(leg.group_id)
    .bind(leg.counterparty_budget_id)
    .bind(leg.counterparty_kind)
    .bind(leg.idempotency_key)
    .bind(leg.created_by)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

impl EntryRepository {
    /// Insert both legs atomically; returns `(out, in)` rows.
    pub async fn create_transfer_pair(
        &self,
        out_leg: &NewLeg<'_>,
        in_leg: &NewLeg<'_>,
    ) -> Result<(DbEntry, DbEntry)> {
        let mut tx = self.pool.begin().await?;
        let out_id = insert_leg(&mut tx, out_leg).await?;
        let in_id = insert_leg(&mut tx, in_leg).await?;
        tx.commit().await?;
        Ok((
            self.get_entry(&out_id).await?,
            self.get_entry(&in_id).await?,
        ))
    }

    /// Single leg (budget-service saga). Caller handles idempotent replays.
    pub async fn create_transfer_leg(&self, leg: &NewLeg<'_>) -> Result<DbEntry> {
        let mut tx = self.pool.begin().await?;
        let id = insert_leg(&mut tx, leg).await?;
        tx.commit().await?;
        self.get_entry(&id).await
    }

    /// Finds a leg by idempotency key, **including soft-deleted rows**, because the
    /// unique index also covers them.
    pub async fn find_leg_by_key(&self, budget_id: &str, key: &str) -> Result<Option<DbEntry>> {
        let sql = format!("{SELECT_LEG} WHERE e.budget_id = ? AND e.idempotency_key = ?");
        Ok(sqlx::query_as::<_, DbEntry>(&sql)
            .bind(budget_id)
            .bind(key)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn get_transfer_legs(&self, group_id: &str) -> Result<Vec<DbEntry>> {
        let sql = format!(
            "{SELECT_LEG} WHERE e.transfer_group_id = ? AND e.deleted_at IS NULL ORDER BY e.kind DESC"
        );
        Ok(sqlx::query_as::<_, DbEntry>(&sql)
            .bind(group_id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn update_transfer(
        &self,
        group_id: &str,
        amount: Option<i64>,
        entry_date: Option<&str>,
        description: Option<&str>,
        notes: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE entries SET amount_minor = COALESCE(?, amount_minor),
                    entry_date = COALESCE(?, entry_date),
                    description = COALESCE(?, description),
                    notes = COALESCE(?, notes),
                    updated_at = ?
             WHERE transfer_group_id = ? AND deleted_at IS NULL",
        )
        .bind(amount)
        .bind(entry_date)
        .bind(description)
        .bind(notes)
        .bind(Utc::now().naive_utc())
        .bind(group_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_transfer(&self, group_id: &str) -> Result<()> {
        let now = Utc::now().naive_utc();
        sqlx::query(
            "UPDATE entries SET deleted_at = ?, updated_at = ?
             WHERE transfer_group_id = ? AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(group_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Soft-delete one leg by idempotency key; no-op if already voided.
    pub async fn void_transfer_leg(&self, budget_id: &str, key: &str) -> Result<()> {
        let now = Utc::now().naive_utc();
        sqlx::query(
            "UPDATE entries SET deleted_at = ?, updated_at = ?
             WHERE budget_id = ? AND idempotency_key = ? AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(budget_id)
        .bind(key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
