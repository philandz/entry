/// Recurring entry scheduler — runs as a background tokio task.
///
/// Every hour it queries `budget_entries` for rows where:
///   - `is_recurring = TRUE`
///   - `next_occurrence <= today (UTC+7)`
///
/// For each due entry it:
///   1. Creates a new entry with the same fields (but a fresh id + today's date)
///   2. Advances `next_occurrence` on the template entry using the RRULE
///
/// RRULE parsing is intentionally minimal for v1 — only FREQ + INTERVAL are
/// supported (DAILY / WEEKLY / MONTHLY / YEARLY).

use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info};
use chrono::Datelike;

use crate::converters::kind_from_db;
use crate::manager::repository::EntryRepository;

/// Parse a minimal RRULE and return the next date after `current`.
/// Supported: FREQ=DAILY|WEEKLY|MONTHLY|YEARLY with optional INTERVAL=N.
pub fn next_occurrence(current: &str, rrule: &str) -> Option<String> {
    use chrono::NaiveDate;
    let date = NaiveDate::parse_from_str(current, "%Y-%m-%d").ok()?;

    let mut freq = "";
    let mut interval: i64 = 1;

    for part in rrule.split(';') {
        let mut kv = part.splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some("FREQ"),     Some(v)) => freq = v,
            (Some("INTERVAL"), Some(v)) => interval = v.parse().unwrap_or(1),
            _ => {}
        }
    }

    let next = match freq {
        "DAILY"   => date + chrono::Duration::days(interval),
        "WEEKLY"  => date + chrono::Duration::weeks(interval),
        "MONTHLY" => {
            let months = date.month() as i64 + interval;
            let year   = date.year() + ((months - 1) / 12) as i32;
            let month  = ((months - 1) % 12 + 1) as u32;
            let day    = date.day().min(days_in_month(year, month));
            NaiveDate::from_ymd_opt(year, month, day)?
        }
        "YEARLY"  => {
            NaiveDate::from_ymd_opt(date.year() + interval as i32, date.month(), date.day())?
        }
        _ => return None,
    };

    Some(next.format("%Y-%m-%d").to_string())
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next_month = if month == 12 { chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1) }
                     else           { chrono::NaiveDate::from_ymd_opt(year, month + 1, 1) };
    next_month
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

pub async fn run(repo: Arc<EntryRepository>) {
    let mut ticker = interval(Duration::from_secs(3600)); // every hour
    loop {
        ticker.tick().await;
        if let Err(e) = tick(&repo).await {
            error!("Recurring scheduler error: {e}");
        }
    }
}

async fn tick(repo: &EntryRepository) -> anyhow::Result<()> {
    let today = {
        use chrono::FixedOffset;
        let tz = FixedOffset::east_opt(7 * 3600).unwrap();
        chrono::Utc::now().with_timezone(&tz).format("%Y-%m-%d").to_string()
    };

    let due = repo.list_due_recurring(&today).await?;
    if due.is_empty() { return Ok(()); }

    info!("Recurring scheduler: {} entries due on {today}", due.len());

    for template in due {
        let rule = match &template.recurrence_rule {
            Some(r) => r.clone(),
            None    => continue,
        };

        let kind = kind_from_db(&template.kind);
        let tags: Vec<String> = template.tags
            .as_deref()
            .unwrap_or("")
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string())
            .collect();

        // Create the new occurrence
        match repo.create_entry(
            &template.budget_id,
            template.category_id.as_deref(),
            kind,
            template.amount,
            &template.description,
            &today,
            &tags,
            template.notes.as_deref(),
            &template.created_by,
        ).await {
            Ok(new_entry) => {
                info!("Spawned recurring entry {} from template {}", new_entry.id, template.id);
            }
            Err(e) => {
                error!("Failed to spawn recurring entry from {}: {e}", template.id);
                continue;
            }
        }

        // Advance next_occurrence on the template
        if let Some(next) = next_occurrence(&today, &rule) {
            if let Err(e) = repo.advance_next_occurrence(&template.id, &next).await {
                error!("Failed to advance next_occurrence for {}: {e}", template.id);
            }
        } else {
            // Unrecognised RRULE — cancel recurrence to avoid infinite loop
            let _ = repo.update_recurrence_rule(&template.id, None).await;
        }
    }

    Ok(())
}
