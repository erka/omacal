export type Rect = { top: number; left: number; width: number; height: number };
export type Size = { width: number; height: number };

export function placePopover(
  anchor: Rect, popover: Size, viewport: Size, gap = 8,
): { top: number; left: number } {
  // Prefer the right of the anchor; flip only when that would overflow, so the
  // popover sits on a consistent side for most events rather than jittering.
  const right = anchor.left + anchor.width + gap;
  let left = right + popover.width + gap > viewport.width
    ? anchor.left - popover.width - gap
    : right;

  // Clamp after flipping: in a viewport too narrow for either side, neither
  // choice fits and staying on screen beats being half off it.
  left = Math.min(left, viewport.width - popover.width - gap);
  left = Math.max(gap, left);

  // Top-aligned with the anchor, lifted only as far as needed. `max(gap, …)`
  // runs last so a popover taller than the viewport pins to the top edge
  // instead of going negative.
  let top = Math.min(anchor.top, viewport.height - popover.height - gap);
  top = Math.max(gap, top);

  return { top, left };
}

/**
 * Where a dropdown opens against its field, in viewport coordinates — for a
 * popup drawn `position: fixed`, which no scrolling ancestor can clip.
 *
 * `DateField`'s calendar and `TimeField`'s list were absolute inside their
 * field, and the Tasks pane's rows scroll: a calendar wider than a narrowed
 * pane was cut off at its edge, days and all (reported 2026-09-17).
 *
 * Under the field, left-aligned with it. Above it only when there is no room
 * below and there is room above — a flip that did not fit either would trade
 * one cut-off for another. Then held inside the viewport by `margin`, the
 * horizontal pull last so a popup wider than the window pins to its left.
 */
export function placeDropdown(
  field: Rect, popup: Size, viewport: Size, gap = 4, margin = 8,
): { top: number; left: number } {
  const below = field.top + field.height + gap;
  const above = field.top - gap - popup.height;
  let top = below + popup.height + margin > viewport.height && above >= margin ? above : below;
  top = Math.max(margin, Math.min(top, viewport.height - popup.height - margin));
  let left = Math.min(field.left, viewport.width - popup.width - margin);
  left = Math.max(margin, left);
  return { top, left };
}
