// The hour grid's overlap layout, on this side of the bridge as well.
//
// `omacal_core::lay_out_day` places a day's events, and its answer arrives
// with the week as `DayColumn.placed`. A task with an hour is drawn among
// those events (2026-09-17), and tasks are the page's own state rather than
// part of the week payload — so for a day that has one, the layout that must
// include it runs here. A port, not a second opinion: the golden file
// `ui/tests/generated/day-layout.json` is written by the Rust function itself,
// and `daylayout.spec.ts` holds this one to every case in it.

import type { Placed } from './api';

/** A half-open span, `[startMs, endMs)` — `layout::Interval`. */
export type Interval = { startMs: number; endMs: number };

/** `layout::MIN_HEIGHT`: a zero-length span is still something to click. */
const MIN_HEIGHT = 0.004;

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

/**
 * Overlapping spans side by side, as fractions of `[dayStartMs, dayEndMs)`.
 *
 * Spans connected by overlap form a *cluster*, and every member of a cluster
 * is drawn at the cluster's full column count so that the edges line up —
 * which is what makes a collision read as one. Sorted by start and then
 * longest first, so the big block takes column 0. The result is in input
 * order: `placed[i].idx === i`.
 */
export function layOutDay(spans: Interval[], dayStartMs: number, dayEndMs: number): Placed[] {
  if (spans.length === 0) return [];

  const order = spans.map((_, i) => i).sort((a, b) =>
    spans[a].startMs - spans[b].startMs
    || (spans[b].endMs - spans[b].startMs) - (spans[a].endMs - spans[a].startMs)
    || a - b);

  const columnOf = spans.map(() => 0);
  const columnsOf = spans.map(() => 1);
  // When each column's current occupant ends.
  let active: number[] = [];
  let cluster: number[] = [];
  let clusterEnd = -Infinity;

  for (const i of order) {
    const span = spans[i];
    // A cluster ends when a span starts at or after every active one's end.
    if (cluster.length > 0 && span.startMs >= clusterEnd) {
      for (const c of cluster) columnsOf[c] = active.length;
      cluster = [];
      active = [];
      clusterEnd = -Infinity;
    }
    // The first column whose occupant has already finished.
    let col = active.findIndex((end) => end <= span.startMs);
    if (col < 0) {
      active.push(span.endMs);
      col = active.length - 1;
    } else {
      active[col] = span.endMs;
    }
    columnOf[i] = col;
    cluster.push(i);
    clusterEnd = Math.max(clusterEnd, span.endMs);
  }
  for (const c of cluster) columnsOf[c] = active.length;

  const window = Math.max(dayEndMs - dayStartMs, 1);
  return spans.map((span, idx) => {
    const start = clamp(span.startMs, dayStartMs, dayEndMs);
    const end = clamp(span.endMs, start, dayEndMs);
    return {
      idx,
      column: columnOf[idx],
      columns: columnsOf[idx],
      top: (start - dayStartMs) / window,
      height: Math.max((end - start) / window, MIN_HEIGHT),
    };
  });
}
