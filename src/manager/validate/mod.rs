#![allow(clippy::result_large_err)]
use tonic::Status;

pub fn user_id_from_metadata(meta: &tonic::metadata::MetadataMap) -> Result<String, Status> {
    tracing::debug!(
        "entry validate: metadata keys: {:?}",
        meta.keys().collect::<Vec<_>>()
    );
    let x_user_id = meta.get("x-user-id");
    tracing::debug!("entry validate: x-user-id value: {:?}", x_user_id);
    x_user_id
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("entry validate: x-user-id not found or empty in metadata");
            Status::unauthenticated("missing x-user-id metadata")
        })
}

pub fn user_type_from_metadata(meta: &tonic::metadata::MetadataMap) -> Option<String> {
    meta.get("x-user-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

pub fn non_empty(field: &str, value: &str) -> Result<(), Status> {
    if value.trim().is_empty() {
        return Err(Status::invalid_argument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

/// Transfer kinds are only created through the transfer RPCs (they always come
/// in linked pairs). The generic entry RPCs must refuse them.
pub fn reject_transfer_kind(kind: crate::pb::service::entry::EntryKind) -> Result<(), Status> {
    if crate::converters::is_transfer(kind) {
        return Err(Status::invalid_argument(
            "transfer entries cannot be created or edited here; use the transfer endpoints",
        ));
    }
    Ok(())
}

pub fn positive_amount(amount: i64) -> Result<(), Status> {
    if amount <= 0 {
        return Err(Status::invalid_argument("amount must be positive"));
    }
    Ok(())
}

pub fn date_range(date_from: &str, date_to: &str) -> Result<(), Status> {
    if date_from.is_empty() && date_to.is_empty() {
        return Ok(());
    }
    let from = if date_from.is_empty() {
        None
    } else {
        chrono::NaiveDate::parse_from_str(date_from, "%Y-%m-%d")
            .map_err(|_| Status::invalid_argument("date_from must be YYYY-MM-DD"))?
            .into()
    };
    let to = if date_to.is_empty() {
        None
    } else {
        chrono::NaiveDate::parse_from_str(date_to, "%Y-%m-%d")
            .map_err(|_| Status::invalid_argument("date_to must be YYYY-MM-DD"))?
            .into()
    };
    match (from, to) {
        (Some(f), Some(t)) if f > t => Err(Status::invalid_argument(
            "date_from must not be after date_to",
        )),
        _ => Ok(()),
    }
}

/// Longest `date_from..date_to` span a list query accepts (a year, plus a leap day). An empty end
/// is allowed: the list is paginated, so an open range is bounded by page size, not by dates.
pub const LIST_MAX_RANGE_DAYS: u32 = 366;

pub fn range_max_days(date_from: &str, date_to: &str, max_days: u32) -> Result<(), Status> {
    if date_from.is_empty() || date_to.is_empty() {
        return Ok(());
    }
    let from = chrono::NaiveDate::parse_from_str(date_from, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument("date_from must be YYYY-MM-DD"))?;
    let to = chrono::NaiveDate::parse_from_str(date_to, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument("date_to must be YYYY-MM-DD"))?;
    let days = (to - from).num_days();
    if days < 0 {
        return Err(Status::invalid_argument(
            "date_from must not be after date_to",
        ));
    }
    if days as u32 > max_days {
        return Err(Status::invalid_argument(format!(
            "date range must not exceed {max_days} days"
        )));
    }
    Ok(())
}

pub fn repeated_ids(ids: &[String], field: &str, max: usize) -> Result<(), Status> {
    if ids.len() > max {
        return Err(Status::invalid_argument(format!(
            "{field} must not have more than {max} entries"
        )));
    }
    for id in ids {
        if id.len() != 36 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(Status::invalid_argument(format!(
                "{field} contains invalid UUID: {id}"
            )));
        }
    }
    Ok(())
}

/// Max number of budgets and max span accepted by `GetPeriodTotals`.
pub const PERIOD_TOTALS_MAX_BUDGETS: usize = 200;
pub const PERIOD_TOTALS_MAX_DAYS: i64 = 3660;

/// Validates a `PeriodTotalsRequest` window. Returns the parsed (from, to) dates.
pub fn period_totals_window(
    budget_ids: &[String],
    date_from: &str,
    date_to: &str,
) -> Result<(chrono::NaiveDate, chrono::NaiveDate), Status> {
    if budget_ids.is_empty() {
        return Err(Status::invalid_argument("budget_ids must not be empty"));
    }
    repeated_ids(budget_ids, "budget_ids", PERIOD_TOTALS_MAX_BUDGETS)?;
    let from = chrono::NaiveDate::parse_from_str(date_from, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument("date_from must be YYYY-MM-DD"))?;
    let to = chrono::NaiveDate::parse_from_str(date_to, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument("date_to must be YYYY-MM-DD"))?;
    if from >= to {
        return Err(Status::invalid_argument("date_from must be before date_to"));
    }
    if (to - from).num_days() > PERIOD_TOTALS_MAX_DAYS {
        return Err(Status::invalid_argument(format!(
            "date range must not exceed {PERIOD_TOTALS_MAX_DAYS} days"
        )));
    }
    Ok((from, to))
}

pub const TRANSFER_MAX_DESCRIPTION: usize = 500;

pub fn iso_date(field: &str, v: &str) -> Result<chrono::NaiveDate, Status> {
    chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument(format!("{field} must be YYYY-MM-DD")))
}

fn uuid_field(field: &str, v: &str) -> Result<(), Status> {
    uuid::Uuid::parse_str(v)
        .map(|_| ())
        .map_err(|_| Status::invalid_argument(format!("{field} must be a UUID")))
}

/// Static checks for CreateTransfer (no I/O).
pub fn create_transfer(
    req: &crate::pb::service::entry::CreateTransferRequest,
) -> Result<(), Status> {
    uuid_field("source_budget_id", &req.source_budget_id)?;
    uuid_field("destination_budget_id", &req.destination_budget_id)?;
    if req.source_budget_id == req.destination_budget_id {
        return Err(Status::invalid_argument(
            "source and destination budgets must differ",
        ));
    }
    positive_amount(req.amount)?;
    iso_date("entry_date", &req.entry_date)?;
    transfer_text(&req.description, req.notes.as_deref(), &req.idempotency_key)
}

pub fn transfer_text(
    description: &str,
    notes: Option<&str>,
    idempotency_key: &str,
) -> Result<(), Status> {
    if description.chars().count() > TRANSFER_MAX_DESCRIPTION {
        return Err(Status::invalid_argument("description too long"));
    }
    if notes.map(|n| n.chars().count() > 2000).unwrap_or(false) {
        return Err(Status::invalid_argument("notes too long"));
    }
    if idempotency_key.len() > 64 {
        return Err(Status::invalid_argument(
            "idempotency_key must be at most 64 characters",
        ));
    }
    Ok(())
}

/// Entry-service transfers move money between two *standard* budgets of the same
/// organisation and currency. Everything else is rejected with FAILED_PRECONDITION
/// (saving/invest funds go through the budget service's fund deposit/withdraw).
pub fn transfer_compat(
    src: &crate::manager::client::BudgetCtx,
    dst: &crate::manager::client::BudgetCtx,
) -> Result<(), Status> {
    use crate::pb::service::budget::BudgetType;
    if src.budget_type != BudgetType::Standard || dst.budget_type != BudgetType::Standard {
        return Err(Status::failed_precondition(
            "transfers here are only between standard budgets; use fund deposit/withdraw for saving or invest budgets",
        ));
    }
    if src.org_id.is_empty() || src.org_id != dst.org_id {
        return Err(Status::failed_precondition(
            "budgets must belong to the same organisation",
        ));
    }
    if src.currency.is_empty() || src.currency != dst.currency {
        return Err(Status::failed_precondition(
            "budgets must use the same currency",
        ));
    }
    Ok(())
}

/// `FEATURE_SAVING_FUND=true` turns Saving budgets into funds (no manual income/expense).
pub fn saving_fund_enabled() -> bool {
    matches!(
        std::env::var("FEATURE_SAVING_FUND").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

/// `FEATURE_SHARING_V2=true` keeps Sharing budgets out of the personal ledger:
/// their spending lives in the sharing service (expenses, split, settlement).
pub fn sharing_v2_enabled() -> bool {
    matches!(
        std::env::var("FEATURE_SHARING_V2").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

/// Sharing budgets record group expenses in the sharing service; a manual
/// income/expense entry there would sit outside the split and the balances.
pub fn sharing_entry_allowed(
    sharing_v2: bool,
    budget_type: crate::pb::service::budget::BudgetType,
) -> Result<(), tonic::Status> {
    if sharing_v2 && budget_type == crate::pb::service::budget::BudgetType::Sharing {
        return Err(tonic::Status::failed_precondition(
            "sharing budgets record group expenses, not income/expense entries",
        ));
    }
    Ok(())
}

/// `FEATURE_DEBT_V2=true` turns Debt budgets into debt books: balances change through recorded
/// debt payments (plan DB15), never through manual income/expense entries.
pub fn debt_v2_enabled() -> bool {
    matches!(
        std::env::var("FEATURE_DEBT_V2").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

pub fn debt_entry_allowed(
    debt_v2: bool,
    budget_type: crate::pb::service::budget::BudgetType,
) -> Result<(), tonic::Status> {
    if debt_v2 && budget_type == crate::pb::service::budget::BudgetType::Debt {
        return Err(tonic::Status::failed_precondition(
            "debt budgets are debt books: record a payment on a debt account",
        ));
    }
    Ok(())
}

/// Saving funds are changed through transfers and portfolio actions only.
pub fn manual_entry_allowed(
    fund_enabled: bool,
    budget_type: crate::pb::service::budget::BudgetType,
) -> Result<(), tonic::Status> {
    if fund_enabled && budget_type == crate::pb::service::budget::BudgetType::Saving {
        return Err(tonic::Status::failed_precondition(
            "saving budgets are funds: move money with transfers or portfolio actions",
        ));
    }
    Ok(())
}
