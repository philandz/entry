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

#[test]
fn sharing_rejects_manual_entries_when_sharing_v2_is_on() {
    use entry::manager::validate::sharing_entry_allowed;
    let e = sharing_entry_allowed(true, BudgetType::Sharing).unwrap_err();
    assert_eq!(e.code(), tonic::Code::FailedPrecondition);
    assert!(sharing_entry_allowed(false, BudgetType::Sharing).is_ok());
    for t in [
        BudgetType::Standard,
        BudgetType::Saving,
        BudgetType::Invest,
        BudgetType::Debt,
    ] {
        assert!(sharing_entry_allowed(true, t).is_ok());
    }
}

#[test]
fn debt_budget_rejects_manual_entries_when_debt_v2_is_on() {
    use entry::manager::validate::debt_entry_allowed;
    let e = debt_entry_allowed(true, BudgetType::Debt).unwrap_err();
    assert_eq!(e.code(), tonic::Code::FailedPrecondition);
    assert!(debt_entry_allowed(false, BudgetType::Debt).is_ok());
    for t in [
        BudgetType::Standard,
        BudgetType::Saving,
        BudgetType::Invest,
        BudgetType::Sharing,
    ] {
        assert!(debt_entry_allowed(true, t).is_ok());
    }
}
