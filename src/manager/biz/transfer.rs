//! Transfers between budgets (plan R1). Standard↔Standard pairs are created here in one
//! transaction; fund (saving/invest) legs are driven by the budget-service saga through
//! `create_transfer_leg` / `void_transfer_leg`.

use tonic::Status;

use super::EntryBiz;
use crate::converters::{kind_from_db, map_entry};
use crate::manager::client::BudgetCtx;
use crate::manager::repository::transfers::NewLeg;
use crate::manager::validate;
use crate::pb::service::budget::BudgetRole;
use crate::pb::service::entry::{
    CreateTransferLegRequest, CreateTransferRequest, Entry, EntryKind, Transfer,
    UpdateTransferRequest, VoidTransferLegRequest,
};

fn can_write(role: BudgetRole) -> bool {
    matches!(
        role,
        BudgetRole::Owner | BudgetRole::Manager | BudgetRole::Contributor
    )
}

impl EntryBiz {
    async fn budget_ctx(
        &self,
        user_id: &str,
        budget_id: &str,
        user_type: Option<&str>,
    ) -> Result<BudgetCtx, Status> {
        let ctx = self
            .budget_client
            .lock()
            .await
            .check_role_ctx(user_id, budget_id, user_type)
            .await?;
        if !can_write(ctx.role) {
            return Err(Status::permission_denied(
                "Requires Contributor role or higher on both budgets",
            ));
        }
        Ok(ctx)
    }

    fn to_transfer(
        group_id: &str,
        legs: Vec<crate::converters::DbEntry>,
    ) -> Result<Transfer, Status> {
        let mut out = None;
        let mut inn = None;
        for l in legs {
            match kind_from_db(&l.kind) {
                EntryKind::TransferOut => out = Some(map_entry(l)),
                EntryKind::TransferIn => inn = Some(map_entry(l)),
                _ => {}
            }
        }
        Ok(Transfer {
            group_id: group_id.to_string(),
            out_entry: out,
            in_entry: inn,
        })
    }

    pub async fn create_transfer(
        &self,
        user_id: &str,
        req: &CreateTransferRequest,
        user_type: Option<&str>,
    ) -> Result<Transfer, Status> {
        validate::create_transfer(req)?;
        let src = self
            .budget_ctx(user_id, &req.source_budget_id, user_type)
            .await?;
        let dst = self
            .budget_ctx(user_id, &req.destination_budget_id, user_type)
            .await?;
        validate::transfer_compat(&src, &dst)?;

        let key = (!req.idempotency_key.is_empty()).then_some(req.idempotency_key.as_str());
        if let Some(k) = key {
            if let Some(existing) = self
                .repo
                .find_leg_by_key(&req.source_budget_id, k)
                .await
                .map_err(Self::internal)?
            {
                if existing.deleted_at.is_some() {
                    return Err(Status::failed_precondition(
                        "idempotency_key was already used by a deleted transfer",
                    ));
                }
                let gid = existing.transfer_group_id.clone().unwrap_or_default();
                let legs = self
                    .repo
                    .get_transfer_legs(&gid)
                    .await
                    .map_err(Self::internal)?;
                return Self::to_transfer(&gid, legs);
            }
        }

        let group_id = uuid::Uuid::new_v4().to_string();
        let out_desc = if req.description.trim().is_empty() {
            format!("Chuyển sang {}", dst.name)
        } else {
            req.description.clone()
        };
        let in_desc = if req.description.trim().is_empty() {
            format!("Nhận từ {}", src.name)
        } else {
            req.description.clone()
        };
        let out_leg = NewLeg {
            budget_id: &req.source_budget_id,
            kind: EntryKind::TransferOut,
            amount: req.amount,
            entry_date: &req.entry_date,
            description: &out_desc,
            notes: req.notes.as_deref(),
            group_id: &group_id,
            counterparty_budget_id: &req.destination_budget_id,
            counterparty_kind: "entry",
            idempotency_key: key,
            created_by: user_id,
        };
        let in_leg = NewLeg {
            budget_id: &req.destination_budget_id,
            kind: EntryKind::TransferIn,
            amount: req.amount,
            entry_date: &req.entry_date,
            description: &in_desc,
            notes: req.notes.as_deref(),
            group_id: &group_id,
            counterparty_budget_id: &req.source_budget_id,
            counterparty_kind: "entry",
            idempotency_key: key,
            created_by: user_id,
        };
        let (o, i) = self
            .repo
            .create_transfer_pair(&out_leg, &in_leg)
            .await
            .map_err(Self::internal)?;
        Ok(Transfer {
            group_id,
            out_entry: Some(map_entry(o)),
            in_entry: Some(map_entry(i)),
        })
    }

    /// Loads both legs of an entry↔entry transfer and checks the caller may write to both.
    async fn load_editable_transfer(
        &self,
        user_id: &str,
        group_id: &str,
        user_type: Option<&str>,
    ) -> Result<Vec<crate::converters::DbEntry>, Status> {
        let legs = self
            .repo
            .get_transfer_legs(group_id)
            .await
            .map_err(Self::internal)?;
        if legs.is_empty() {
            return Err(Status::not_found("Transfer not found"));
        }
        if legs
            .iter()
            .any(|l| l.counterparty_kind.as_deref() == Some("fund_cash"))
            || legs.len() != 2
        {
            return Err(Status::failed_precondition(
                "this transfer involves a fund; manage it from the fund",
            ));
        }
        for l in &legs {
            self.budget_ctx(user_id, &l.budget_id, user_type).await?;
        }
        Ok(legs)
    }

    pub async fn update_transfer(
        &self,
        user_id: &str,
        req: &UpdateTransferRequest,
        user_type: Option<&str>,
    ) -> Result<Transfer, Status> {
        if let Some(a) = req.amount {
            validate::positive_amount(a)?;
        }
        if let Some(d) = req.entry_date.as_deref() {
            validate::iso_date("entry_date", d)?;
        }
        validate::transfer_text(
            req.description.as_deref().unwrap_or(""),
            req.notes.as_deref(),
            "",
        )?;
        self.load_editable_transfer(user_id, &req.group_id, user_type)
            .await?;
        self.repo
            .update_transfer(
                &req.group_id,
                req.amount,
                req.entry_date.as_deref(),
                req.description.as_deref(),
                req.notes.as_deref(),
            )
            .await
            .map_err(Self::internal)?;
        let legs = self
            .repo
            .get_transfer_legs(&req.group_id)
            .await
            .map_err(Self::internal)?;
        Self::to_transfer(&req.group_id, legs)
    }

    pub async fn delete_transfer(
        &self,
        user_id: &str,
        group_id: &str,
        user_type: Option<&str>,
    ) -> Result<(), Status> {
        self.load_editable_transfer(user_id, group_id, user_type)
            .await?;
        self.repo
            .delete_transfer(group_id)
            .await
            .map_err(Self::internal)
    }

    /// Internal: one fund-cash leg for the budget-service saga. Idempotent on
    /// `(budget_id, idempotency_key)`.
    pub async fn create_transfer_leg(
        &self,
        req: &CreateTransferLegRequest,
        user_type: Option<&str>,
    ) -> Result<Entry, Status> {
        let kind = EntryKind::try_from(req.kind).unwrap_or(EntryKind::Unspecified);
        if !crate::converters::is_transfer(kind) {
            return Err(Status::invalid_argument("kind must be a transfer kind"));
        }
        validate::positive_amount(req.amount)?;
        validate::iso_date("entry_date", &req.entry_date)?;
        validate::transfer_text(&req.description, None, &req.idempotency_key)?;
        validate::non_empty("idempotency_key", &req.idempotency_key)?;
        validate::non_empty("actor_user_id", &req.actor_user_id)?;
        if req.counterparty_kind != "fund_cash" {
            return Err(Status::invalid_argument(
                "counterparty_kind must be fund_cash",
            ));
        }
        let ctx = self
            .budget_ctx(&req.actor_user_id, &req.budget_id, user_type)
            .await?;
        if ctx.budget_type != crate::pb::service::budget::BudgetType::Standard {
            return Err(Status::failed_precondition(
                "fund transfers need a standard budget on the entry side",
            ));
        }
        if let Some(existing) = self
            .repo
            .find_leg_by_key(&req.budget_id, &req.idempotency_key)
            .await
            .map_err(Self::internal)?
        {
            if existing.deleted_at.is_some() {
                return Err(Status::failed_precondition(
                    "idempotency_key was already voided",
                ));
            }
            return Ok(map_entry(existing));
        }
        let leg = NewLeg {
            budget_id: &req.budget_id,
            kind,
            amount: req.amount,
            entry_date: &req.entry_date,
            description: &req.description,
            notes: None,
            group_id: &req.transfer_group_id,
            counterparty_budget_id: &req.counterparty_budget_id,
            counterparty_kind: "fund_cash",
            idempotency_key: Some(&req.idempotency_key),
            created_by: &req.actor_user_id,
        };
        let db = self
            .repo
            .create_transfer_leg(&leg)
            .await
            .map_err(Self::internal)?;
        Ok(map_entry(db))
    }

    /// Internal compensation: soft-delete the leg created with this key. Idempotent.
    pub async fn void_transfer_leg(&self, req: &VoidTransferLegRequest) -> Result<Entry, Status> {
        validate::non_empty("idempotency_key", &req.idempotency_key)?;
        let leg = self
            .repo
            .find_leg_by_key(&req.budget_id, &req.idempotency_key)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("transfer leg not found"))?;
        self.repo
            .void_transfer_leg(&req.budget_id, &req.idempotency_key)
            .await
            .map_err(Self::internal)?;
        Ok(map_entry(leg))
    }
}
