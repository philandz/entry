use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::manager::biz::EntryBiz;
use crate::manager::validate;
use crate::pb::service::entry::{
    entry_service_server::EntryService,
    AddCommentRequest,
    AttachFileRequest,
    Attachment,
    BulkImportEntriesRequest,
    BulkImportEntriesResponse,
    CancelRecurrenceRequest,
    Comment,
    CreateEntryRequest,
    // Recurring
    CreateRecurringEntryRequest,
    // Split
    CreateSplitEntryRequest,
    CreateSplitEntryResponse,
    DeleteCommentRequest,
    DeleteCommentResponse,
    DeleteEntryRequest,
    DeleteEntryResponse,
    EditCommentRequest,
    Entry,
    EntryKind,
    GetEntryRequest,
    GetEntryResponse,
    ListAttachmentsRequest,
    ListAttachmentsResponse,
    ListCommentsRequest,
    ListCommentsResponse,
    ListEntriesRequest,
    ListEntriesResponse,
    ListSplitLegsRequest,
    ListSplitLegsResponse,
    RemoveAttachmentRequest,
    RemoveAttachmentResponse,
    UpdateEntryRequest,
    UpdateRecurrenceRuleRequest,
};

pub struct EntryHandler {
    biz: Arc<EntryBiz>,
}

impl EntryHandler {
    pub fn new(biz: Arc<EntryBiz>) -> Self {
        Self { biz }
    }
}

#[tonic::async_trait]
impl EntryService for EntryHandler {
    async fn create_entry(
        &self,
        request: Request<CreateEntryRequest>,
    ) -> Result<Response<Entry>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        validate::positive_amount(req.amount)?;
        validate::non_empty("entry_date", &req.entry_date)?;
        let kind = EntryKind::try_from(req.kind).unwrap_or(EntryKind::Expense);
        let cat_id = if req.category_id.is_empty() {
            None
        } else {
            Some(req.category_id.as_str())
        };
        let notes = if req.notes.is_empty() {
            None
        } else {
            Some(req.notes.as_str())
        };
        let entry = self
            .biz
            .create_entry(
                &user_id,
                &req.budget_id,
                cat_id,
                kind,
                req.amount,
                &req.description,
                &req.entry_date,
                &req.tags,
                notes,
                user_type.as_deref(),
            )
            .await?;
        Ok(Response::new(entry))
    }

    async fn get_entry(
        &self,
        request: Request<GetEntryRequest>,
    ) -> Result<Response<GetEntryResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let entry = self.biz.get_entry(&user_id, &req.entry_id).await?;
        Ok(Response::new(GetEntryResponse { entry: Some(entry) }))
    }

    async fn list_entries(
        &self,
        request: Request<ListEntriesRequest>,
    ) -> Result<Response<ListEntriesResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        tracing::debug!("list_entries handler: user_id={}, user_type={:?}", user_id, user_type);
        let req = request.into_inner();
        let default_params = crate::pb::service::entry::ListParams::default();
        let params = req.params.as_ref().unwrap_or(&default_params);
        let budget_id = req.budget_id.as_deref().filter(|s| !s.is_empty());
        let (entries, meta) = self
            .biz
            .list_entries(&user_id, budget_id, &req.budget_ids, params, user_type.as_deref())
            .await?;
        Ok(Response::new(ListEntriesResponse {
            entries,
            meta: Some(meta),
        }))
    }

    async fn update_entry(
        &self,
        request: Request<UpdateEntryRequest>,
    ) -> Result<Response<Entry>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        if let Some(a) = req.amount {
            validate::positive_amount(a)?;
        }
        let kind = req
            .kind
            .map(|k| EntryKind::try_from(k).unwrap_or(EntryKind::Expense));
        let cat_id = req.category_id.as_deref().filter(|s| !s.is_empty());
        let desc = req.description.as_deref().filter(|s| !s.is_empty());
        let date = req.entry_date.as_deref().filter(|s| !s.is_empty());
        let notes = req.notes.as_deref();
        let tags = if req.tags.is_empty() {
            None
        } else {
            Some(req.tags.as_slice())
        };
        let entry = self
            .biz
            .update_entry(
                &user_id,
                &req.entry_id,
                cat_id,
                kind,
                req.amount,
                desc,
                date,
                tags,
                notes,
            )
            .await?;
        Ok(Response::new(entry))
    }

    async fn delete_entry(
        &self,
        request: Request<DeleteEntryRequest>,
    ) -> Result<Response<DeleteEntryResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        self.biz.delete_entry(&user_id, &req.entry_id).await?;
        Ok(Response::new(DeleteEntryResponse { success: true }))
    }

    async fn bulk_import_entries(
        &self,
        request: Request<BulkImportEntriesRequest>,
    ) -> Result<Response<BulkImportEntriesResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let resp = self
            .biz
            .bulk_import(&user_id, &req.budget_id, req.rows)
            .await?;
        Ok(Response::new(resp))
    }

    async fn add_comment(
        &self,
        request: Request<AddCommentRequest>,
    ) -> Result<Response<Comment>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        validate::non_empty("body", &req.body)?;
        let comment = self
            .biz
            .add_comment(&user_id, &req.entry_id, &req.body)
            .await?;
        Ok(Response::new(comment))
    }

    async fn edit_comment(
        &self,
        request: Request<EditCommentRequest>,
    ) -> Result<Response<Comment>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        validate::non_empty("body", &req.body)?;
        let comment = self
            .biz
            .edit_comment(&user_id, &req.comment_id, &req.body)
            .await?;
        Ok(Response::new(comment))
    }

    async fn delete_comment(
        &self,
        request: Request<DeleteCommentRequest>,
    ) -> Result<Response<DeleteCommentResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        self.biz.delete_comment(&user_id, &req.comment_id).await?;
        Ok(Response::new(DeleteCommentResponse { success: true }))
    }

    async fn list_comments(
        &self,
        request: Request<ListCommentsRequest>,
    ) -> Result<Response<ListCommentsResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let comments = self.biz.list_comments(&user_id, &req.entry_id).await?;
        Ok(Response::new(ListCommentsResponse { comments }))
    }

    async fn attach_file(
        &self,
        request: Request<AttachFileRequest>,
    ) -> Result<Response<Attachment>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let att = self
            .biz
            .attach_file(&user_id, &req.entry_id, &req.file_id, &req.file_name)
            .await?;
        Ok(Response::new(att))
    }

    async fn remove_attachment(
        &self,
        request: Request<RemoveAttachmentRequest>,
    ) -> Result<Response<RemoveAttachmentResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        self.biz
            .remove_attachment(&user_id, &req.attachment_id)
            .await?;
        Ok(Response::new(RemoveAttachmentResponse { success: true }))
    }

    async fn list_attachments(
        &self,
        request: Request<ListAttachmentsRequest>,
    ) -> Result<Response<ListAttachmentsResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let attachments = self.biz.list_attachments(&user_id, &req.entry_id).await?;
        Ok(Response::new(ListAttachmentsResponse { attachments }))
    }

    // -----------------------------------------------------------------------
    // Recurring
    // -----------------------------------------------------------------------

    async fn create_recurring_entry(
        &self,
        request: Request<CreateRecurringEntryRequest>,
    ) -> Result<Response<Entry>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        validate::positive_amount(req.amount)?;
        validate::non_empty("entry_date", &req.entry_date)?;
        validate::non_empty("recurrence_rule", &req.recurrence_rule)?;
        let kind = EntryKind::try_from(req.kind).unwrap_or(EntryKind::Expense);
        let cat_id = if req.category_id.is_empty() {
            None
        } else {
            Some(req.category_id.as_str())
        };
        let notes = if req.notes.is_empty() {
            None
        } else {
            Some(req.notes.as_str())
        };
        let entry = self
            .biz
            .create_recurring_entry(
                &user_id,
                &req.budget_id,
                cat_id,
                kind,
                req.amount,
                &req.description,
                &req.entry_date,
                &req.tags,
                notes,
                &req.recurrence_rule,
            )
            .await?;
        Ok(Response::new(entry))
    }

    async fn update_recurrence_rule(
        &self,
        request: Request<UpdateRecurrenceRuleRequest>,
    ) -> Result<Response<Entry>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let rule = if req.recurrence_rule.is_empty() {
            None
        } else {
            Some(req.recurrence_rule.as_str())
        };
        let entry = self
            .biz
            .update_recurrence_rule(&user_id, &req.entry_id, rule)
            .await?;
        Ok(Response::new(entry))
    }

    async fn cancel_recurrence(
        &self,
        request: Request<CancelRecurrenceRequest>,
    ) -> Result<Response<Entry>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let entry = self.biz.cancel_recurrence(&user_id, &req.entry_id).await?;
        Ok(Response::new(entry))
    }

    // -----------------------------------------------------------------------
    // Split
    // -----------------------------------------------------------------------

    async fn create_split_entry(
        &self,
        request: Request<CreateSplitEntryRequest>,
    ) -> Result<Response<CreateSplitEntryResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        validate::positive_amount(req.total_amount)?;
        validate::non_empty("entry_date", &req.entry_date)?;
        let kind = EntryKind::try_from(req.kind).unwrap_or(EntryKind::Expense);
        let notes = if req.notes.is_empty() {
            None
        } else {
            Some(req.notes.as_str())
        };
        let resp = self
            .biz
            .create_split_entry(
                &user_id,
                &req.budget_id,
                kind,
                req.total_amount,
                &req.description,
                &req.entry_date,
                &req.tags,
                notes,
                req.legs,
            )
            .await?;
        Ok(Response::new(resp))
    }

    async fn list_split_legs(
        &self,
        request: Request<ListSplitLegsRequest>,
    ) -> Result<Response<ListSplitLegsResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let (legs, split_group_id) = self.biz.list_split_legs(&user_id, &req.entry_id).await?;
        Ok(Response::new(ListSplitLegsResponse {
            legs,
            split_group_id,
        }))
    }
}
