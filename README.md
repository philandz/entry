# entry

## Notifications (producer)

Publishes personal notifications to the `notification` service through a transactional outbox
(`entry_notify_outbox`, drained by `libs/event`). **Off by default**; set `NOTIFY_PUBLISH_ENABLED=true` together
with `NOTIFICATION_GRPC_URL` (e.g. `http://notification.philandz.svc.cluster.local:50109`) and
`INTERNAL_SERVICE_TOKEN`. Drainer tuning: `NOTIFY_OUTBOX_INTERVAL_SECS`, `_BATCH_SIZE`, `_MAX_ATTEMPTS`, `_RETENTION_SECS`.

Events: `entry.comment_added` to the entry creator and earlier commenters (minus the author); unread ones on the same entry collapse into one notification.

Test: `DATABASE_URL=… cargo test --test comment_notification_test -- --include-ignored`.

Enqueue happens after the main change has committed and is best-effort (failures are logged).
Check the outbox: `SELECT event_type, attempts, last_error FROM entry_notify_outbox WHERE delivered_at IS NULL;`
