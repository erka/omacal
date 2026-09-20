-- What has already been announced for a task (#137), so a restart does not
-- announce it again.
--
-- Keyed by the due as well as the task: moving a due is a new announcement,
-- and moving it back is not a second one within the same due. There is no
-- `minutes` half of the key as `fired_reminders` has — a task has one
-- announcement, at its due or at its own alarm's lead, never a list of them.
--
-- No end column either, and for the reason tasks differ from events: an
-- occurrence stops being worth announcing when it ends, while an overdue task
-- stays overdue until it is done. So a row is not pruned on a horizon (which
-- would re-announce a task left undone for a week) but when nothing answers
-- to it any more — see `prune_fired_tasks`.
CREATE TABLE fired_task_reminders (
  task_id     INTEGER NOT NULL,
  due_ms      INTEGER NOT NULL,
  fired_at_ms INTEGER NOT NULL,
  PRIMARY KEY (task_id, due_ms)
);

-- Deliberately no foreign key to `tasks`, exactly as `fired_reminders` has
-- none to `events`: a sync deleting the task between the read and this write
-- would turn a constraint violation into a failed notification pass, and the
-- prune collects the row anyway.
