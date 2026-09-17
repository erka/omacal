import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { layOutDay } from '../src/lib/daylayout';
import type { Placed } from '../src/lib/api';

/**
 * The UI's port of `omacal_core::lay_out_day`, held to the Rust function on
 * every case in the golden file that function writes
 * (`commands::tests::the_day_layout_golden_file_is_what_lay_out_day_produces`).
 * The grid runs the port only for a day holding a task at an hour, so a
 * divergence would show as a day whose meetings rearranged themselves the
 * moment a task landed beside them.
 */
type Case = { name: string; day_start_ms: number; day_end_ms: number; spans: [number, number][]; placed: Placed[] };
const cases: Case[] = JSON.parse(
  readFileSync(new URL('./generated/day-layout.json', import.meta.url), 'utf8'),
);

test('the golden file has the cases it was written with', () => {
  expect(cases.length).toBeGreaterThanOrEqual(10);
});

for (const c of cases) {
  test(`agrees with lay_out_day: ${c.name}`, () => {
    const got = layOutDay(c.spans.map(([startMs, endMs]) => ({ startMs, endMs })), c.day_start_ms, c.day_end_ms);
    expect(got.map(({ idx, column, columns }) => ({ idx, column, columns })))
      .toEqual(c.placed.map(({ idx, column, columns }) => ({ idx, column, columns })));
    // Rust's geometry is f32; the port's is f64. Agreement to a millionth of
    // a day is a hundredth of a pixel on the tallest zoom.
    got.forEach((p, i) => {
      expect(p.top).toBeCloseTo(c.placed[i].top, 6);
      expect(p.height).toBeCloseTo(c.placed[i].height, 6);
    });
  });
}
