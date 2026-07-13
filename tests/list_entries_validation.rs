//! Unit tests for entry validators: date_range, range_max_days, repeated_ids.

use entry::manager::validate::{date_range, range_max_days, repeated_ids};

fn valid_uuid() -> String {
    "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string()
}

// ---------------------------------------------------------------------------
// date_range
// ---------------------------------------------------------------------------

#[test]
fn date_range_accepts_valid_yyyy_mm_dd() {
    assert!(date_range("2026-01-01", "2026-01-15").is_ok());
    assert!(date_range("2026-07-01", "2026-07-13").is_ok());
}

#[test]
fn date_range_accepts_empty_range() {
    assert!(date_range("", "").is_ok());
    assert!(date_range("2026-01-01", "").is_ok());
    assert!(date_range("", "2026-01-15").is_ok());
}

#[test]
fn date_range_rejects_invalid_format() {
    // wrong separator / order
    assert!(date_range("01-01-2026", "2026-01-15").is_err());
    assert!(date_range("2026/01/01", "2026-01-15").is_err());
    assert!(date_range("2026-01-01", "15-01-2026").is_err());
    // garbage
    assert!(date_range("not-a-date", "2026-01-15").is_err());
    assert!(date_range("2026-01-01", "also-not").is_err());
}

#[test]
fn date_range_rejects_from_after_to() {
    assert!(date_range("2026-07-15", "2026-07-01").is_err());
    assert!(date_range("2026-12-31", "2026-01-01").is_err());
}

// ---------------------------------------------------------------------------
// range_max_days
// ---------------------------------------------------------------------------

#[test]
fn range_max_days_accepts_ranges_within_limit() {
    // exactly 1 day
    assert!(range_max_days("2026-07-01", "2026-07-02", 30).is_ok());
    // 7 days
    assert!(range_max_days("2026-07-01", "2026-07-08", 30).is_ok());
    // 15 days
    assert!(range_max_days("2026-07-01", "2026-07-16", 30).is_ok());
}

#[test]
fn range_max_days_accepts_exactly_30_days() {
    // 30-day span = day 0 to day 30 = 30 days difference
    assert!(range_max_days("2026-07-01", "2026-07-31", 30).is_ok());
}

#[test]
fn range_max_days_rejects_exceeding_30_days() {
    // 31 days
    assert!(range_max_days("2026-07-01", "2026-08-01", 30).is_err());
    // 60 days
    assert!(range_max_days("2026-05-01", "2026-07-01", 30).is_err());
    // 90 days
    assert!(range_max_days("2026-04-01", "2026-07-01", 30).is_err());
}

#[test]
fn range_max_days_accepts_empty_range() {
    // either bound empty is a no-op
    assert!(range_max_days("", "2026-07-15", 30).is_ok());
    assert!(range_max_days("2026-07-01", "", 30).is_ok());
    assert!(range_max_days("", "", 30).is_ok());
}

#[test]
fn range_max_days_rejects_invalid_format() {
    assert!(range_max_days("not-date", "2026-07-15", 30).is_err());
    assert!(range_max_days("2026-07-01", "also-not", 30).is_err());
}

// ---------------------------------------------------------------------------
// repeated_ids
// ---------------------------------------------------------------------------

#[test]
fn repeated_ids_accepts_empty_slice() {
    assert!(repeated_ids(&[], "category_ids", 50).is_ok());
}

#[test]
fn repeated_ids_accepts_up_to_50_ids() {
    let ids: Vec<String> = (0..50).map(|i| format!("{:036}", i)).collect();
    assert!(repeated_ids(&ids, "category_ids", 50).is_ok());

    // 1 id is fine
    assert!(repeated_ids(&[valid_uuid()], "category_ids", 50).is_ok());
    // 2 ids
    assert!(repeated_ids(&[valid_uuid(), valid_uuid()], "category_ids", 50).is_ok());
}

#[test]
fn repeated_ids_rejects_more_than_50_ids() {
    let ids: Vec<String> = (0..51).map(|i| format!("{:036}", i)).collect();
    let result = repeated_ids(&ids, "category_ids", 50);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.message();
    assert!(msg.contains("category_ids"), "got: {msg}");
    assert!(msg.contains("50"), "got: {msg}");
}

#[test]
fn repeated_ids_rejects_ids_longer_than_36_chars() {
    // 37-char string
    let long_id = "a".repeat(37);
    let result = repeated_ids(&[long_id], "member_ids", 50);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.message();
    assert!(msg.contains("member_ids"), "got: {msg}");
    assert!(msg.contains("invalid UUID"), "got: {msg}");
}

#[test]
fn repeated_ids_rejects_malformed_uuid() {
    // valid length but invalid chars
    let bad_id = "not-a-uuid-not-a-uuid-not-a-uuid!!".to_string();
    let result = repeated_ids(&[bad_id], "category_ids", 50);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.message();
    assert!(msg.contains("invalid UUID"), "got: {msg}");
}

#[test]
fn repeated_ids_reports_field_name_in_error() {
    let ids: Vec<String> = (0..51).map(|i| format!("{:036}", i)).collect();
    let result = repeated_ids(&ids, "member_ids", 50);
    let err = result.unwrap_err();
    assert!(err.message().contains("member_ids"));
}
