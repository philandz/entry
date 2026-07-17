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
