//! Debt payment legs (plan §4): both legs land together, a replay finds the same legs, a void
//! undoes both. `DATABASE_URL=... cargo test --test debt_legs_test -- --ignored`

use entry::manager::repository::transfers::NewDebtLegs;
use entry::manager::repository::EntryRepository;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn legs<'a>(
    group: &'a str,
    from: &'a str,
    debt: &'a str,
    principal: i64,
    cost: i64,
    cat: &'a str,
) -> NewDebtLegs<'a> {
    NewDebtLegs {
        group_id: group,
        from_budget_id: from,
        debt_budget_id: debt,
        principal_minor: principal,
        cost_minor: cost,
        category_id: cat,
        entry_date: "2026-11-25",
        description: "Trả nợ",
        created_by: "u1",
    }
}

async fn kinds(repo: &EntryRepository, group: &str) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = repo
        .get_transfer_legs(group)
        .await
        .unwrap()
        .into_iter()
        .map(|e| (e.kind, e.amount_minor))
        .collect();
    v.sort();
    v
}

async fn repo() -> EntryRepository {
    EntryRepository::new(&std::env::var("DATABASE_URL").expect("DATABASE_URL"))
        .await
        .expect("repo")
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (MySQL); run with --ignored"]
async fn principal_is_a_transfer_and_interest_plus_fee_an_expense_in_one_group() {
    let repo = repo().await;
    let (g, from, debt, cat) = (id(), id(), id(), id());
    let (p, c, replayed) = repo
        .create_debt_legs(&legs(&g, &from, &debt, 11_900_375, 1_873_973, &cat))
        .await
        .unwrap();
    assert!(!p.is_empty() && !c.is_empty() && !replayed);
    assert_eq!(
        kinds(&repo, &g).await,
        vec![
            ("expense".to_string(), 1_873_973),
            ("transfer_out".to_string(), 11_900_375)
        ]
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (MySQL); run with --ignored"]
async fn a_zero_amount_leg_is_skipped() {
    let repo = repo().await;
    let (g, from, debt, cat) = (id(), id(), id(), id());
    let (p, c, _) = repo
        .create_debt_legs(&legs(&g, &from, &debt, 5_000, 0, &cat))
        .await
        .unwrap();
    assert!(!p.is_empty() && c.is_empty());
    assert_eq!(
        kinds(&repo, &g).await,
        vec![("transfer_out".to_string(), 5_000)]
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (MySQL); run with --ignored"]
async fn a_replay_returns_the_same_legs_without_writing_twice() {
    let repo = repo().await;
    let (g, from, debt, cat) = (id(), id(), id(), id());
    let first = repo
        .create_debt_legs(&legs(&g, &from, &debt, 100, 10, &cat))
        .await
        .unwrap();
    let again = repo
        .create_debt_legs(&legs(&g, &from, &debt, 100, 10, &cat))
        .await
        .unwrap();
    assert!(again.2, "second call is a replay");
    assert_eq!((&first.0, &first.1), (&again.0, &again.1));
    assert_eq!(kinds(&repo, &g).await.len(), 2);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (MySQL); run with --ignored"]
async fn voiding_removes_both_legs_and_is_idempotent_and_blocks_a_late_replay() {
    let repo = repo().await;
    let (g, from, debt, cat) = (id(), id(), id(), id());
    repo.create_debt_legs(&legs(&g, &from, &debt, 100, 10, &cat))
        .await
        .unwrap();
    assert_eq!(repo.void_debt_legs(&g, &from).await.unwrap(), 2);
    assert_eq!(repo.void_debt_legs(&g, &from).await.unwrap(), 0);
    assert!(kinds(&repo, &g).await.is_empty());
    // a late retry of the same payment must not resurrect the legs
    assert!(repo
        .create_debt_legs(&legs(&g, &from, &debt, 100, 10, &cat))
        .await
        .is_err());
}
