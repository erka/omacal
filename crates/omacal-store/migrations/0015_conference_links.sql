-- conferenceData's video entry point has been imported since v0.17.0, but an
-- event cached before that and unchanged since is never re-delivered by an
-- incremental sync, so its Join link stays missing. Dropping Google's cursors
-- makes the next sync a full window fetch per Google calendar, which is the
-- only way those rows acquire it. Costs one slow sync on first launch after
-- the update. The cached events stay, available offline until the refetch
-- lands, and CalDAV, whose ctags share this table, needs no backfill.
DELETE FROM sync_state WHERE calendar_id IN (
    SELECT c.id FROM calendars c JOIN accounts a ON a.id = c.account_id
    WHERE a.provider = 'google'
);
