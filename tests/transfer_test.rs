//! R1 — entry-service transfers: static validation, compatibility rules, and the
//! DB-backed pair lifecycle (`DATABASE_URL=... cargo test --test transfer_test -- --ignored`).

use entry::manager::client::BudgetCtx;
use entry::manager::repository::transfers::NewLeg;
use entry::manager::validate::{create_transfer, transfer_compat};
use entry::pb::service::budget::{BudgetRole, BudgetType};
use entry::pb::service::entry::{CreateTransferRequest, EntryKind};
use tonic::Code;

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn req() -> CreateTransferRequest {
    CreateTransferRequest {
        source_budget_id: uuid(),
        destination_budget_id: uuid(),
        amount: 1000,
        entry_date: "2026-10-23".into(),
        description: "x".into(),
        notes: None,
        idempotency_key: String::new(),
    }
}

fn ctx(t: BudgetType, org: &str, cur: &str) -> BudgetCtx {
    BudgetCtx {
        role: BudgetRole::Owner,
        org_id: org.into(),
        currency: cur.into(),
        budget_type: t,
        name: "n".into(),
    }
}

#[test]
fn create_transfer_validation() {
    assert!(create_transfer(&req()).is_ok());

    let mut r = req();
    r.destination_budget_id = r.source_budget_id.clone();
    assert_eq!(
        create_transfer(&r).unwrap_err().code(),
        Code::InvalidArgument
    );

    for amount in [0, -5] {
        let mut r = req();
        r.amount = amount;
        assert_eq!(
            create_transfer(&r).unwrap_err().code(),
            Code::InvalidArgument
        );
    }
    let mut r = req();
    r.entry_date = "23/10/2026".into();
    assert!(create_transfer(&r).is_err());
    let mut r = req();
    r.source_budget_id = "nope".into();
    assert!(create_transfer(&r).is_err());
    let mut r = req();
    r.idempotency_key = "k".repeat(65);
    assert!(create_transfer(&r).is_err());
}

#[test]
fn compat_rules() {
    let s = BudgetType::Standard;
    assert!(transfer_compat(&ctx(s, "o", "VND"), &ctx(s, "o", "VND")).is_ok());
    // different org, currency, or non-standard types are FAILED_PRECONDITION
    for (a, b) in [
        (ctx(s, "o1", "VND"), ctx(s, "o2", "VND")),
        (ctx(s, "o", "VND"), ctx(s, "o", "USD")),
        (ctx(s, "o", "VND"), ctx(BudgetType::Saving, "o", "VND")),
        (ctx(BudgetType::Sharing, "o", "VND"), ctx(s, "o", "VND")),
        (ctx(s, "", "VND"), ctx(s, "", "VND")),
    ] {
        assert_eq!(
            transfer_compat(&a, &b).unwrap_err().code(),
            Code::FailedPrecondition
        );
    }
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (MySQL); run with --ignored"]
async fn pair_lifecycle_and_totals_exclude_transfers_from_expense() {
    use entry::manager::repository::EntryRepository;
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let repo = EntryRepository::new(&url).await.unwrap();

    let (a, b, g, key) = (uuid(), uuid(), uuid(), uuid());
    let out = NewLeg {
        budget_id: &a,
        kind: EntryKind::TransferOut,
        amount: 500,
        entry_date: "2026-10-05",
        description: "to b",
        notes: None,
        group_id: &g,
        counterparty_budget_id: &b,
        counterparty_kind: "entry",
        idempotency_key: Some(&key),
        created_by: "u",
    };
    let inn = NewLeg {
        budget_id: &b,
        kind: EntryKind::TransferIn,
        amount: 500,
        entry_date: "2026-10-05",
        description: "from a",
        notes: None,
        group_id: &g,
        counterparty_budget_id: &a,
        counterparty_kind: "entry",
        idempotency_key: Some(&key),
        created_by: "u",
    };
    let (o, i) = repo.create_transfer_pair(&out, &inn).await.unwrap();
    assert_eq!(o.transfer_group_id.as_deref(), Some(g.as_str()));
    assert_eq!(i.counterparty_budget_id.as_deref(), Some(a.as_str()));

    // same key on the same budget is rejected by the unique index (replays are looked up first)
    assert!(repo.find_leg_by_key(&a, &key).await.unwrap().is_some());
    assert!(repo.create_transfer_pair(&out, &inn).await.is_err());
    // …and the failed second attempt left no half pair behind
    assert_eq!(repo.get_transfer_legs(&g).await.unwrap().len(), 2);

    // I3: transfers never count as expense or income
    let rows = repo
        .get_period_totals(
            &[a.clone(), b.clone()],
            "2026-10-01",
            "2026-11-01",
            false,
            false,
        )
        .await
        .unwrap();
    for r in &rows {
        assert_eq!((r.income, r.expense), (0, 0));
    }
    let ra = rows.iter().find(|r| r.budget_id == a).unwrap();
    let rb = rows.iter().find(|r| r.budget_id == b).unwrap();
    assert_eq!((ra.transfer_out, ra.transfer_in), (500, 0));
    assert_eq!((rb.transfer_out, rb.transfer_in), (0, 500));

    // update both legs together
    repo.update_transfer(&g, Some(700), Some("2026-10-06"), None, Some("n"))
        .await
        .unwrap();
    let legs = repo.get_transfer_legs(&g).await.unwrap();
    assert!(legs
        .iter()
        .all(|l| l.description == "to b" || l.description == "from a"));

    // delete removes both, and a deleted key is still found (so it can't be reused silently)
    repo.delete_transfer(&g).await.unwrap();
    assert!(repo.get_transfer_legs(&g).await.unwrap().is_empty());
    assert!(repo
        .find_leg_by_key(&a, &key)
        .await
        .unwrap()
        .unwrap()
        .deleted_at
        .is_some());

    // single-leg void is idempotent
    let (c, k2) = (uuid(), uuid());
    let leg = NewLeg {
        budget_id: &c,
        kind: EntryKind::TransferOut,
        amount: 10,
        entry_date: "2026-10-05",
        description: "fund",
        notes: None,
        group_id: &uuid(),
        counterparty_budget_id: &uuid(),
        counterparty_kind: "fund_cash",
        idempotency_key: Some(&k2),
        created_by: "u",
    };
    repo.create_transfer_leg(&leg).await.unwrap();
    repo.void_transfer_leg(&c, &k2).await.unwrap();
    repo.void_transfer_leg(&c, &k2).await.unwrap();
}
