import { expect, test } from '@playwright/test';
import {
  TASKS_WIDTH_DEFAULT, TASKS_WIDTH_MAX, TASKS_WIDTH_MIN, clampTasksWidth,
} from '../src/lib/taskwidth';

/**
 * The sidebar's width (#130), as a rule rather than as a drag: the Rust side
 * clamps the stored row to the same pair, so a hand-edited value and a
 * hand-dragged one land on the same range.
 */
test.describe('the tasks sidebar width', () => {
  test('a drag is held to a readable minimum and a calendar-first maximum', () => {
    expect(clampTasksWidth(TASKS_WIDTH_DEFAULT)).toBe(TASKS_WIDTH_DEFAULT);
    expect(clampTasksWidth(TASKS_WIDTH_MIN - 200)).toBe(TASKS_WIDTH_MIN);
    expect(clampTasksWidth(TASKS_WIDTH_MAX + 900)).toBe(TASKS_WIDTH_MAX);
    expect(clampTasksWidth(0)).toBe(TASKS_WIDTH_MIN);
    // Whole pixels: the row is an integer and so is the layout.
    expect(clampTasksWidth(301.6)).toBe(302);
  });

  test('a width that is not a number is the default, never a collapsed panel', () => {
    expect(clampTasksWidth(Number.NaN)).toBe(TASKS_WIDTH_DEFAULT);
    expect(clampTasksWidth(Number.POSITIVE_INFINITY)).toBe(TASKS_WIDTH_DEFAULT);
  });

  test('the range keeps the calendar the larger half of a laptop pane', () => {
    expect(TASKS_WIDTH_MIN).toBeLessThan(TASKS_WIDTH_DEFAULT);
    expect(TASKS_WIDTH_DEFAULT).toBeLessThan(TASKS_WIDTH_MAX);
    expect(TASKS_WIDTH_MAX).toBeLessThan(1280 / 2);
  });
});
