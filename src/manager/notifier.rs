//! Fire-and-log access to the notify outbox (see budget's notifier for the rationale).

use philand_event::{MySqlOutbox, NotifyCategory, NotifyEvent, NotifySeverity};

const SOURCE: &str = "entry";
/// Longest comment excerpt put in the notification params.
const EXCERPT_CHARS: usize = 80;

#[derive(Clone)]
pub struct Notifier {
    outbox: MySqlOutbox,
    pool: sqlx::MySqlPool,
}

impl Notifier {
    pub fn new(outbox: MySqlOutbox, pool: sqlx::MySqlPool) -> Self {
        Self { outbox, pool }
    }

    /// Enqueue `event`; does nothing when it has no recipients.
    pub async fn send(&self, event: NotifyEvent) {
        if !event.has_recipients() {
            return;
        }
        let kind = event.event_type.clone();
        if let Err(e) = self.outbox.enqueue(&self.pool, &event).await {
            tracing::warn!("notify: could not enqueue {kind}: {e}");
        }
    }
}

/// First `EXCERPT_CHARS` characters of a comment, cut on a char boundary.
pub fn excerpt(body: &str) -> String {
    let trimmed = body.trim();
    let mut out: String = trimmed.chars().take(EXCERPT_CHARS).collect();
    if trimmed.chars().count() > EXCERPT_CHARS {
        out.push('…');
    }
    out
}

/// A comment was added to an entry. Recipients are the thread: the entry's creator and everyone
/// who commented before, minus the author. Unread comments on the same entry collapse into one
/// notification (`group_key`).
pub fn comment_added_event(
    budget_id: &str,
    entry_id: &str,
    author: &str,
    thread_members: Vec<String>,
    body: &str,
) -> NotifyEvent {
    NotifyEvent::new(
        SOURCE,
        "entry.comment_added",
        NotifyCategory::Activity,
        thread_members,
        "New comment on a transaction",
    )
    .severity(NotifySeverity::Info)
    .actor(author)
    .budget(budget_id)
    .link(format!("/budgets/{budget_id}"))
    .group_key(format!("entry_comment:{entry_id}"))
    .params(serde_json::json!({
        "entry_id": entry_id,
        "excerpt": excerpt(body),
    }))
    .without_actor()
}
