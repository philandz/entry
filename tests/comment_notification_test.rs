//! Comment notification event shape and sink mapping.
use entry::manager::client::notification::{classify, to_publish_request};
use entry::manager::notifier::{comment_added_event, excerpt};
use philand_event::SinkError;

#[test]
fn thread_members_exclude_the_author_and_duplicates() {
    let ev = comment_added_event(
        "b1",
        "e1",
        "Tiền điện",
        "u2",
        vec!["u1".into(), "u2".into(), "u3".into(), "u1".into()],
        "looks good",
    );
    assert_eq!(ev.event_type, "entry.comment_added");
    assert_eq!(ev.recipient_user_ids, vec!["u1", "u3"]);
    assert_eq!(ev.actor_user_id.as_deref(), Some("u2"));
    assert_eq!(ev.group_key.as_deref(), Some("entry_comment:e1"));
    assert_eq!(
        ev.link.as_deref(),
        Some("/budgets/b1?tab=transactions&tx=e1")
    );
    assert_eq!(ev.params["entry_description"], "Tiền điện");
    assert_eq!(ev.params["excerpt"], "looks good");
}

#[test]
fn commenting_on_your_own_entry_alone_notifies_nobody() {
    let ev = comment_added_event("b1", "e1", "", "u1", vec!["u1".into()], "note to self");
    assert!(!ev.has_recipients());
}

#[test]
fn excerpt_is_cut_on_a_character_boundary() {
    let long = "á".repeat(200);
    let e = excerpt(&long);
    assert_eq!(e.chars().count(), 81);
    assert!(e.ends_with('…'));
    assert_eq!(excerpt("  short  "), "short");
}

#[test]
fn publish_request_uses_the_group_key() {
    let ev = comment_added_event("b1", "e1", "", "u2", vec!["u1".into()], "hi");
    let r = to_publish_request(&ev);
    assert_eq!(r.group_key, "entry_comment:e1");
    assert_eq!(r.category, 1); // ACTIVITY
    assert_eq!(r.source, "entry");
}

#[test]
fn only_invalid_argument_is_permanent() {
    assert!(matches!(
        classify(tonic::Code::InvalidArgument, "x"),
        SinkError::Permanent(_)
    ));
    assert!(matches!(
        classify(tonic::Code::Unavailable, "x"),
        SinkError::Retryable(_)
    ));
}
