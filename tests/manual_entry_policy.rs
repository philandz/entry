use entry::manager::validate::manual_entry_allowed;
use entry::pb::service::budget::BudgetType;
use tonic::Code;

#[test]
fn saving_budget_rejects_manual_entries_when_fund_enabled() {
    let e = manual_entry_allowed(true, BudgetType::Saving).unwrap_err();
    assert_eq!(e.code(), Code::FailedPrecondition);
}

#[test]
fn saving_budget_unchanged_when_flag_off() {
    assert!(manual_entry_allowed(false, BudgetType::Saving).is_ok());
}

#[test]
fn other_budget_types_unaffected() {
    for t in [
        BudgetType::Standard,
        BudgetType::Invest,
        BudgetType::Sharing,
    ] {
        assert!(manual_entry_allowed(true, t).is_ok());
    }
}
