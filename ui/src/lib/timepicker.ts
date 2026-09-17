// The arithmetic behind `TimeField`'s list, in a `.ts` of its own for
// `datepicker.ts`'s reason: `tsconfig.test.json` compiles no `.svelte`, so
// anything a spec should pin directly cannot live in the component.
//
// Times here are stored `HH:MM` strings, the form every time field in the
// app keeps (`timefmt.ts`'s `parseClock` / `displayClock`). Never instants:
// a list of the hours of a day is the same list on every day.

const pad2 = (n: number) => String(n).padStart(2, '0');

/** Every `stepMin` minutes from 00:00, as `HH:MM`. `stepMin` must divide a
 *  day; 30 is what the field offers — a task is rarely due at a quarter
 *  past, and anything the list does not hold can be typed. */
export function timeSlots(stepMin = 30): string[] {
  const out: string[] = [];
  for (let m = 0; m < 24 * 60; m += stepMin) out.push(`${pad2(Math.floor(m / 60))}:${pad2(m % 60)}`);
  return out;
}

const minutesOf = (hhmm: string): number | null => {
  const m = /^(\d{2}):(\d{2})$/.exec(hhmm);
  return m ? Number(m[1]) * 60 + Number(m[2]) : null;
};

/** The slot at or just before `hhmm`, so 11:15 opens the list on 11:00 with
 *  11:30 right under it. `null` for something that is not a time. */
export function slotAtOrBefore(hhmm: string, slots: string[]): string | null {
  const at = minutesOf(hhmm);
  if (at === null || slots.length === 0) return null;
  let best = slots[0];
  for (const s of slots) {
    if ((minutesOf(s) ?? 0) <= at) best = s;
  }
  return best;
}

/**
 * Where the list opens, so the likely answer is already in view.
 *
 * - A field with a time opens on it.
 * - An empty field for **today** opens on the next slot after now: a task
 *   due today is due later today, and 00:00 at the top of the list is an
 *   answer to a question nobody asked. After the last slot, the last slot.
 * - An empty field for any other day, or none yet, opens on 09:00 — the
 *   start of the day most people mean by "that day".
 */
export function openingSlot(
  value: string,
  slots: string[],
  { nowMs, isToday }: { nowMs: number; isToday: boolean },
): string {
  const own = value ? slotAtOrBefore(value, slots) : null;
  if (own) return own;
  if (isToday) {
    const d = new Date(nowMs);
    const now = d.getHours() * 60 + d.getMinutes();
    return slots.find((s) => (minutesOf(s) ?? 0) > now) ?? slots[slots.length - 1];
  }
  return slots.includes('09:00') ? '09:00' : slots[0];
}

/** How long an end at `end` makes a span starting at `start`, as the End
 *  list says it beside each time ("30 min", "1 h", "1.5 h", "2 h 15 min").
 *  `null` for an end at or before the start, which the list leaves bare. */
export function durationNote(start: string, end: string): string | null {
  const a = minutesOf(start);
  const b = minutesOf(end);
  if (a === null || b === null || b <= a) return null;
  const d = b - a;
  if (d < 60) return `${d} min`;
  const h = Math.floor(d / 60);
  const m = d % 60;
  if (m === 0) return `${h} h`;
  if (m === 30) return `${h}.5 h`;
  return `${h} h ${m} min`;
}

/** The highlight moved `by` rows, held to the list's ends rather than
 *  wrapping: a hand holding ArrowDown past 23:30 means "the last one", not
 *  "midnight again". From nowhere, a step lands on `from ?? first`. */
export function stepSlot(slots: string[], from: string | null, by: number): string {
  const i = from === null ? -1 : slots.indexOf(from);
  if (i < 0) return slots[0];
  return slots[Math.max(0, Math.min(slots.length - 1, i + by))];
}
