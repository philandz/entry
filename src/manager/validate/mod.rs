#![allow(clippy::result_large_err)]
use tonic::Status;

pub fn user_id_from_metadata(meta: &tonic::metadata::MetadataMap) -> Result<String, Status> {
    meta.get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Status::unauthenticated("missing x-user-id metadata"))
}

pub fn non_empty(field: &str, value: &str) -> Result<(), Status> {
    if value.trim().is_empty() {
        return Err(Status::invalid_argument(format!("{field} must not be empty")));
    }
    Ok(())
}

pub fn positive_amount(amount: i64) -> Result<(), Status> {
    if amount <= 0 {
        return Err(Status::invalid_argument("amount must be positive"));
    }
    Ok(())
}
