//! The tasks commands — VTODO's face toward the UI.
//!
//! Reads come straight off the store; writes follow the calendar rule this
//! codebase lives by: **the server first, the local row after**, so the app
//! never shows a state the server refused. A completion toggle is a
//! line-surgery rewrite of the task's own resource (`ics::patch_todo_status`)
//! guarded by its etag; a create is a fresh single-VTODO resource guarded by
//! `If-None-Match: *`. Both go through the same client the sync loop uses.

use serde::Serialize;

use crate::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskVm {
    pub id: i64,
    pub calendar_id: i64,
    pub summary: String,
    pub notes: Option<String>,
    pub due_ms: Option<i64>,
    pub due_all_day: bool,
    pub completed: bool,
    pub calendar: String,
    pub color: Option<String>,
    pub priority: i64,
    /// False on read-only lists and in demo mode; the checkbox renders
    /// disabled rather than pretending.
    pub can_write: bool,
}

/// How far back completed tasks stay visible: a week, matching the feed's
/// notion of "recent enough to still matter".
const DONE_WINDOW_MS: i64 = 7 * 24 * 3_600_000;

fn to_vm(row: &omacal_store::TaskRow, demo: bool) -> TaskVm {
    TaskVm {
        id: row.task.id,
        calendar_id: row.task.calendar_id,
        summary: row.task.summary.clone().unwrap_or_else(|| "(untitled)".into()),
        notes: row.task.description.clone(),
        due_ms: row.task.due_utc,
        due_all_day: row.task.due_all_day,
        completed: row.task.status == "completed",
        calendar: row.calendar_summary.clone(),
        color: row.color_hex.clone(),
        priority: row.task.priority,
        can_write: !demo && row.access_role != "reader",
    }
}

#[tauri::command]
pub async fn list_tasks(state: tauri::State<'_, AppState>) -> Result<Vec<TaskVm>, String> {
    let since = crate::now_ms() - DONE_WINDOW_MS;
    let rows = omacal_store::tasks_for_ui(&state.pool, since)
        .await
        .map_err(|e| crate::errors::user_facing(&e))?;
    Ok(rows.iter().map(|r| to_vm(r, state.demo)).collect())
}

/// The account credentials behind one task's calendar — the shared
/// per-calendar helper, with a task-flavoured wrapper name kept for the
/// call sites below.
async fn client_for_task_calendar(
    state: &AppState,
    calendar_id: i64,
) -> anyhow::Result<(omacal_caldav::CalDavClient, String, String)> {
    let (client, collection_url) =
        crate::caldav_account::client_for_calendar(state, calendar_id).await?;
    Ok((client, collection_url, String::new()))
}

/// Where a task list lives.
///
/// A Google account brings no tasks, so an install with only Google had a
/// pane that could never hold one (Plamen, 2026-09-16). An **on-this-device**
/// list is the answer: the same rows, the same pane, the same CLI, with
/// nothing on the other end. Every write below asks this first, and the only
/// difference it makes is whether a `PUT` happens — the iCalendar text is
/// written either way, so a list that later moves to a server moves as a
/// copy rather than a rewrite.
enum TaskHome {
    /// A CalDAV collection: its client and the collection's URL.
    Server(Box<omacal_caldav::CalDavClient>, String),
    /// This machine.
    Device,
}

async fn task_home(state: &AppState, calendar_id: i64) -> anyhow::Result<TaskHome> {
    if omacal_store::is_local_calendar(&state.pool, calendar_id).await? {
        return Ok(TaskHome::Device);
    }
    let (client, collection_url, _) = client_for_task_calendar(state, calendar_id).await?;
    Ok(TaskHome::Server(Box::new(client), collection_url))
}

const TASK_CHANGED_ON_SERVER: &str =
    "That task changed on the server since it was loaded — sync and try again";

#[tauri::command]
pub async fn set_task_completed(
    state: tauri::State<'_, AppState>,
    id: i64,
    on: bool,
) -> Result<Vec<TaskVm>, String> {
    crate::demo_sync_guard(state.demo)?;
    set_completed_impl(&state, id, on).await.map_err(|e| e.to_string())?;
    list_tasks(state).await
}

async fn set_completed_impl(state: &AppState, id: i64, on: bool) -> anyhow::Result<()> {
    let task = omacal_store::task_by_id(&state.pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!(TASK_GONE))?;
    let raw = task.raw_ics.as_deref().ok_or_else(|| anyhow::anyhow!("task has no resource"))?;
    let home = task_home(state, task.calendar_id).await?;

    let now = jiff::Timestamp::from_millisecond(crate::now_ms())?;
    let patched = omacal_caldav::patch_todo_status(raw, &task.uid, on, now)
        .ok_or_else(|| anyhow::anyhow!("could not rewrite the task's resource"))?;

    let new_etag = match &home {
        TaskHome::Device => None,
        TaskHome::Server(client, _) => {
            let href = task.caldav_href.as_deref()
                .ok_or_else(|| anyhow::anyhow!("task has no href"))?;
            client
                .put(href, &patched, task.etag.as_deref())
                .await
                .map_err(|e| match e {
                    omacal_caldav::CalDavError::PreconditionFailed => {
                        anyhow::anyhow!(TASK_CHANGED_ON_SERVER)
                    }
                    other => anyhow::Error::from(other),
                })?
        }
    };

    let now_ms = crate::now_ms();
    omacal_store::mark_task_status(
        &state.pool,
        id,
        if on { "completed" } else { "needs-action" },
        on.then_some(now_ms),
        new_etag.as_deref(),
        Some(&patched),
        now_ms,
    )
    .await?;
    crate::upcoming::refresh_soon(state.pool.clone(), state.demo);
    Ok(())
}

/// Edits a task: its title, its due date and its note, in one write.
///
/// Every field is the whole answer rather than a change to apply — the
/// editor knows the complete state, so there is no "leave this alone" to
/// get wrong, and clearing a due date is saying `None` rather than
/// omitting it.
///
/// `due_all_day` is the difference between "by Thursday" and "by Thursday
/// at 18:00", and it is the user's distinction, not a storage detail: a
/// date-only due goes on the wire as `VALUE=DATE` and an instant as a UTC
/// stamp. A due date resolves against the *calendar's* zone, the same one
/// the sync reads it back in, so a task does not move a day when the
/// display zone differs.
#[tauri::command]
pub async fn update_task(
    state: tauri::State<'_, AppState>,
    id: i64,
    summary: String,
    due_ms: Option<i64>,
    due_all_day: bool,
    notes: Option<String>,
) -> Result<Vec<TaskVm>, String> {
    crate::demo_sync_guard(state.demo)?;
    update_impl(&state, id, &summary, due_ms, due_all_day, notes.as_deref())
        .await
        .map_err(|e| crate::errors::user_facing(&e))?;
    list_tasks(state).await
}

async fn update_impl(
    state: &AppState,
    id: i64,
    summary: &str,
    due_ms: Option<i64>,
    due_all_day: bool,
    notes: Option<&str>,
) -> anyhow::Result<()> {
    let summary = summary.trim();
    if summary.is_empty() {
        anyhow::bail!(TASK_NEEDS_A_TITLE);
    }
    let notes = notes.map(str::trim).filter(|n| !n.is_empty());

    let task = omacal_store::task_by_id(&state.pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!(TASK_GONE))?;
    let raw = task.raw_ics.as_deref().ok_or_else(|| anyhow::anyhow!(TASK_GONE))?;
    let home = task_home(state, task.calendar_id).await?;
    let cal_tz: String = sqlx::query_scalar("SELECT timezone FROM calendars WHERE id = ?1")
        .bind(task.calendar_id)
        .fetch_one(&state.pool)
        .await?;

    let due = due_for(due_ms, due_all_day, &cal_tz)?;
    let now = jiff::Timestamp::from_millisecond(crate::now_ms())?;
    let edit = omacal_caldav::TodoEdit { summary, due, description: notes };
    let patched = omacal_caldav::patch_todo_fields(raw, &task.uid, &edit, &cal_tz, now)
        .ok_or_else(|| anyhow::anyhow!("could not rewrite the task's resource"))?;

    let new_etag = match &home {
        TaskHome::Device => None,
        TaskHome::Server(client, _) => {
            let href = task.caldav_href.as_deref().ok_or_else(|| anyhow::anyhow!(TASK_GONE))?;
            client
                .put(href, &patched, task.etag.as_deref())
                .await
                .map_err(|e| match e {
                    omacal_caldav::CalDavError::PreconditionFailed => {
                        anyhow::anyhow!(TASK_CHANGED_ON_SERVER)
                    }
                    other => anyhow::Error::from(other),
                })?
        }
    };

    omacal_store::update_task_fields(
        &state.pool,
        id,
        summary,
        notes,
        due_ms,
        due.is_some().then_some(cal_tz.as_str()),
        due_all_day,
        new_etag.as_deref(),
        &patched,
        crate::now_ms(),
    )
    .await?;
    crate::upcoming::refresh_soon(state.pool.clone(), state.demo);
    Ok(())
}

/// The wire's `(ms, all_day)` pair as iCalendar spells it.
///
/// Split out and pure so the one thing worth checking — that an all-day due
/// takes its date in the *calendar's* zone rather than UTC — is checkable
/// without a server. In Kolkata a task due at 00:30 local is 19:00 UTC the
/// day before, and reading the UTC date would file it a day early: the same
/// class of bug as #44.
pub(crate) fn due_for(
    due_ms: Option<i64>,
    all_day: bool,
    cal_tz: &str,
) -> anyhow::Result<Option<omacal_caldav::TodoDue>> {
    let Some(ms) = due_ms else { return Ok(None) };
    let ts = jiff::Timestamp::from_millisecond(ms)?;
    if !all_day {
        return Ok(Some(omacal_caldav::TodoDue::At(ts)));
    }
    let tz = jiff::tz::TimeZone::get(cal_tz).unwrap_or(jiff::tz::TimeZone::UTC);
    Ok(Some(omacal_caldav::TodoDue::Date(ts.to_zoned(tz).date())))
}

/// The list a task lands on when nobody said: the first one a task can be
/// created on, which is what `task_lists` already means.
pub(crate) async fn first_writable_list(pool: &sqlx::SqlitePool) -> Option<i64> {
    writable_task_lists(pool).await.ok()?.first().map(|l| l.calendar_id)
}

/// The socket's three task verbs, sharing the window's own write path —
/// one code path, the same guards, as the events side does.
pub(crate) async fn create_body(
    state: &AppState,
    calendar_id: i64,
    summary: &str,
    due_ms: Option<i64>,
    due_all_day: bool,
) -> Result<Vec<TaskVm>, String> {
    crate::demo_sync_guard(state.demo)?;
    create_impl(state, calendar_id, summary, due_ms, due_all_day)
        .await
        .map_err(|e| crate::errors::user_facing(&e))?;
    Ok(list_body(state).await)
}

pub(crate) async fn complete_body(state: &AppState, id: i64, done: bool) -> Result<Vec<TaskVm>, String> {
    crate::demo_sync_guard(state.demo)?;
    set_completed_impl(state, id, done)
        .await
        .map_err(|e| crate::errors::user_facing(&e))?;
    Ok(list_body(state).await)
}

pub(crate) async fn update_body(
    state: &AppState,
    id: i64,
    summary: &str,
    due_ms: Option<i64>,
    due_all_day: bool,
    notes: Option<&str>,
) -> Result<Vec<TaskVm>, String> {
    crate::demo_sync_guard(state.demo)?;
    update_impl(state, id, summary, due_ms, due_all_day, notes)
        .await
        .map_err(|e| crate::errors::user_facing(&e))?;
    Ok(list_body(state).await)
}

/// `list_tasks` without the Tauri wrapper, for the socket.
pub(crate) async fn list_body(state: &AppState) -> Vec<TaskVm> {
    let since = crate::now_ms() - DONE_WINDOW_MS;
    omacal_store::tasks_for_ui(&state.pool, since)
        .await
        .map(|rows| rows.iter().map(|r| to_vm(r, state.demo)).collect())
        .unwrap_or_default()
}

pub(crate) const TASK_NEEDS_A_TITLE: &str = "a task needs a title";
pub(crate) const TASK_GONE: &str = "that task is no longer here";

#[tauri::command]
pub async fn create_task(
    state: tauri::State<'_, AppState>,
    calendar_id: i64,
    summary: String,
    due_ms: Option<i64>,
) -> Result<Vec<TaskVm>, String> {
    crate::demo_sync_guard(state.demo)?;
    create_impl(&state, calendar_id, &summary, due_ms, true).await.map_err(|e| e.to_string())?;
    list_tasks(state).await
}

async fn create_impl(
    state: &AppState,
    calendar_id: i64,
    summary: &str,
    due_ms: Option<i64>,
    all_day: bool,
) -> anyhow::Result<()> {
    let summary = summary.trim();
    if summary.is_empty() {
        anyhow::bail!(TASK_NEEDS_A_TITLE);
    }
    let home = task_home(state, calendar_id).await?;
    let cal_tz: String = sqlx::query_scalar("SELECT timezone FROM calendars WHERE id = ?1")
        .bind(calendar_id)
        .fetch_one(&state.pool)
        .await?;

    let uid = uuid::Uuid::new_v4().to_string();
    let now = jiff::Timestamp::from_millisecond(crate::now_ms())?;
    // The window's quick-add says dates, not instants: "by Friday", not
    // "by 16:23:07". The CLI can say an hour, and then it means one.
    let due_time = due_ms.and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok()).map(|ts| {
        let tz = jiff::tz::TimeZone::get(&cal_tz).unwrap_or(jiff::tz::TimeZone::UTC);
        let z = ts.to_zoned(tz);
        if all_day {
            omacal_caldav::IcsTime::Date(z.date())
        } else {
            // In the calendar's zone, not as a bare UTC instant (issue
            // #102): `cal_tz` was already being handed to `new_todo_ics`
            // and ignored there, which is the shape of a wire that was
            // meant to be connected and never was.
            omacal_caldav::IcsTime::Zoned { dt: z.datetime(), tzid: cal_tz.clone() }
        }
    });
    let ics = omacal_caldav::new_todo_ics(&uid, summary, due_time.as_ref(), now);

    // On this device there is no resource to address, so the row carries no
    // href — which is also what every write above reads it as.
    let (href, new_etag) = match &home {
        TaskHome::Device => (None, None),
        TaskHome::Server(client, collection_url) => {
            let href = format!("{}/{uid}.ics", collection_url.trim_end_matches('/'));
            let etag = client.put(&href, &ics, None).await.map_err(anyhow::Error::from)?;
            (Some(href), etag)
        }
    };

    let now_ms = crate::now_ms();
    let due = due_time.as_ref().and_then(|t| omacal_caldav::resolve(t, &cal_tz));
    omacal_store::upsert_task(
        &state.pool,
        &omacal_store::StoredTask {
            id: 0,
            calendar_id,
            uid,
            etag: new_etag,
            caldav_href: href,
            summary: Some(summary.to_string()),
            description: None,
            due_utc: due.as_ref().map(|(ms, _, _)| *ms),
            due_tz: due.as_ref().map(|(_, tz, _)| tz.clone()),
            due_all_day: due.is_some(),
            status: "needs-action".into(),
            completed_utc: None,
            priority: 0,
            updated_at: now_ms,
            raw_ics: Some(ics),
        },
    )
    .await?;
    crate::upcoming::refresh_soon(state.pool.clone(), state.demo);
    Ok(())
}

#[tauri::command]
pub async fn delete_task_cmd(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<Vec<TaskVm>, String> {
    crate::demo_sync_guard(state.demo)?;
    delete_impl(&state, id).await.map_err(|e| e.to_string())?;
    list_tasks(state).await
}

async fn delete_impl(state: &AppState, id: i64) -> anyhow::Result<()> {
    let task = omacal_store::task_by_id(&state.pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("that task is no longer here"))?;
    if let TaskHome::Server(client, _) = task_home(state, task.calendar_id).await? {
        let href = task.caldav_href.as_deref()
            .ok_or_else(|| anyhow::anyhow!("task has no href"))?;
        client.delete(href, task.etag.as_deref()).await.map_err(anyhow::Error::from)?;
    }
    omacal_store::delete_task(&state.pool, id).await?;
    crate::upcoming::refresh_soon(state.pool.clone(), state.demo);
    Ok(())
}

/// The task-capable, writable lists the quick-add can land on.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListVm {
    pub calendar_id: i64,
    pub name: String,
    pub color: Option<String>,
}

/// The task-capable, writable lists, in the order the picker offers them.
/// One query, two callers: the window's command and the socket's "no list
/// was named" default, so the CLI can never land a task somewhere the
/// window would not offer.
pub(crate) async fn writable_task_lists(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<TaskListVm>> {
    let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT c.id, COALESCE(c.label_override, c.summary), COALESCE(c.color_override, c.color_hex)
         FROM calendars c JOIN accounts a ON a.id = c.account_id
         WHERE a.provider IN ('caldav', 'local') AND c.supports_tasks = 1
           AND c.selected = 1 AND c.access_role != 'reader'
         ORDER BY COALESCE(c.label_override, c.summary) COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(calendar_id, name, color)| TaskListVm { calendar_id, name, color })
        .collect())
}

/// Creates the on-this-device task list, or finds the one already there,
/// and answers with the lists as the pickers see them.
///
/// The button behind this exists because a Google-only install has no task
/// list at all and no way to make one: Google keeps tasks in another product
/// with another API, and asking for a CalDAV account to write a shopping
/// list is asking for a server nobody wanted (Plamen, 2026-09-16).
///
/// The list's zone is the display zone, which is what a due date means here:
/// "by Thursday" is Thursday where the user is.
#[tauri::command]
pub async fn create_local_task_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<TaskListVm>, String> {
    crate::demo_sync_guard(state.demo)?;
    let tz = crate::settings::read_settings(&state.pool)
        .await
        .display_timezone
        .unwrap_or_else(|| jiff::tz::TimeZone::system().iana_name().unwrap_or("UTC").to_string());
    omacal_store::ensure_local_task_list(&state.pool, LOCAL_TASK_LIST_NAME, &tz, crate::now_ms())
        .await
        .map_err(|e| crate::errors::user_facing(&e))?;
    writable_task_lists(&state.pool)
        .await
        .map_err(|e| crate::errors::user_facing(&e))
}

/// What the on-this-device list is called. Plain, because the pane already
/// says these are tasks and the account row beside it says where they live.
pub(crate) const LOCAL_TASK_LIST_NAME: &str = "Tasks on this device";

#[tauri::command]
pub async fn task_lists(state: tauri::State<'_, AppState>) -> Result<Vec<TaskListVm>, String> {
    writable_task_lists(&state.pool)
        .await
        .map_err(|e| crate::errors::user_facing(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_state(pool: sqlx::SqlitePool) -> AppState {
        AppState {
            pool,
            demo: false,
            tokens: Default::default(),
            reauth: Default::default(),
            update: Default::default(),
            update_checked_at: Default::default(),
            system_tz_change: Default::default(),
            quit_on_close: Default::default(),
            open_date: Default::default(),
        }
    }

    /// **A task list with no server behind it** (Plamen, 2026-09-16): the
    /// whole life of one, through the same commands a CalDAV list uses. If
    /// any of them reached for a client this would fail here rather than in
    /// somebody's pane — there is no server, no credential and no keyring
    /// entry anywhere in this test.
    #[tokio::test]
    async fn a_task_on_this_device_is_created_edited_completed_and_deleted_without_a_server() {
        let pool = omacal_store::connect_memory().await.unwrap();
        let list =
            omacal_store::ensure_local_task_list(&pool, "Tasks on this device", "Europe/Sofia", 0)
                .await
                .unwrap();
        let state = local_state(pool.clone());

        create_impl(&state, list, "Water the plants", None, true).await.unwrap();
        let rows = omacal_store::tasks_for_ui(&pool, 0).await.unwrap();
        assert_eq!(rows.len(), 1, "the pane shows it, like any other list's task");
        let task = &rows[0].task;
        let id = task.id;
        assert_eq!(task.summary.as_deref(), Some("Water the plants"));
        // No resource to address, and the iCalendar text kept anyway — which
        // is what lets this list move to a server later as a copy.
        assert!(task.caldav_href.is_none(), "nothing to address on this device");
        assert!(task.etag.is_none());
        assert!(task.raw_ics.as_deref().is_some_and(|r| r.contains("BEGIN:VTODO")));

        // A due date, a note and a new title, in one write.
        let due: i64 = "2026-09-18T00:00:00+03:00".parse::<jiff::Timestamp>().unwrap().as_millisecond();
        update_impl(&state, id, "Water the plants twice", Some(due), true, Some("the big one"))
            .await
            .unwrap();
        let t = omacal_store::task_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(t.summary.as_deref(), Some("Water the plants twice"));
        assert_eq!(t.description.as_deref(), Some("the big one"));
        assert!(t.due_utc.is_some() && t.due_all_day);

        set_completed_impl(&state, id, true).await.unwrap();
        let t = omacal_store::task_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(t.status, "completed");
        assert!(t.completed_utc.is_some());
        set_completed_impl(&state, id, false).await.unwrap();
        assert_eq!(
            omacal_store::task_by_id(&pool, id).await.unwrap().unwrap().status,
            "needs-action"
        );

        delete_impl(&state, id).await.unwrap();
        assert!(omacal_store::task_by_id(&pool, id).await.unwrap().is_none());
    }

    /// The list is offered to the pickers, and making it twice makes one.
    #[tokio::test]
    async fn the_on_device_list_is_offered_once_however_often_it_is_asked_for() {
        let pool = omacal_store::connect_memory().await.unwrap();
        let first =
            omacal_store::ensure_local_task_list(&pool, LOCAL_TASK_LIST_NAME, "UTC", 0).await.unwrap();
        let again =
            omacal_store::ensure_local_task_list(&pool, LOCAL_TASK_LIST_NAME, "UTC", 1).await.unwrap();
        assert_eq!(first, again, "one list, not two");

        let lists = writable_task_lists(&pool).await.unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].calendar_id, first);
        assert_eq!(lists[0].name, LOCAL_TASK_LIST_NAME);
        assert!(omacal_store::is_local_calendar(&pool, first).await.unwrap());
    }

    /// An all-day due date takes its date in the **calendar's** zone.
    ///
    /// The case that matters is the one that bit #44: in Kolkata a task due
    /// at 00:30 local is 19:00 UTC the day before, so reading the UTC date
    /// would file it a day early. A timed due is the instant itself and has
    /// no such question.
    #[test]
    fn an_all_day_due_takes_the_calendars_own_date() {
        // 2026-09-11T00:30 in Asia/Kolkata is 2026-09-10T19:00Z.
        let ms: i64 = "2026-09-10T19:00:00Z".parse::<jiff::Timestamp>().unwrap().as_millisecond();

        let kolkata = due_for(Some(ms), true, "Asia/Kolkata").unwrap();
        assert_eq!(kolkata, Some(omacal_caldav::TodoDue::Date(jiff::civil::date(2026, 9, 11))));
        let utc = due_for(Some(ms), true, "UTC").unwrap();
        assert_eq!(utc, Some(omacal_caldav::TodoDue::Date(jiff::civil::date(2026, 9, 10))));

        // A timed due is the instant, whatever the calendar's zone.
        let at = due_for(Some(ms), false, "Asia/Kolkata").unwrap();
        assert_eq!(at, Some(omacal_caldav::TodoDue::At(jiff::Timestamp::from_millisecond(ms).unwrap())));

        // No date is no date, not an instant at zero.
        assert_eq!(due_for(None, true, "Asia/Kolkata").unwrap(), None);
        assert_eq!(due_for(None, false, "UTC").unwrap(), None);

        // A zone the machine does not know falls back rather than refusing:
        // a task filed on the wrong day beats a task that cannot be saved.
        assert!(due_for(Some(ms), true, "Mars/Olympus").unwrap().is_some());
    }
}
