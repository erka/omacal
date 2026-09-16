// The tasks sidebar's width (#130), and the bounds a drag is held to.
//
// Mirrors `settings::TASKS_WIDTH_*` on the Rust side, which clamps what is
// stored regardless of what the page asks for — the same split as `zoom.ts`
// and `HOUR_HEIGHT_*`. Pure, so `taskwidth.spec.ts` can pin the rule.

/** What the panel shipped as, and what a double-click on the edge restores. */
export const TASKS_WIDTH_DEFAULT = 288;
/** A task's own line: a checkbox, a title worth reading, a date chip. */
export const TASKS_WIDTH_MIN = 220;
/** Wide enough for long titles, narrow enough that the calendar stays the
 *  larger half of a laptop pane. */
export const TASKS_WIDTH_MAX = 560;

/** A dragged width, held to the range. A number that is not one at all —
 *  a `NaN` from an empty measurement — reads as the default rather than
 *  collapsing the panel. */
export const clampTasksWidth = (px: number): number =>
  Number.isFinite(px) ? Math.round(Math.min(Math.max(px, TASKS_WIDTH_MIN), TASKS_WIDTH_MAX)) : TASKS_WIDTH_DEFAULT;
