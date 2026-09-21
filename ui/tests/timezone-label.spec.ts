import { test, expect } from '@playwright/test';
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
