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

/// Parts of one debt payment (plan DB6): the principal moves to the debt budget as a transfer,
/// interest and fees are an expense of the paying budget.
pub struct NewDebtLegs<'a> {
    pub group_id: &'a str,
    pub from_budget_id: &'a str,
    pub debt_budget_id: &'a str,
    pub principal_minor: i64,
    pub cost_minor: i64,
    pub category_id: &'a str,
    pub entry_date: &'a str,
    pub description: &'a str,
    pub created_by: &'a str,
}

/// Idempotency keys of the two legs; derived from the group id so a replay finds them again.
pub fn debt_principal_key(group_id: &str) -> String {
    format!("{group_id}:p")
}
pub fn debt_cost_key(group_id: &str) -> String {
    format!("{group_id}:c")
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
    /// Writes the Standard-side legs of a debt payment in ONE transaction. Returns
    /// `(principal_entry_id, cost_entry_id, replayed)`; empty ids for a skipped leg.
    /// Idempotent on the group id: legs that already exist are returned, a voided group is refused.
    pub async fn create_debt_legs(&self, l: &NewDebtLegs<'_>) -> Result<(String, String, bool)> {
        let pkey = debt_principal_key(l.group_id);
        let ckey = debt_cost_key(l.group_id);
        let mut tx = self.pool.begin().await?;
        // Serialise concurrent replays of the same payment.
        let existing: Vec<(String, Option<String>, Option<chrono::NaiveDateTime>)> =
            sqlx::query_as(
                "SELECT id, idempotency_key, deleted_at FROM entries
             WHERE budget_id = ? AND idempotency_key IN (?, ?) FOR UPDATE",
            )
            .bind(l.from_budget_id)
            .bind(&pkey)
            .bind(&ckey)
            .fetch_all(&mut *tx)
            .await?;
        if !existing.is_empty() {
            tx.rollback().await?;
            if existing.iter().any(|(_, _, d)| d.is_some()) {
                anyhow::bail!("debt payment legs were already voided");
            }
            let find = |k: &str| {
                existing
                    .iter()
                    .find(|(_, key, _)| key.as_deref() == Some(k))
                    .map(|(id, _, _)| id.clone())
                    .unwrap_or_default()
            };
            return Ok((find(&pkey), find(&ckey), true));
        }
        let mut principal_id = String::new();
        let mut cost_id = String::new();
        // The two legs show up as separate rows in the paying budget: tell them apart.
        let principal_desc = format!("{} — gốc", l.description);
        let cost_desc = format!("{} — lãi & phí", l.description);
        if l.principal_minor > 0 {
            principal_id = insert_leg(
                &mut tx,
                &NewLeg {
                    budget_id: l.from_budget_id,
                    kind: EntryKind::TransferOut,
                    amount: l.principal_minor,
                    entry_date: l.entry_date,
                    description: &principal_desc,
                    notes: None,
                    group_id: l.group_id,
                    counterparty_budget_id: l.debt_budget_id,
                    counterparty_kind: "debt",
                    idempotency_key: Some(&pkey),
                    created_by: l.created_by,
                },
            )
            .await?;
        }
        if l.cost_minor > 0 {
            let id = new_id();
            let now = Utc::now().naive_utc();
            sqlx::query(
                "INSERT INTO entries (id, budget_id, category_id, kind, amount_minor, description, entry_date,
                                      transfer_group_id, counterparty_budget_id, counterparty_kind, idempotency_key,
                                      created_by, created_at, updated_at)
                 VALUES (?, ?, ?, 'expense', ?, ?, ?, ?, ?, 'debt', ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(l.from_budget_id)
            .bind(l.category_id)
            .bind(l.cost_minor)
            .bind(&cost_desc)
            .bind(l.entry_date)
            .bind(l.group_id)
            .bind(l.debt_budget_id)
            .bind(&ckey)
            .bind(l.created_by)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            cost_id = id;
        }
        tx.commit().await?;
        Ok((principal_id, cost_id, false))
    }

    /// Compensation: soft-delete every live leg of the group in that budget. Returns the count.
    pub async fn void_debt_legs(&self, group_id: &str, from_budget_id: &str) -> Result<u64> {
        let now = Utc::now().naive_utc();
        let r = sqlx::query(
            "UPDATE entries SET deleted_at = ?, updated_at = ?
             WHERE budget_id = ? AND transfer_group_id = ? AND counterparty_kind = 'debt' AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(from_budget_id)
        .bind(group_id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

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
