//! Unit tests for EntryQueryBuilder conditions and binds.

use entry::manager::repository::EntryQueryBuilder;
use entry::pb::service::entry::ListParams;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_params() -> ListParams {
    ListParams::default()
}

fn cat(s: &str) -> String {
    s.to_string()
}

// ---------------------------------------------------------------------------
// Empty params → conditions = ["e.deleted_at IS NULL"]
// ---------------------------------------------------------------------------

#[test]
fn empty_params_conditions_deleted_at_only() {
    let params = make_params();
    let builder = EntryQueryBuilder::new(&params);
    let (sql, _) = builder.build_count();

    assert!(
        sql.contains("e.deleted_at IS NULL"),
        "expected deleted_at condition in: {sql}"
    );
    // No AND conditions beyond the deleted_at baseline
    assert!(
        sql.matches(" AND ").count() == 0,
        "expected no AND conditions in: {sql}"
    );
}

// ---------------------------------------------------------------------------
// category_ids with 2 values → "e.category_id IN (?,?)" + 2 binds
// ---------------------------------------------------------------------------

#[test]
fn category_ids_builds_in_clause_with_2_binds() {
    let mut params = make_params();
    params.category_ids = vec![
        cat("cat-1111-1111-1111-111111111111"),
        cat("cat-2222-2222-2222-222222222222"),
    ];

    let builder = EntryQueryBuilder::new(&params).apply_category();
    let (sql, binds) = builder.build_count();

    assert!(
        sql.contains("e.category_id IN (?,?)"),
        "expected IN clause in: {sql}"
    );
    assert_eq!(binds.len(), 2, "expected 2 binds, got: {binds:?}");
    assert!(
        binds.contains(&cat("cat-1111-1111-1111-111111111111")),
        "missing first category bind in: {binds:?}"
    );
    assert!(
        binds.contains(&cat("cat-2222-2222-2222-222222222222")),
        "missing second category bind in: {binds:?}"
    );
}

// ---------------------------------------------------------------------------
// member_ids with 2 values → filters by entry creator
// ---------------------------------------------------------------------------

#[test]
fn member_ids_filter_entry_creators() {
    let mut params = make_params();
    params.member_ids = vec![
        cat("user-1111-1111-1111-111111111111"),
        cat("user-2222-2222-2222-222222222222"),
    ];

    let builder = EntryQueryBuilder::new(&params).apply_member();
    let (sql, binds) = builder.build_count();

    assert!(
        sql.contains("e.created_by IN (?,?)"),
        "expected e.created_by IN (?,?) in: {sql}"
    );
    assert!(
        !sql.contains("budget_members"),
        "membership must not filter creators: {sql}"
    );
    assert_eq!(
        binds,
        vec![
            cat("user-1111-1111-1111-111111111111"),
            cat("user-2222-2222-2222-222222222222")
        ]
    );
}

#[test]
fn member_ids_empty_slice_adds_nothing() {
    let mut params = make_params();
    params.member_ids = vec![];
    let builder = EntryQueryBuilder::new(&params).apply_member();
    let (sql, binds) = builder.build_count();

    // No extra condition when slice is empty
    assert!(
        !sql.contains("budget_members"),
        "unexpected budget_members in: {sql}"
    );
    assert!(binds.is_empty(), "expected no binds, got: {binds:?}");
}

// ---------------------------------------------------------------------------
// tags: comma-separated, every tag required, exact match via FIND_IN_SET
// ---------------------------------------------------------------------------

#[test]
fn tags_require_every_listed_tag_exactly() {
    let mut params = make_params();
    params.tags = Some(cat("lunch, work ,"));

    let builder = EntryQueryBuilder::new(&params).apply_tags();
    let (sql, binds) = builder.build_count();

    assert_eq!(
        sql.matches("FIND_IN_SET(?, e.tags) > 0").count(),
        2,
        "{sql}"
    );
    assert_eq!(binds, vec![cat("lunch"), cat("work")]);
}

// ---------------------------------------------------------------------------
// date_from + date_to → both bounds present
// ---------------------------------------------------------------------------

#[test]
fn date_from_and_date_to_both_bounds() {
    let mut params = make_params();
    params.date_from = Some(cat("2026-07-01"));
    params.date_to = Some(cat("2026-07-15"));

    let builder = EntryQueryBuilder::new(&params).apply_date();
    let (sql, binds) = builder.build_count();

    assert!(
        sql.contains("e.entry_date >= ?"),
        "expected lower bound in: {sql}"
    );
    assert!(
        sql.contains("e.entry_date <= ?"),
        "expected upper bound in: {sql}"
    );
    assert_eq!(binds.len(), 2, "expected 2 binds, got: {binds:?}");
    assert!(binds.contains(&cat("2026-07-01")));
    assert!(binds.contains(&cat("2026-07-15")));
}

// ---------------------------------------------------------------------------
// Mixed: category_ids + member_ids + kind + date → all conditions present
// ---------------------------------------------------------------------------

#[test]
fn mixed_filters_all_conditions_present() {
    let mut params = make_params();
    params.category_ids = vec![
        cat("cat-1111-1111-1111-111111111111"),
        cat("cat-2222-2222-2222-222222222222"),
    ];
    params.member_ids = vec![cat("user-1111-1111-1111-111111111111")];
    params.kind = Some(cat("expense"));
    params.date_from = Some(cat("2026-07-01"));
    params.date_to = Some(cat("2026-07-15"));

    let builder = EntryQueryBuilder::new(&params)
        .apply_category()
        .apply_member()
        .apply_kind()
        .apply_date();
    let (sql, binds) = builder.build_count();

    assert!(
        sql.contains("e.category_id IN (?,?)"),
        "missing category IN: {sql}"
    );
    assert!(
        sql.contains("e.created_by IN (?)"),
        "missing member filter: {sql}"
    );
    assert!(sql.contains("e.kind = ?"), "missing kind: {sql}");
    assert!(
        sql.contains("e.entry_date >= ?"),
        "missing date_from: {sql}"
    );
    assert!(sql.contains("e.entry_date <= ?"), "missing date_to: {sql}");
    assert!(
        sql.contains("e.deleted_at IS NULL"),
        "missing deleted_at: {sql}"
    );

    // 2 category + 1 member + 1 kind + 2 date = 6 binds
    assert_eq!(binds.len(), 6, "expected 6 binds, got: {binds:?}");
}

// ---------------------------------------------------------------------------
// build_data produces LIMIT/OFFSET in query
// ---------------------------------------------------------------------------

#[test]
fn build_data_includes_limit_and_offset() {
    let params = make_params();
    let builder = EntryQueryBuilder::new(&params);
    let (sql, binds) = builder.build_data("entry_date", "DESC", 30, 0);

    assert!(
        sql.contains("LIMIT ? OFFSET ?"),
        "missing LIMIT/OFFSET in: {sql}"
    );
    // build_data appends limit + offset to binds
    assert!(binds.contains(&cat("30")), "missing limit bind: {binds:?}");
    assert!(binds.contains(&cat("0")), "missing offset bind: {binds:?}");
}

// ---------------------------------------------------------------------------
// build_count does NOT include LIMIT/OFFSET
// ---------------------------------------------------------------------------

#[test]
fn build_count_excludes_limit_offset() {
    let params = make_params();
    let builder = EntryQueryBuilder::new(&params);
    let (sql, _) = builder.build_count();

    assert!(!sql.contains("LIMIT"), "unexpected LIMIT in count: {sql}");
    assert!(!sql.contains("OFFSET"), "unexpected OFFSET in count: {sql}");
}

// ---------------------------------------------------------------------------
// kind=transfer matches both legs without a bind
// ---------------------------------------------------------------------------

#[test]
fn kind_transfer_matches_both_legs() {
    let mut params = make_params();
    params.kind = Some(cat("transfer"));
    let (sql, binds) = EntryQueryBuilder::new(&params).apply_kind().build_count();
    assert!(
        sql.contains("e.kind IN ('transfer_out','transfer_in')"),
        "{sql}"
    );
    assert!(binds.is_empty(), "{binds:?}");
}

// ---------------------------------------------------------------------------
// Stable paging: rows that share the sort key need a total order, otherwise a row can
// appear on two pages (or on none) when two pages are fetched at different times.
// ---------------------------------------------------------------------------

#[test]
fn data_query_has_a_total_order_after_the_sort_column() {
    let params = make_params();
    let builder = EntryQueryBuilder::new(&params);
    for (col, dir) in [
        ("e.entry_date", "DESC"),
        ("e.entry_date", "ASC"),
        ("e.amount_minor", "DESC"),
        ("e.description", "ASC"),
    ] {
        let (sql, _) = builder.build_data(col, dir, 30, 0);
        let want = format!("ORDER BY {col} {dir}, e.created_at {dir}, e.id {dir}");
        assert!(sql.contains(&want), "expected `{want}` in: {sql}");
    }
}
