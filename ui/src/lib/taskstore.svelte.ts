// The tasks as the whole window reads them: one copy, for the grid and the
// sidebar alike.
//
// A module-level rune for `clock.svelte.ts`'s reason — the readers sit in
// different subtrees, `App` (which draws them on the week) and
// `TasksSidebar` (which lists them) — and for a reason of its own. The two
// used to hold a copy each, and ticking a task off in the grid left it open
// in the sidebar until the sidebar was closed and opened again (reported
// 2026-09-17). Every write command already answers with the fresh list, so
// whoever writes hands that list here, and every reader redraws.

import { listTasks, taskLists, type Task, type TaskList } from './tasks';

const state = $state<{ rows: Task[] | null; lists: TaskList[]; revision: number }>({
  rows: null,
  lists: [],
  revision: 0,
});

/** The tasks, or `null` before the first load answered. */
export const taskRows = () => state.rows;

/** The writable lists a task can be put on. */
export const taskListRows = () => state.lists;

/** Bumped on every change to the rows. Something that shows tasks the rows
 *  do not carry — the Done list's earlier history, fetched on its own —
 *  reads it to know when to ask again. */
export const taskRevision = () => state.revision;

/** A write's answer: the fresh list, whoever made the write. */
export function setTaskRows(rows: Task[]) {
  state.rows = rows;
  state.revision += 1;
}

export function setTaskLists(lists: TaskList[]) {
  state.lists = lists;
}

/** Reads both again. Throws what the backend said, so a caller with
 *  somewhere to show it can; the grid has nowhere and ignores it. */
export async function refreshTasks(): Promise<void> {
  const [rows, lists] = await Promise.all([listTasks(), taskLists()]);
  state.lists = lists;
  setTaskRows(rows);
}
