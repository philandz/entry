//! T0.3a — GetPeriodTotals: request validation (unit) and SQL aggregation
//! (integration, needs MySQL: `DATABASE_URL=... cargo test --test period_totals -- --ignored`).

use entry::manager::validate::{period_totals_window, PERIOD_TOTALS_MAX_BUDGETS};
use tonic::Code;

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[test]
fn window_accepts_a_normal_month() {
    let ids = vec![uuid()];
    let (from, to) = period_totals_window(&ids, "2026-10-01", "2026-11-01").unwrap();
    assert_eq!((to - from).num_days(), 31);
}

#[test]
fn window_rejects_bad_input() {
    let ok = vec![uuid()];
    let cases: Vec<(Vec<String>, &str, &str)> = vec![
        (vec![], "2026-10-01", "2026-11-01"), // no budgets
        (vec!["not-a-uuid".into()], "2026-10-01", "2026-11-01"), // bad id
        (ok.clone(), "", "2026-11-01"),       // missing from
        (ok.clone(), "2026-10-01", ""),       // missing to
        (ok.clone(), "01/10/2026", "2026-11-01"), // wrong format
        (ok.clone(), "2026-11-01", "2026-10-01"), // reversed
        (ok.clone(), "2026-10-01", "2026-10-01"), // empty window (to is exclusive)
        (ok.clone(), "2000-01-01", "2026-11-01"), // > 3660 days
    ];
    for (ids, from, to) in cases {
        let err = period_totals_window(&ids, from, to).expect_err(&format!("{ids:?} {from} {to}"));
        assert_eq!(err.code(), Code::InvalidArgument);
    }
}

#[test]
fn window_caps_budget_count() {
    let ids: Vec<String> = (0..=PERIOD_TOTALS_MAX_BUDGETS).map(|_| uuid()).collect();
    assert!(period_totals_window(&ids, "2026-10-01", "2026-11-01").is_err());
    let ids: Vec<String> = (0..PERIOD_TOTALS_MAX_BUDGETS).map(|_| uuid()).collect();
    assert!(period_totals_window(&ids, "2026-10-01", "2026-11-01").is_ok());
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (MySQL); run with --ignored"]
async fn aggregates_kinds_categories_days_and_ignores_deleted_and_out_of_range() {
    use entry::manager::repository::EntryRepository;
    use entry::pb::service::entry::EntryKind::{Expense, Income, TransferIn, TransferOut};

    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let repo = EntryRepository::new(&url).await.expect("repo + migrations");

    let (a, b, c1, c2) = (uuid(), uuid(), uuid(), uuid());
    let u = "user-1";
    let add = |bud: &str, cat: Option<&str>, k, amt: i64, date: &str| {
        let (bud, cat, date) = (bud.to_string(), cat.map(str::to_string), date.to_string());
        let repo = &repo;
        async move {
            repo.create_entry(&bud, cat.as_deref(), k, amt, "t", &date, &[], None, u)
                .await
                .unwrap()
        }
    };

    add(&a, Some(&c1), Expense, 100, "2026-10-01").await;
    add(&a, Some(&c1), Expense, 50, "2026-10-01").await;
    add(&a, Some(&c2), Expense, 70, "2026-10-02").await;
    add(&a, None, Income, 1000, "2026-10-02").await;
    add(&a, None, TransferOut, 300, "2026-10-03").await;
    add(&a, None, TransferIn, 40, "2026-10-03").await;
    add(&b, None, Expense, 5, "2026-10-01").await;
    add(&a, Some(&c1), Expense, 999, "2026-11-01").await; // `to` is exclusive
    add(&a, Some(&c1), Expense, 888, "2026-09-30").await; // before `from`
    let deleted = add(&a, Some(&c1), Expense, 777, "2026-10-04").await;
    repo.delete_entry(&deleted.id).await.unwrap();

    let ids = vec![a.clone(), b.clone()];
    let q = |cat: bool, day: bool| {
        let (ids, repo) = (ids.clone(), &repo);
        async move {
            repo.get_period_totals(&ids, "2026-10-01", "2026-11-01", cat, day)
                .await
                .unwrap()
        }
    };

    // No grouping: one row per budget.
    let rows = q(false, false).await;
    let ra = rows.iter().find(|r| r.budget_id == a).unwrap();
    assert_eq!(
        (
            ra.income,
            ra.expense,
            ra.transfer_in,
            ra.transfer_out,
            ra.cnt
        ),
        (1000, 220, 40, 300, 6)
    );
    let rb = rows.iter().find(|r| r.budget_id == b).unwrap();
    assert_eq!((rb.expense, rb.cnt), (5, 1));
    assert_eq!(rows.len(), 2);

    // By category: uncategorised rows have category_id "".
    let rows = q(true, false).await;
    let get = |bud: &str, cat: &str| {
        rows.iter()
            .find(|r| r.budget_id == bud && r.category_id == cat)
    };
    assert_eq!(get(&a, &c1).unwrap().expense, 150);
    assert_eq!(get(&a, &c2).unwrap().expense, 70);
    assert_eq!(get(&a, "").unwrap().income, 1000);
    assert_eq!(get(&a, "").unwrap().transfer_out, 300);

    // By day.
    let rows = q(false, true).await;
    let day = |bud: &str, d: &str| {
        rows.iter()
            .find(|r| r.budget_id == bud && r.day == d)
            .unwrap()
    };
    assert_eq!(day(&a, "2026-10-01").expense, 150);
    assert_eq!(day(&a, "2026-10-02").income, 1000);
    assert_eq!(day(&a, "2026-10-03").transfer_out, 300);
    assert!(
        rows.iter().all(|r| r.day != "2026-10-04"),
        "deleted entry must not appear"
    );

    // Totals ignore transfers for the income/expense sums (invariant I3).
    let total_expense: i64 = q(false, false).await.iter().map(|r| r.expense).sum();
    assert_eq!(total_expense, 225);
}
