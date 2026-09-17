import { test, expect } from '@playwright/test';
import { openingSlot, slotAtOrBefore, stepSlot, timeSlots } from '../src/lib/timepicker';

/**
 * The task editor's time list (2026-09-17): which times it offers, and
 * where it opens. Invisible from a rendered list, where "opened on 12:30"
 * and "happened to be scrolled near 12:30" look alike.
 */
const SLOTS = timeSlots(30);
const at = (h: number, m = 0) => new Date(2026, 8, 17, h, m).getTime();

test('the list is every half hour of a day, from midnight', () => {
  expect(SLOTS).toHaveLength(48);
  expect(SLOTS.slice(0, 3)).toEqual(['00:00', '00:30', '01:00']);
  expect(SLOTS[SLOTS.length - 1]).toBe('23:30');
  expect(timeSlots(15)).toHaveLength(96);
});

test('a time between slots opens on the slot before it', () => {
  expect(slotAtOrBefore('11:15', SLOTS)).toBe('11:00');
  expect(slotAtOrBefore('11:30', SLOTS)).toBe('11:30');
  expect(slotAtOrBefore('23:59', SLOTS)).toBe('23:30');
  expect(slotAtOrBefore('00:00', SLOTS)).toBe('00:00');
  expect(slotAtOrBefore('soon', SLOTS)).toBe(null);
});

test('a field with a time opens on it, whatever day it is', () => {
  expect(openingSlot('15:00', SLOTS, { nowMs: at(9), isToday: true })).toBe('15:00');
  expect(openingSlot('15:10', SLOTS, { nowMs: at(9), isToday: false })).toBe('15:00');
});

/** A task due today is due later today. */
test('an empty field for today opens on the next half hour after now', () => {
  expect(openingSlot('', SLOTS, { nowMs: at(12, 0), isToday: true })).toBe('12:30');
  expect(openingSlot('', SLOTS, { nowMs: at(12, 10), isToday: true })).toBe('12:30');
  expect(openingSlot('', SLOTS, { nowMs: at(12, 30), isToday: true })).toBe('13:00');
  // Late in the evening there is no later slot; the last one, not midnight.
  expect(openingSlot('', SLOTS, { nowMs: at(23, 45), isToday: true })).toBe('23:30');
});

test('an empty field for another day opens on 09:00', () => {
  expect(openingSlot('', SLOTS, { nowMs: at(22), isToday: false })).toBe('09:00');
});

test('the arrow keys step through the list and stop at its ends', () => {
  expect(stepSlot(SLOTS, '11:00', 1)).toBe('11:30');
  expect(stepSlot(SLOTS, '11:00', -1)).toBe('10:30');
  expect(stepSlot(SLOTS, '23:30', 1)).toBe('23:30');
  expect(stepSlot(SLOTS, '00:00', -1)).toBe('00:00');
  // From a time the list does not hold, or from nothing, the first row.
  expect(stepSlot(SLOTS, null, 1)).toBe('00:00');
  expect(stepSlot(SLOTS, '11:15', 1)).toBe('00:00');
});
