// The name of the zone the app runs in, as the rest of the UI says it.
//
// A module-level rune for `secondzone.svelte.ts`'s reason exactly: the
// readers — the header's label and its moved-zone banner, the Week gutter's
// abbreviation, the `TZID` the event form writes — sit in different subtrees
// and none of them owns the answer.
//
// **The backend names it, not `Intl`.** ICU keeps its own alias table and
// resolves `Europe/Kyiv` to `Europe/Kiev`, so a name taken from the webview
// is a zone the picker never offered: the header disagreed with Settings,
// and the form stamped the old spelling onto every event it wrote (#140).
// The backend answers from jiff — the same IANA database the picker lists
// and the validator checks against — frozen at launch, so it stays the zone
// the grid is actually drawn in even if the machine moves underneath it.
//
// Seeded and kept fresh by `App`, the only writer. Until the first settings
// read lands, `Intl`'s own answer is the best there is: the right zone,
// under a name that may be a decade out of date.

const state = $state<{ zone: string }>({
  zone: Intl.DateTimeFormat().resolvedOptions().timeZone,
});

/** The IANA zone every time in the app is read in. Read at render time, so
 *  the name repaints everywhere when settings land without any reader
 *  subscribing to anything. */
export const zoneName = () => state.zone;

/** `App` only. Called on startup and after every settings change.
 *
 *  A missing name is ignored rather than stored. The backend always sends
 *  one, so this only fires if some settings-bearing reply ever forgets the
 *  field — and the cost of trusting it blindly is out of all proportion to
 *  the field: `zoneAbbrev` splits the name on `/`, so one `null` through
 *  here throws inside the gutter and takes the whole grid's render with it.
 *  A stale name is a wrong label; no name is no calendar. */
export const setZoneName = (z: string | null | undefined) => {
  if (z) state.zone = z;
};
