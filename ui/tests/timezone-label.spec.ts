import { test, expect, type Page } from '@playwright/test';
import { APP_NOW } from './fixtures';

for (const [selected, alias, offset] of [
  ['Europe/Kyiv', 'Europe/Kiev', 'GMT+2'],
  ['Asia/Kolkata', 'Asia/Calcutta', 'GMT+5:30'],
] as const) {
  test.describe(`the header's ${selected} label`, () => {
    test.use({ timezoneId: selected });

    test('keeps the saved spelling when the runtime returns an older alias', async ({ page }) => {
      await page.clock.setFixedTime(APP_NOW);
      await page.addInitScript(({ selected, alias }) => {
        sessionStorage.setItem('omacal-stub-settings', JSON.stringify({ displayTimezone: selected }));
        (window as any).__holdSettings = true;
        // Keep the real zone's date arithmetic and offset, but reproduce a
        // webview whose Intl data still returns the older spelling.
        function withAlias(formatter: Intl.DateTimeFormat) {
          const resolvedOptions = formatter.resolvedOptions.bind(formatter);
          formatter.resolvedOptions = () => {
            const options = resolvedOptions();
            if (options.timeZone === selected) options.timeZone = alias;
            return options;
          };
          return formatter;
        }
        Intl.DateTimeFormat = new Proxy(Intl.DateTimeFormat, {
          apply: (target, self, args) => withAlias(Reflect.apply(target, self, args)),
          construct: (target, args) => withAlias(Reflect.construct(target, args)),
        });
      }, { selected, alias });
      await page.goto('/tests/harness/index.html?c=App');
      const label = page.locator('.tz');
      // The header mounts before settings arrive. It must react when the
      // saved selection lands, rather than freeze its initial fallback.
      await expect(label).toHaveText(alias);
      await page.evaluate(() => window.__harness.releaseSettings());
      await expect(label).toHaveText(selected);
      await expect(label).toHaveAttribute('title', `${selected} · ${offset}`);

      await page.getByRole('button', { name: 'Menu' }).click();
      await page.getByRole('button', { name: 'Settings…' }).click();
      const modal = page.getByRole('dialog', { name: 'Settings' });
      await expect(modal.getByLabel('Time zone', { exact: true })).toHaveValue(selected);
      await expect(modal.getByRole('button', { name: 'Apply & restart' })).toBeDisabled();
    });
  });
}

test('system default keeps the runtime zone after settings load', async ({ page }) => {
  await page.goto('/tests/harness/index.html?c=App');
  await page.getByRole('button', { name: 'Menu' }).click();
  await page.getByRole('button', { name: 'Settings…' }).click();
  await expect(page.getByLabel('Time zone', { exact: true })).toHaveValue('System default');
  await expect(page.locator('.tz')).toHaveText('UTC');
  await expect(page.locator('.tz')).toHaveAttribute('title', /^UTC · GMT(?:\+0)?$/);
});

// The two above are the reported symptom: a zone chosen in Settings. These
// are the rest of the same disagreement — the case nobody chose, and the one
// that leaves the machine.

const app = (fixture = 'default') => `/tests/harness/index.html?c=App&f=${fixture}`;

/** A webview whose ICU resolves `selected` to the older `alias`, over a stub
 *  whose backend names the zone `selected` with nothing saved in Settings.
 *  Date arithmetic and offsets stay the real zone's — only the *name* the
 *  runtime reports is old, which is exactly ICU's behaviour (#140). */
const aliasingWebview = (page: Page, selected: string, alias: string) =>
  page.addInitScript(({ selected, alias }) => {
    sessionStorage.setItem('omacal-stub-settings', JSON.stringify({
      displayTimezone: null, effectiveTimezone: selected,
    }));
    const patch = (formatter: Intl.DateTimeFormat) => {
      const resolvedOptions = formatter.resolvedOptions.bind(formatter);
      formatter.resolvedOptions = () => {
        const options = resolvedOptions();
        if (options.timeZone === selected) options.timeZone = alias;
        return options;
      };
      return formatter;
    };
    Intl.DateTimeFormat = new Proxy(Intl.DateTimeFormat, {
      apply: (target, self, args) => patch(Reflect.apply(target, self, args)),
      construct: (target, args) => patch(Reflect.construct(target, args)),
    });
  }, { selected, alias });

test.describe('a zone nobody selected', () => {
  test.use({ timezoneId: 'Europe/Kyiv' });

  // `displayTimezone` is null here — System default, which is what most
  // installs run on. The saved preference cannot answer, so the name has to
  // come from the backend: this is the case a fix built on the preference
  // alone leaves exactly as it found it.
  test('the header still says what the backend names the zone', async ({ page }) => {
    await page.clock.setFixedTime(APP_NOW);
    await aliasingWebview(page, 'Europe/Kyiv', 'Europe/Kiev');
    await page.goto(app());
    await expect(page.locator('.tz')).toHaveText('Europe/Kyiv');
    await expect(page.locator('.tz')).toHaveAttribute('title', 'Europe/Kyiv · GMT+2');
  });
});

test.describe('the name an event is written with', () => {
  test.use({ timezoneId: 'Europe/Kyiv' });

  // The label is cosmetic; this is not. `tz` becomes the event's `TZID` on
  // the server, so an app that took the name from the webview stamped
  // `Europe/Kiev` on everything a Kyiv user created.
  test("is the backend's, not the webview's", async ({ page }) => {
    await page.clock.setFixedTime(APP_NOW);
    await aliasingWebview(page, 'Europe/Kyiv', 'Europe/Kiev');
    await page.goto(app('writable'));
    await expect(page.locator('.tz')).toHaveText('Europe/Kyiv');

    await page.evaluate(() => window.__harness.emit('quick-add-requested', null));
    const quick = page.getByRole('dialog', { name: 'Quick add event' });
    await quick.getByRole('textbox', { name: 'Describe the event' })
      .fill('30 min at 2pm Design sync');
    await expect(quick.getByText('Design sync', { exact: true })).toBeVisible();
    await quick.getByRole('button', { name: 'Create event' }).click();
    await expect(quick).toHaveCount(0);

    const writes = await page.evaluate(() =>
      window.__harness.calls.filter((c) => c.cmd === 'create_event'));
    expect(writes).toHaveLength(1);
    expect((writes[0].args as { fields: { tz: string } }).fields.tz).toBe('Europe/Kyiv');
  });
});
