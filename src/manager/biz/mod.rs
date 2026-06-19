#![allow(clippy::result_large_err)]
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;

use crate::converters::{map_attachment, map_comment, map_entry};
use crate::manager::client::BudgetClient;
use crate::manager::repository::EntryRepository;
use crate::pb::service::budget::BudgetRole;
use crate::pb::service::entry::{
    Attachment, BulkImportEntriesResponse, BulkImportRow, BulkImportRowResult, Comment,
    CreateSplitEntryResponse, Entry, EntryKind, ListParams, PageMeta,
};

pub struct EntryBiz {
    pub repo: Arc<EntryRepository>,
    pub budget_client: Arc<Mutex<BudgetClient>>,
}

impl EntryBiz {
    pub fn new(repo: EntryRepository, budget_client: BudgetClient) -> Self {
        Self {
            repo: Arc::new(repo),
            budget_client: Arc::new(Mutex::new(budget_client)),
        }
    }

    pub fn new_with_arc(repo: Arc<EntryRepository>, budget_client: BudgetClient) -> Self {
        Self {
            repo,
            budget_client: Arc::new(Mutex::new(budget_client)),
        }
    }

    fn internal(e: impl ToString) -> Status {
        Status::internal(e.to_string())
    }

    async fn check_role(&self, user_id: &str, budget_id: &str, user_type: Option<&str>) -> Result<BudgetRole, Status> {
        self.budget_client
            .lock()
            .await
            .check_role(user_id, budget_id, user_type)
            .await
    }

    async fn assert_member(&self, budget_id: &str, user_id: &str, user_type: Option<&str>) -> Result<(), Status> {
        if self.check_role(user_id, budget_id, user_type).await? == BudgetRole::Unspecified {
            return Err(Status::permission_denied("Not a member of this budget"));
        }
        Ok(())
    }

    async fn assert_contributor(&self, budget_id: &str, user_id: &str, user_type: Option<&str>) -> Result<(), Status> {
        let role = self.check_role(user_id, budget_id, user_type).await?;
        if !matches!(
            role,
            BudgetRole::Owner | BudgetRole::Manager | BudgetRole::Contributor
        ) {
            return Err(Status::permission_denied(
                "Requires Contributor role or higher",
            ));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Entry CRUD
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn create_entry(
        &self,
        user_id: &str,
        budget_id: &str,
        category_id: Option<&str>,
        kind: EntryKind,
        amount: i64,
        description: &str,
        entry_date: &str,
        tags: &[String],
        notes: Option<&str>,
        user_type: Option<&str>,
    ) -> Result<Entry, Status> {
        self.assert_contributor(budget_id, user_id, user_type).await?;
        let db = self
            .repo
            .create_entry(
                budget_id,
                category_id,
                kind,
                amount,
                description,
                entry_date,
                tags,
                notes,
                user_id,
            )
            .await
            .map_err(Self::internal)?;
        Ok(map_entry(db))
    }

    // -----------------------------------------------------------------------
    // Recurring entries
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn create_recurring_entry(
        &self,
        user_id: &str,
        budget_id: &str,
        category_id: Option<&str>,
        kind: EntryKind,
        amount: i64,
        description: &str,
        entry_date: &str,
        tags: &[String],
        notes: Option<&str>,
        recurrence_rule: &str,
    ) -> Result<Entry, Status> {
        if recurrence_rule.is_empty() {
            return Err(Status::invalid_argument("recurrence_rule is required"));
        }
        self.assert_contributor(budget_id, user_id, None).await?;
        let db = self
            .repo
            .create_entry_full(
                budget_id,
                category_id,
                kind,
                amount,
                description,
                entry_date,
                tags,
                notes,
                user_id,
                Some(recurrence_rule),
                None,
                None,
            )
            .await
            .map_err(Self::internal)?;
        Ok(map_entry(db))
    }

    pub async fn update_recurrence_rule(
        &self,
        user_id: &str,
        entry_id: &str,
        rule: Option<&str>,
    ) -> Result<Entry, Status> {
        let budget_id = self
            .repo
            .get_entry_budget_id(entry_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Entry not found"))?;
        self.assert_contributor(&budget_id, user_id, None).await?;
        let db = self
            .repo
            .update_recurrence_rule(entry_id, rule)
            .await
            .map_err(Self::internal)?;
        Ok(map_entry(db))
    }

    pub async fn cancel_recurrence(&self, user_id: &str, entry_id: &str) -> Result<Entry, Status> {
        self.update_recurrence_rule(user_id, entry_id, None).await
    }

    // -----------------------------------------------------------------------
    // Split entries
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn create_split_entry(
        &self,
        user_id: &str,
        budget_id: &str,
        kind: EntryKind,
        total_amount: i64,
        description: &str,
        entry_date: &str,
        tags: &[String],
        notes: Option<&str>,
        legs: Vec<crate::pb::service::entry::SplitLeg>,
    ) -> Result<CreateSplitEntryResponse, Status> {
        if legs.len() < 2 {
            return Err(Status::invalid_argument("Split requires at least 2 legs"));
        }
        let legs_sum: i64 = legs.iter().map(|l| l.amount).sum();
        if legs_sum != total_amount {
            return Err(Status::invalid_argument(format!(
                "Legs sum ({legs_sum}) must equal total_amount ({total_amount})"
            )));
        }
        self.assert_contributor(budget_id, user_id, None).await?;

        let split_group_id = uuid::Uuid::new_v4().to_string();
        let mut created_legs = Vec::with_capacity(legs.len());

        for leg in legs {
            let leg_budget = if leg.budget_id.is_empty() {
                budget_id.to_string()
            } else {
                leg.budget_id.clone()
            };
            let cat_id = if leg.category_id.is_empty() {
                None
            } else {
                Some(leg.category_id.clone())
            };
            let leg_desc = if leg.description.is_empty() {
                description.to_string()
            } else {
                leg.description.clone()
            };
            let db = self
                .repo
                .create_entry_full(
                    &leg_budget,
                    cat_id.as_deref(),
                    kind,
                    leg.amount,
                    &leg_desc,
                    entry_date,
                    tags,
                    notes,
                    user_id,
                    None,
                    Some(&split_group_id),
                    Some(total_amount),
                )
                .await
                .map_err(Self::internal)?;
            created_legs.push(map_entry(db));
        }

        Ok(CreateSplitEntryResponse {
            split_group_id,
            legs: created_legs,
        })
    }

    pub async fn list_split_legs(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<(Vec<Entry>, String), Status> {
        let db_entry = self
            .repo
            .get_entry(entry_id)
            .await
            .map_err(|_| Status::not_found("Entry not found"))?;
        let split_group_id = db_entry
            .split_group_id
            .clone()
            .ok_or_else(|| Status::invalid_argument("Entry is not part of a split"))?;
        self.assert_member(&db_entry.budget_id, user_id, None).await?;
        let legs = self
            .repo
            .list_split_legs(&split_group_id)
            .await
            .map_err(Self::internal)?;
        Ok((legs.into_iter().map(map_entry).collect(), split_group_id))
    }

    pub async fn get_entry(&self, user_id: &str, entry_id: &str) -> Result<Entry, Status> {
        let db = self
            .repo
            .get_entry(entry_id)
            .await
            .map_err(|_| Status::not_found("Entry not found"))?;
        self.assert_member(&db.budget_id, user_id, None).await?;
        Ok(map_entry(db))
    }

    pub async fn list_entries(
        &self,
        user_id: &str,
        budget_id: Option<&str>,
        budget_ids: &[String],
        params: &ListParams,
        user_type: Option<&str>,
    ) -> Result<(Vec<Entry>, PageMeta), Status> {
        if let Some(bid) = budget_id {
            self.assert_member(bid, user_id, user_type).await?;
            let result = self
                .repo
                .list_entries(Some(bid), &[], None, params)
                .await
                .map_err(Self::internal)?;
            Ok((
                result.entries.into_iter().map(map_entry).collect(),
                result.meta,
            ))
        } else {
            let result = self
                .repo
                .list_entries(None, budget_ids, Some(user_id), params)
                .await
                .map_err(Self::internal)?;
            Ok((
                result.entries.into_iter().map(map_entry).collect(),
                result.meta,
            ))
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_entry(
        &self,
        user_id: &str,
        entry_id: &str,
        category_id: Option<&str>,
        kind: Option<EntryKind>,
        amount: Option<i64>,
        description: Option<&str>,
        entry_date: Option<&str>,
        tags: Option<&[String]>,
        notes: Option<&str>,
    ) -> Result<Entry, Status> {
        let budget_id = self
            .repo
            .get_entry_budget_id(entry_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Entry not found"))?;
        self.assert_contributor(&budget_id, user_id, None).await?;
        let db = self
            .repo
            .update_entry(
                entry_id,
                category_id,
                kind,
                amount,
                description,
                entry_date,
                tags,
                notes,
            )
            .await
            .map_err(Self::internal)?;
        Ok(map_entry(db))
    }

    pub async fn delete_entry(&self, user_id: &str, entry_id: &str) -> Result<(), Status> {
        let budget_id = self
            .repo
            .get_entry_budget_id(entry_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Entry not found"))?;
        self.assert_contributor(&budget_id, user_id, None).await?;
        self.repo
            .delete_entry(entry_id)
            .await
            .map_err(Self::internal)
    }

    // -----------------------------------------------------------------------
    // Bulk import
    // -----------------------------------------------------------------------

    pub async fn bulk_import(
        &self,
        user_id: &str,
        budget_id: &str,
        rows: Vec<BulkImportRow>,
    ) -> Result<BulkImportEntriesResponse, Status> {
        self.assert_contributor(budget_id, user_id, None).await?;
        let mut results = Vec::with_capacity(rows.len());
        let mut imported = 0i32;
        let mut errors = 0i32;

        for (i, row) in rows.into_iter().enumerate() {
            let kind = EntryKind::try_from(row.kind).unwrap_or(EntryKind::Expense);
            if row.amount <= 0 || row.entry_date.is_empty() {
                results.push(BulkImportRowResult {
                    row_index: i as i32,
                    success: false,
                    error: "amount must be positive and entry_date required".to_string(),
                    entry_id: String::new(),
                });
                errors += 1;
                continue;
            }
            let cat_id = if row.category_id.is_empty() {
                None
            } else {
                Some(row.category_id.as_str())
            };
            let notes = if row.notes.is_empty() {
                None
            } else {
                Some(row.notes.as_str())
            };
            match self
                .repo
                .create_entry(
                    budget_id,
                    cat_id,
                    kind,
                    row.amount,
                    &row.description,
                    &row.entry_date,
                    &row.tags,
                    notes,
                    user_id,
                )
                .await
            {
                Ok(db) => {
                    results.push(BulkImportRowResult {
                        row_index: i as i32,
                        success: true,
                        error: String::new(),
                        entry_id: db.id,
                    });
                    imported += 1;
                }
                Err(e) => {
                    results.push(BulkImportRowResult {
                        row_index: i as i32,
                        success: false,
                        error: e.to_string(),
                        entry_id: String::new(),
                    });
                    errors += 1;
                }
            }
        }
        Ok(BulkImportEntriesResponse {
            imported_count: imported,
            error_count: errors,
            results,
        })
    }

    // -----------------------------------------------------------------------
    // Comments
    // -----------------------------------------------------------------------

    pub async fn add_comment(
        &self,
        user_id: &str,
        entry_id: &str,
        body: &str,
    ) -> Result<Comment, Status> {
        let budget_id = self
            .repo
            .get_entry_budget_id(entry_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Entry not found"))?;
        self.assert_member(&budget_id, user_id, None).await?;
        let db = self
            .repo
            .add_comment(entry_id, body, user_id)
            .await
            .map_err(Self::internal)?;
        Ok(map_comment(db))
    }

    pub async fn edit_comment(
        &self,
        user_id: &str,
        comment_id: &str,
        body: &str,
    ) -> Result<Comment, Status> {
        let db = self
            .repo
            .get_comment(comment_id)
            .await
            .map_err(|_| Status::not_found("Comment not found"))?;
        if db.user_id != user_id {
            return Err(Status::permission_denied("Can only edit your own comments"));
        }
        let updated = self
            .repo
            .edit_comment(comment_id, body)
            .await
            .map_err(Self::internal)?;
        Ok(map_comment(updated))
    }

    pub async fn delete_comment(&self, user_id: &str, comment_id: &str) -> Result<(), Status> {
        let db = self
            .repo
            .get_comment(comment_id)
            .await
            .map_err(|_| Status::not_found("Comment not found"))?;
        if db.user_id != user_id {
            return Err(Status::permission_denied(
                "Can only delete your own comments",
            ));
        }
        self.repo
            .delete_comment(comment_id)
            .await
            .map_err(Self::internal)
    }

    pub async fn list_comments(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<Vec<Comment>, Status> {
        let budget_id = self
            .repo
            .get_entry_budget_id(entry_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Entry not found"))?;
        self.assert_member(&budget_id, user_id, None).await?;
        let rows = self
            .repo
            .list_comments(entry_id)
            .await
            .map_err(Self::internal)?;
        Ok(rows.into_iter().map(map_comment).collect())
    }

    // -----------------------------------------------------------------------
    // Attachments
    // -----------------------------------------------------------------------

    pub async fn attach_file(
        &self,
        user_id: &str,
        entry_id: &str,
        file_id: &str,
        file_name: &str,
    ) -> Result<Attachment, Status> {
        let budget_id = self
            .repo
            .get_entry_budget_id(entry_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Entry not found"))?;
        self.assert_contributor(&budget_id, user_id, None).await?;
        let db = self
            .repo
            .attach_file(entry_id, file_id, file_name, user_id)
            .await
            .map_err(Self::internal)?;
        Ok(map_attachment(db))
    }

    pub async fn remove_attachment(
        &self,
        user_id: &str,
        attachment_id: &str,
    ) -> Result<(), Status> {
        let entry_id = self
            .repo
            .get_attachment_entry_id(attachment_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Attachment not found"))?;
        let budget_id = self
            .repo
            .get_entry_budget_id(&entry_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Entry not found"))?;
        self.assert_contributor(&budget_id, user_id, None).await?;
        self.repo
            .remove_attachment(attachment_id)
            .await
            .map_err(Self::internal)
    }

    pub async fn list_attachments(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<Vec<Attachment>, Status> {
        let budget_id = self
            .repo
            .get_entry_budget_id(entry_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Entry not found"))?;
        self.assert_member(&budget_id, user_id, None).await?;
        let rows = self
            .repo
            .list_attachments(entry_id)
            .await
            .map_err(Self::internal)?;
        Ok(rows.into_iter().map(map_attachment).collect())
    }
}
