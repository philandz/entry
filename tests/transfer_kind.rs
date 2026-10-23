//! T0.1 — transfer entry kinds: DB mapping and rejection on the generic RPCs.

use entry::converters::{is_transfer, kind_from_db, kind_to_db};
use entry::manager::validate::reject_transfer_kind;
use entry::pb::service::entry::EntryKind;
use tonic::Code;

#[test]
fn kind_round_trips_through_db_strings() {
    for (kind, db) in [
        (EntryKind::Expense, "expense"),
        (EntryKind::Income, "income"),
        (EntryKind::TransferOut, "transfer_out"),
        (EntryKind::TransferIn, "transfer_in"),
    ] {
        assert_eq!(kind_to_db(kind), db);
        assert_eq!(kind_from_db(db), kind);
    }
}

#[test]
fn unknown_db_kind_still_falls_back_to_expense() {
    // Legacy behaviour is preserved so old rows never break reads.
    assert_eq!(kind_from_db("bogus"), EntryKind::Expense);
    assert_eq!(kind_from_db(""), EntryKind::Expense);
}

#[test]
fn only_transfer_kinds_are_transfers() {
    assert!(is_transfer(EntryKind::TransferOut));
    assert!(is_transfer(EntryKind::TransferIn));
    assert!(!is_transfer(EntryKind::Expense));
    assert!(!is_transfer(EntryKind::Income));
    assert!(!is_transfer(EntryKind::Unspecified));
}

#[test]
fn generic_rpcs_reject_transfer_kinds() {
    for k in [EntryKind::TransferOut, EntryKind::TransferIn] {
        let err = reject_transfer_kind(k).expect_err("transfer kind must be rejected");
        assert_eq!(err.code(), Code::InvalidArgument);
    }
    for k in [
        EntryKind::Expense,
        EntryKind::Income,
        EntryKind::Unspecified,
    ] {
        assert!(reject_transfer_kind(k).is_ok());
    }
}

#[test]
fn proto_numbers_are_stable() {
    // Wire numbers must never change (see skills/05-protobuf/versioning.md).
    assert_eq!(EntryKind::Expense as i32, 1);
    assert_eq!(EntryKind::Income as i32, 2);
    assert_eq!(EntryKind::TransferOut as i32, 3);
    assert_eq!(EntryKind::TransferIn as i32, 4);
}
