-- conferenceData's video entry point is now imported, but unchanged Google
-- events cached before that fix are never delivered by incremental sync.
-- Refetch once so existing Zoom/Teams/Webex meetings get their Join links.
-- Keep the cached events available offline; only discard Google's cursors.
-- CalDAV uses the same table for ctags and does not need this backfill.
DELETE FROM sync_state WHERE calendar_id IN (
    SELECT c.id FROM calendars c JOIN accounts a ON a.id = c.account_id
    WHERE a.provider = 'google'
);
