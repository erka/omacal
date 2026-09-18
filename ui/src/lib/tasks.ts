import { invoke } from '@tauri-apps/api/core';

/** One task, as `tasks::list_tasks` shapes it. */
export type Task = {
  id: number;
  calendarId: number;
  summary: string;
  notes: string | null;
  dueMs: number | null;
  dueAllDay: boolean;
  completed: boolean;
  /** When it was completed, or `null` when the resource does not say — a
   *  server may set a task's status without stamping the time. */
  completedMs: number | null;
  calendar: string;
  color: string | null;
  priority: number;
  /** False on read-only lists and in demo mode — the checkbox renders
   *  disabled rather than pretending. */
  canWrite: boolean;
};

/** One list the quick-add can land on. */
export type TaskList = {
  calendarId: number;
  name: string;
  color: string | null;
  /** Kept on this machine: the lists the Tasks pane can rename and delete. */
  local: boolean;
};

export const listTasks = () => invoke<Task[]>('list_tasks');

/** A page of completed tasks, newest first: `list_tasks` carries only the
 *  last week's, and the Done list's "earlier" asks for the rest. */
export type DonePage = { tasks: Task[]; more: boolean };

/** Completed tasks from before `beforeMs` whose title or note has every
 *  word of `query`, case folded (an empty query is all of them). */
export const searchDoneTasks = (query: string, beforeMs: number | null, offset: number, limit: number) =>
  invoke<DonePage>('search_done_tasks', { query, beforeMs, offset, limit });
export const taskLists = () => invoke<TaskList[]>('task_lists');

/** Makes the on-this-device list, or finds the one already there, and
 *  answers with the lists as the pickers see them. A Google-only install has
 *  no task list and no way to make one: Google keeps tasks in a different
 *  product with a different API. */
export const createLocalTaskList = () => invoke<TaskList[]>('create_local_task_list');

/** Another list on this device, named; answers with the lists, it among
 *  them. Refused for a blank name or one another list already has. */
export const createTaskList = (name: string) => invoke<TaskList[]>('create_task_list', { name });

/** Renames a list on this device. */
export const renameTaskList = (id: number, name: string) =>
  invoke<TaskList[]>('rename_task_list', { id, name });

/** Deletes a list on this device, and every task on it. */
export const deleteTaskList = (id: number) => invoke<TaskList[]>('delete_task_list', { id });

/** Completes (or reopens) a task — the server first, then the store, which is
 *  why the fresh list comes back from the same call. */
export const setTaskCompleted = (id: number, on: boolean) =>
  invoke<Task[]>('set_task_completed', { id, on });

/** A new task on a list. `dueAllDay` is `updateTask`'s: a date, or the hour
 *  `dueMs` names on it. */
export const createTask = (calendarId: number, summary: string, dueMs: number | null, dueAllDay = true) =>
  invoke<Task[]>('create_task', { calendarId, summary, dueMs, dueAllDay });

export const deleteTask = (id: number) => invoke<Task[]>('delete_task_cmd', { id });

/** Edits a task whole: title, due date and note in one write.
 *
 *  Every field is the complete answer rather than a change to apply, so a
 *  `null` due date clears it. `dueAllDay` is the difference between "by
 *  Thursday" and "by Thursday at 18:00", which is the user's distinction
 *  and not a storage detail — the backend spells the two differently on the
 *  wire. `calendarId` moves the task to that list in the same save; `null`
 *  leaves it where it is. */
export const updateTask = (
  id: number,
  summary: string,
  dueMs: number | null,
  dueAllDay: boolean,
  notes: string | null,
  calendarId: number | null = null,
) => invoke<Task[]>('update_task', { id, summary, dueMs, dueAllDay, notes, calendarId });

/** Connects an iCloud or generic CalDAV account. Resolves to the account's
 *  display email once discovery has accepted the credentials. */
export const connectCaldav = (args: {
  kind: 'icloud' | 'caldav';
  serverUrl?: string;
  email: string;
  username?: string;
  password: string;
}) =>
  invoke<string>('connect_caldav', {
    kind: args.kind,
    serverUrl: args.serverUrl ?? null,
    email: args.email,
    username: args.username ?? null,
    password: args.password,
  });
