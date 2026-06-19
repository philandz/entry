#![allow(clippy::result_large_err)]
use tonic::Status;

pub fn user_id_from_metadata(meta: &tonic::metadata::MetadataMap) -> Result<String, Status> {
    tracing::debug!("entry validate: metadata keys: {:?}", meta.keys().collect::<Vec<_>>());
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
