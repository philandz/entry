//! List search + whole-set summary against a real MySQL.
//! `DATABASE_URL=... cargo test --test list_summary_test -- --ignored`

use entry::manager::repository::EntryRepository;
use entry::pb::service::entry::EntryKind::{Expense, Income, TransferIn, TransferOut};
use entry::pb::service::entry::ListParams;

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (MySQL); run with --ignored"]
async fn search_covers_notes_tags_amount_and_summary_ignores_paging() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let repo = EntryRepository::new(&url).await.expect("repo + migrations");
    let (bud, other) = (uuid(), uuid());
    let u = "user-1";

    let add = |b: &str, k, amt: i64, desc: &str, date: &str, tags: &[&str], notes: Option<&str>| {
        let tags: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        let (b, desc, date, notes) = (
            b.to_string(),
            desc.to_string(),
            date.to_string(),
            notes.map(str::to_string),
        );
        let repo = &repo;
        async move {
            repo.create_entry(&b, None, k, amt, &desc, &date, &tags, notes.as_deref(), u)
                .await
                .unwrap()
        }
    };
    add(
        &bud,
        Expense,
        50_000,
        "Cafe",
        "2026-11-01",
        &["morning"],
        None,
    )
    .await;
    add(
        &bud,
        Expense,
        1_500_000,
        "Rent",
        "2026-11-02",
        &[],
        Some("paid to landlord"),
    )
    .await;
    add(
        &bud,
        Income,
        20_000_000,
        "Salary",
        "2026-11-03",
        &["work"],
        None,
    )
    .await;
    add(
        &bud,
        TransferOut,
        300,
        "To savings",
        "2026-11-04",
        &[],
        None,
    )
    .await;
    add(
        &bud,
        TransferIn,
        40,
        "From savings",
        "2026-11-04",
        &[],
        None,
    )
    .await;
    add(&bud, Expense, 10, "100% juice", "2026-11-05", &[], None).await;
    let gone = add(&bud, Expense, 777, "Deleted", "2026-11-05", &[], None).await;
    repo.delete_entry(&gone.id).await.unwrap();
    add(&other, Expense, 999, "Cafe", "2026-11-01", &[], None).await;

    let list = |p: ListParams| {
        let (repo, bud) = (&repo, bud.clone());
        async move { repo.list_entries(Some(&bud), &[], None, &p).await.unwrap() }
    };
    let p = |q: &str| ListParams {
        q: Some(q.into()),
        include_summary: Some(true),
        page: Some(1),
        page_size: Some(30),
        ..Default::default()
    };

    // description, notes, tag, exact amount (with separators), literal % — and no cross-budget leak
    assert_eq!(list(p("cafe")).await.meta.total_rows, 1);
    assert_eq!(list(p("landlord")).await.entries[0].description, "Rent");
    assert_eq!(list(p("morning")).await.entries[0].description, "Cafe");
    assert_eq!(list(p("1.500.000")).await.entries[0].description, "Rent");
    assert_eq!(list(p("100%")).await.entries[0].description, "100% juice");
    assert_eq!(list(p("%")).await.meta.total_rows, 1, "% is literal");

    // summary: whole filtered set, deleted rows excluded, other budget excluded
    let all = list(ListParams {
        include_summary: Some(true),
        page: Some(1),
        page_size: Some(30),
        ..Default::default()
    })
    .await;
    let s = all.summary.expect("summary requested");
    assert_eq!(
        (s.count, s.income, s.expense, s.transfer_in, s.transfer_out),
        (6, 20_000_000, 1_550_010, 40, 300)
    );

    // page_size 2 still summarises everything
    let paged = list(ListParams {
        include_summary: Some(true),
        page: Some(1),
        page_size: Some(2),
        ..Default::default()
    })
    .await;
    assert_eq!(paged.entries.len(), 2);
    assert_eq!(paged.summary.unwrap().count, 6);

    // not requested -> not computed
    assert!(list(ListParams {
        page: Some(1),
        page_size: Some(5),
        ..Default::default()
    })
    .await
    .summary
    .is_none());

    // summary follows the filter
    let filtered = list(ListParams {
        include_summary: Some(true),
        kind: Some("expense".into()),
        page: Some(1),
        page_size: Some(30),
        ..Default::default()
    })
    .await;
    let s = filtered.summary.unwrap();
    assert_eq!((s.count, s.income, s.expense), (3, 0, 1_550_010));
}
