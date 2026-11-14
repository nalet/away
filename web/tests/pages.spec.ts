// Getting in and the frame, as a person meets them in a browser.
import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';
import type { Page, Request } from '@playwright/test';

import { readFileSync, writeFileSync } from 'node:fs';

import { AWAY, AWAY_WITHOUT_SERVICE, FILES, SIGN_IN_SERVICE } from '../playwright.config';

// Tells the stand-in for the sign-in service how to behave from now on.
async function service(told: Record<string, string>) {
  const answer = await fetch(`${SIGN_IN_SERVICE}/stand-in?${new URLSearchParams(told)}`, { method: 'POST' });
  expect(answer.ok).toBe(true);
}

// What the browser asked for while a test ran, and what it wrote to its console.
function watch(page: Page) {
  const asked: Request[] = [];
  const complaints: string[] = [];
  page.on('request', (request) => asked.push(request));
  page.on('console', (message) => {
    if (message.type() === 'error' || message.type() === 'warning') complaints.push(message.text());
  });
  page.on('pageerror', (error) => complaints.push(error.message));
  return { asked, complaints };
}

// The rules a page has to meet: WCAG 2.2, levels A and AA.
async function accessible(page: Page) {
  const found = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']).analyze();
  expect(found.violations.map((violation) => `${violation.id}: ${violation.help} (${violation.nodes.length})`)).toEqual([]);
}

// What has the keyboard's focus, by its name.
function focused(page: Page) {
  return page.evaluate(() => {
    const element = document.activeElement;
    return element?.getAttribute('aria-label') ?? element?.textContent?.trim() ?? '';
  });
}

test.beforeEach(async () => {
  await service({ reset: 'true' });
});

test('someone who opens Away is signed in and sees the frame, and the pages load nothing from outside', async ({ page }) => {
  const { asked, complaints } = watch(page);
  const first = await page.goto('/');
  await expect(page).toHaveURL(`${AWAY}/`);
  await expect(page).toHaveTitle('home · away');
  await expect(page.getByRole('banner').getByRole('link', { name: 'away, home' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Alex Urban, your account' }).locator('.av')).toHaveText('AU');
  const rail = page.getByRole('navigation', { name: 'views' });
  await expect(rail.getByRole('link')).toHaveCount(4);
  await expect(rail.getByRole('link', { name: 'home' })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('heading', { level: 1, name: 'home' })).toBeAttached();
  await expect(page.getByRole('heading', { level: 2, name: 'engineering 3' })).toBeVisible();

  // the page comes with the rule that lets a browser load Away's own files, and nothing else
  expect(first?.headers()['content-security-policy']).toBe(
    "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self'; " +
      "base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
  );
  // the browser went to the sign-in service to sign in, and for nothing else; everything
  // a page loaded came from Away
  const elsewhere = asked.filter((request) => !request.url().startsWith(`${AWAY}/`));
  expect(elsewhere.length).toBeGreaterThan(0);
  for (const request of elsewhere) {
    expect(request.url().startsWith(`${SIGN_IN_SERVICE}/authorize?`)).toBe(true);
    expect(request.isNavigationRequest()).toBe(true);
  }
  expect(asked.some((request) => request.resourceType() === 'font')).toBe(true);
  expect(complaints).toEqual([]);
  await accessible(page);
});

test('a link to a page of Away leads there after signing in', async ({ page }) => {
  await page.goto('/settings');
  await expect(page).toHaveURL(`${AWAY}/settings`);
  await expect(page).toHaveTitle('settings · away');
  await expect(page.getByRole('heading', { level: 1, name: 'settings' })).toBeVisible();
  const account = page.getByRole('region', { name: 'account' });
  await expect(account).toContainText('Alex Urban');
  await expect(account).toContainText('alex.urban@example.com');
  await expect(page.getByText('Questions go to it-help@example.com.')).toBeVisible();
  const about = page.getByRole('region', { name: 'about' });
  await expect(about.getByRole('link', { name: /^About Away/ })).toHaveAttribute('href', '/settings/about');
  await expect(about).toContainText(/Version \d+\.\d+\.\d+, its source and its licences\./);
  await accessible(page);
});

test('Away says which version runs, under which licence, and what it is built from', async ({ page }) => {
  const { asked, complaints } = watch(page);
  await page.goto('/settings');
  await page.getByRole('link', { name: /^About Away/ }).click();
  await expect(page).toHaveURL(`${AWAY}/settings/about`);
  await expect(page).toHaveTitle('about away · away');
  await expect(page.getByRole('heading', { level: 1, name: 'about away' })).toBeVisible();
  // it is a page of the settings, which stay the view one is in
  await expect(page.getByRole('navigation', { name: 'views' }).getByRole('link', { name: 'settings' })).toHaveAttribute('aria-current', 'page');

  const itself = page.getByRole('region', { name: 'this away' });
  await expect(itself).toContainText(/Version\s*\d+\.\d+\.\d+/);
  await expect(itself).toContainText('Mozilla Public License 2.0');
  await expect(itself.getByRole('link', { name: 'its text' })).toHaveAttribute('href', '/licence.txt');
  await expect(itself.getByRole('link', { name: 'How to report it in private' })).toHaveAttribute('href', /^https:\/\/.+\/security\/policy$/);
  await expect(itself).toContainText('This Away asks nobody outside for anything: no fonts, no scripts, no statistics and no checks for updates.');
  // the licence's text comes from Away itself
  expect(await page.evaluate(() => fetch('/licence.txt').then((answer) => answer.text()))).toMatch(/^Mozilla Public License Version 2\.0/);

  // every part, with its version and its licence
  const parts = page.locator('section[aria-labelledby="built-from"]');
  await expect(parts.getByRole('heading', { level: 2, name: 'what it is built from' })).toBeVisible();
  await expect(parts.getByRole('status')).toHaveText(/^\d+ parts$/);
  const all = Number(((await parts.getByRole('status').textContent()) ?? '').split(' ')[0]);
  expect(all).toBeGreaterThan(20);
  const rows = parts.getByRole('table', { name: 'what it is built from' }).getByRole('row');
  await expect(rows).toHaveCount(all + 1);
  await expect(rows.nth(0)).toHaveText(/part\s*version\s*licence/);
  await expect(parts.getByRole('row', { name: /^rustls \d/ })).toContainText('Apache-2.0 or ISC or MIT');

  // found by a name or by a licence, with the keyboard alone
  const find = parts.getByRole('searchbox', { name: 'find a part…' });
  await find.focus();
  await page.keyboard.type('ofl');
  await expect(parts.getByRole('status')).toHaveText(`2 of ${all} parts`);
  await expect(rows).toHaveCount(3);
  await find.fill('no such part');
  await expect(parts).toContainText('No part has that in its name or its licence.');
  await accessible(page);
  await find.fill('');
  await expect(rows).toHaveCount(all + 1);

  // nothing was loaded from outside, and nothing went wrong
  expect(asked.filter((request) => !request.url().startsWith(`${AWAY}/`) && !request.url().startsWith(`${SIGN_IN_SERVICE}/authorize?`))).toEqual([]);
  expect(complaints).toEqual([]);
  await accessible(page);

  // on a phone the page is one column, and nothing is wider than the screen
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(itself).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await accessible(page);
  await page.setViewportSize({ width: 1280, height: 800 });

  // and back to the settings, with the keyboard
  await page.getByRole('main').getByRole('link', { name: 'settings' }).focus();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${AWAY}/settings`);
  await page.goBack();
  await expect(page).toHaveTitle('about away · away');
});

test('what Away is built from is said only to those who are signed in', async ({ request }) => {
  const refused = await request.get(`${AWAY}/api/built-from`);
  expect(refused.status()).toBe(401);
  expect((await request.get(`${AWAY}/licence.txt`)).status()).toBe(200);
});

// The tiles of a group on home: each machine's name and what it is.
async function tiles(page: Page, group: string) {
  const section = page.locator('section.grp').filter({ has: page.getByRole('heading', { level: 2, name: group }) });
  return section.getByRole('link').evaluateAll((links) => links.map((link) => [link.querySelector('.name')?.textContent, link.querySelector('.sub')?.textContent]));
}

test('home shows the machines a rule gives, in their groups, and a person sees no others', async ({ page }) => {
  const { complaints } = watch(page);
  await page.goto('/');
  // an administrator of the tests is in staff and in away-admins: both rules give
  await expect(page.locator('section.grp h2')).toHaveText(['design 1', 'engineering 3', 'finance 2', 'other machines 1']);
  expect(await tiles(page, 'engineering 3')).toEqual([
    ['build-01', 'shell'],
    ['build-02', 'shell'],
    ['dev-linux-03', 'Linux desktop, shell'],
  ]);
  expect(await tiles(page, 'finance 2')).toEqual([
    ['fin-app-02', 'Windows desktop, 4 programs'],
    ['hr-app-01', 'Windows desktop'],
  ]);
  expect(await tiles(page, 'design 1')).toEqual([['mac-design-01', 'Mac desktop']]);
  expect(await tiles(page, 'other machines 1')).toEqual([['Desktops', 'Windows desktop']]);
  expect(complaints).toEqual([]);
  await accessible(page);

  // someone who is in staff alone: what the one rule gives
  await service({ person: 'person' });
  await page.goto('/sign-in?to=/');
  await expect(page.getByRole('button', { name: 'Robin Meyer, your account' })).toBeVisible();
  await expect(page.locator('section.grp h2')).toHaveText(['finance 2', 'other machines 1']);
  await expect(page.getByRole('link', { name: /build-01/ })).toHaveCount(0);
  // and a machine that no rule gives them is not there for them, by its address either
  await page.goto('/machines/build-01');
  await expect(page.getByRole('heading', { name: 'no machine of that name for you' })).toBeVisible();
  await expect(page.getByText("It doesn't exist, or no rule gives it to you.")).toBeVisible();
  await accessible(page);
  await page.getByRole('link', { name: 'go home' }).click();
  await expect(page).toHaveTitle('home · away');
});

test('a machine is found by its name, its address or its group', async ({ page }) => {
  await page.goto('/');
  const find = page.getByRole('searchbox', { name: 'find a machine…' });
  const shown = () => page.locator('a.m .name').allTextContents();
  await find.focus();
  await page.keyboard.type('BUILD');
  expect(await shown()).toEqual(['build-01', 'build-02']);
  // a group's name finds its machines
  await find.fill('finance');
  expect(await shown()).toEqual(['fin-app-02', 'hr-app-01']);
  await expect(page.locator('section.grp h2')).toHaveText(['finance 2']);
  // and an address its machine
  await find.fill('farm.corp');
  expect(await shown()).toEqual(['Desktops']);
  await find.fill('xyz');
  await expect(page.getByRole('status')).toContainText('No machine matches “xyz”.');
  await expect(page.getByRole('status')).toContainText('The search looks at the names, the addresses and the groups of the machines you can reach.');
  await accessible(page);
  await find.fill('');
  expect((await shown()).length).toBe(7);
});

test('a machine has a page of its own, which says what it offers and what it is', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('link', { name: /^fin-app-02/ }).click();
  await expect(page).toHaveURL(`${AWAY}/machines/fin-app-02`);
  await expect(page).toHaveTitle('fin-app-02 · away');
  await expect(page.getByRole('heading', { level: 1, name: 'fin-app-02' })).toBeVisible();
  // it is a page of home
  await expect(page.getByRole('navigation', { name: 'views' }).getByRole('link', { name: 'home' })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByText('This version of Away shows the machines you may reach. It opens none yet.')).toBeVisible();
  await expect(page.getByRole('region', { name: 'desktop' })).toContainText('Windows desktop');
  const programs = page.getByRole('region', { name: 'programs 4' });
  await expect(programs.getByRole('listitem')).toHaveText(['Ledger', 'Payroll', 'Reports', 'Mail']);
  await expect(page.getByRole('region', { name: 'shell' })).toHaveCount(0);
  const about = page.getByRole('region', { name: 'about this machine' });
  await expect(about).toContainText('What it isWindows machine');
  await expect(about).toContainText('Addressfin-app-02.corp.example');
  await expect(about).toContainText('Groupsfinance');
  await accessible(page);

  // a link to a machine leads to it, and a farm says its collection
  await page.goto('/machines/Desktops');
  await expect(page).toHaveTitle('Desktops · away');
  await expect(page.getByRole('region', { name: 'about this machine' })).toContainText('CollectionDesktops');
  await expect(page.getByRole('region', { name: 'about this machine' })).toContainText('Groupsnone');
  await page.goto('/machines/dev-linux-03');
  await expect(page.getByRole('region', { name: 'shell' })).toContainText('A shell on the machine');
  await expect(page.getByRole('region', { name: 'desktop' })).toContainText('Linux desktop');

  // on a phone nothing is wider than the screen
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBe(0);
  await accessible(page);
  await page.setViewportSize({ width: 1280, height: 800 });
  // and back to the machines, with the keyboard
  await page.getByRole('main').getByRole('link', { name: 'machines' }).focus();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${AWAY}/`);
});

test('those who administer see what is in force and the file each thing comes from', async ({ page }) => {
  await page.goto('/administration');
  await expect(page.getByRole('tab', { name: 'machines' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByText(/^In force since .+, when the files were last applied\. To change a machine, change its file\.$/)).toBeVisible();
  const machines = page.getByRole('table', { name: 'machines' });
  await expect(machines.getByRole('row')).toHaveCount(8);
  await expect(machines.getByRole('row').nth(0)).toHaveText(/machine\s*what it is\s*groups\s*who reaches it\s*file/);
  const farm = machines.getByRole('row', { name: /^Desktops/ });
  await expect(farm).toContainText('Windows desktop');
  await expect(farm).toContainText('staff-finance gives it to staff');
  await expect(farm).toContainText('machines/engineering.yaml:30');
  await expect(machines.getByRole('row', { name: /^mac-design-01/ })).toContainText('admins-engineering gives it to away-admins');
  // filtered by a name, a group or a file
  await page.getByRole('searchbox', { name: 'filter machines…' }).fill('finance.yaml');
  await expect(machines.getByRole('row')).toHaveCount(3);
  await accessible(page);

  // the rules, on a page of their own
  await page.getByRole('tab', { name: 'rules' }).click();
  await expect(page).toHaveURL(`${AWAY}/administration/rules`);
  await expect(page.getByRole('navigation', { name: 'views' }).getByRole('link', { name: 'administration' })).toHaveAttribute('aria-current', 'page');
  const rules = page.getByRole('table', { name: 'rules' });
  await expect(rules.getByRole('row')).toHaveText([/rule\s*who\s*reaches\s*file/, /admins-engineering\s*away-admins\s*engineering, mac-design-01\s*rules\/access\.yaml:8/, /staff-finance\s*staff\s*finance, Desktops\s*rules\/access\.yaml:4/]);
  await expect(page.getByText('To change a rule, change its file.')).toBeVisible();
  await accessible(page);
  await page.reload();
  await expect(page.getByRole('tab', { name: 'rules' })).toHaveAttribute('aria-selected', 'true');
});

test('a changed file with a mistake in it is refused with its line and its reason, and what was in force stays', async ({ page }) => {
  const file = `${FILES}/machines/finance.yaml`;
  const before = readFileSync(file, 'utf8');
  try {
    // a machine more, with a part that no machine has
    writeFileSync(file, `${before}\n  - name: fin-app-03\n    kind: windows\n    address: fin-app-03.corp.example\n    prot: 3389\n    desktop: true\n`);
    await page.goto('/administration');
    const refused = page.getByRole('status').filter({ hasText: 'A change to machines/finance.yaml was not applied on ' });
    await expect(async () => {
      await page.reload();
      await expect(refused).toBeVisible({ timeout: 1000 });
    }).toPass({ timeout: 20_000 });
    await expect(refused).toContainText('In line 19, “prot” is not something a machine has. What was in force before stays.');
    await expect(page.getByRole('table', { name: 'machines' }).getByRole('row')).toHaveCount(8);
    await accessible(page);
    // and people go on seeing what they saw
    await page.goto('/');
    await expect(page.locator('section.grp h2')).toHaveText(['design 1', 'engineering 3', 'finance 2', 'other machines 1']);

    // the mistake put right: the machine is there, and nothing is refused any more
    writeFileSync(file, `${before}\n  - name: fin-app-03\n    kind: windows\n    address: fin-app-03.corp.example\n    groups: [finance]\n    desktop: true\n`);
    await expect(async () => {
      await page.reload();
      await expect(page.locator('section.grp h2')).toHaveText(['design 1', 'engineering 3', 'finance 3', 'other machines 1'], { timeout: 1000 });
    }).toPass({ timeout: 20_000 });
    await page.goto('/administration');
    await expect(page.getByRole('table', { name: 'machines' }).getByRole('row')).toHaveCount(9);
    await expect(refused).toHaveCount(0);
  } finally {
    writeFileSync(file, before);
  }
  // as it was before, for the tests that follow
  await expect(async () => {
    await page.goto('/');
    await expect(page.locator('section.grp h2')).toHaveText(['design 1', 'engineering 3', 'finance 2', 'other machines 1'], { timeout: 1000 });
  }).toPass({ timeout: 20_000 });
});

test('nothing of the sign-in is in the page: one cookie that no script can read', async ({ page, context }) => {
  await page.goto('/');
  await expect(page).toHaveTitle('home · away');
  const cookies = await context.cookies(AWAY);
  expect(cookies.map((cookie) => cookie.name)).toEqual(['__Host-away']);
  expect(cookies[0]).toMatchObject({ httpOnly: true, secure: true, sameSite: 'Lax', path: '/' });
  expect(await page.evaluate(() => [document.cookie, window.localStorage.length, window.sessionStorage.length])).toEqual(['', 0, 0]);
  // what the page is told about who is signed in holds no token
  const who = await page.evaluate(() => fetch('/api/who').then((answer) => answer.text()));
  expect(who).toContain('"signed_in":true');
  expect(who).not.toMatch(/eyJ|token/);
});

test('every view has an address, and back and forward lead through them', async ({ page }) => {
  await page.goto('/');
  const rail = page.getByRole('navigation', { name: 'views' });
  await rail.getByRole('link', { name: 'sessions' }).click();
  await expect(page).toHaveURL(`${AWAY}/sessions`);
  await expect(page.getByRole('heading', { name: 'no sessions are open' })).toBeVisible();
  await accessible(page);
  await rail.getByRole('link', { name: 'administration' }).click();
  await expect(page).toHaveURL(`${AWAY}/administration`);
  await expect(page.getByRole('heading', { level: 1, name: 'administration' })).toBeVisible();
  await accessible(page);
  await page.goBack();
  await expect(page).toHaveTitle('sessions · away');
  await page.goBack();
  await expect(page).toHaveTitle('home · away');
  await page.goForward();
  await expect(rail.getByRole('link', { name: 'sessions' })).toHaveAttribute('aria-current', 'page');
  // a reload stays where it is, and the mark leads home
  await page.reload();
  await expect(page).toHaveTitle('sessions · away');
  await page.getByRole('link', { name: 'away, home' }).click();
  await expect(page).toHaveURL(`${AWAY}/`);
  // an address that is nothing says so, inside the frame
  await page.goto('/no-such-page');
  await expect(page.getByRole('heading', { name: 'nothing is at this address' })).toBeVisible();
  await accessible(page);
  await page.getByRole('link', { name: 'go home' }).click();
  await expect(page).toHaveTitle('home · away');
});

test('the whole frame works with the keyboard alone', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle('home · away');
  // the Tab key goes from the link that skips to the page, to the mark, the avatar, the rail, and then the page
  const order: string[] = [];
  for (let stop = 0; stop < 7; stop += 1) {
    await page.keyboard.press('Tab');
    order.push(await focused(page));
  }
  expect(order).toEqual(['skip to the page', 'away, home', 'AU, Alex Urban, your account', 'home', 'sessions', 'administration', 'settings']);
  // a view under the keyboard's focus says its name
  await expect(page.locator('.rail .it-settings + .nc-tooltip__bubble')).toBeVisible();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${AWAY}/settings`);

  // the link that skips to the page
  await page.goto('/');
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'skip to the page' })).toBeInViewport();
  await page.keyboard.press('Enter');
  expect(await page.evaluate(() => document.activeElement?.id)).toBe('page');

  // the avatar's menu: opened, gone through and left again
  const avatar = page.getByRole('button', { name: 'Alex Urban, your account' });
  await avatar.focus();
  await page.keyboard.press('Enter');
  await expect(avatar).toHaveAttribute('aria-expanded', 'true');
  const menu = page.getByRole('menu');
  await expect(menu).toContainText('Alex Urban');
  await expect(menu).toContainText('alex.urban@example.com');
  expect(await focused(page)).toBe('settings');
  await accessible(page);
  await page.keyboard.press('ArrowDown');
  expect(await focused(page)).toBe('sign out');
  await page.keyboard.press('ArrowDown');
  expect(await focused(page)).toBe('settings');
  await page.keyboard.press('End');
  expect(await focused(page)).toBe('sign out');
  await page.keyboard.press('Escape');
  await expect(menu).toBeHidden();
  await expect(avatar).toBeFocused();

  // signing out, and in again
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${AWAY}/?signed-out`);
  await expect(page.getByRole('status')).toHaveText("You're signed out.");
  await expect(page.getByText("You sign in with your organisation's account.")).toBeVisible();
  await accessible(page);
  await page.keyboard.press('Tab');
  expect(await focused(page)).toBe('sign in');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${AWAY}/`);
  await expect(page).toHaveTitle('home · away');
});

test('signing out ends the sign-in, and the page of someone who is signed out sends nobody on by itself', async ({ page, context }) => {
  await page.goto('/sessions');
  await page.getByRole('button', { name: 'Alex Urban, your account' }).click();
  await page.getByRole('menuitem', { name: 'sign out' }).click();
  await expect(page).toHaveURL(`${AWAY}/?signed-out`);
  expect(await context.cookies(AWAY)).toEqual([]);
  await page.reload();
  await expect(page.getByRole('button', { name: 'sign in' })).toBeVisible();
  // any other address of Away signs the person in again, and leads where it says
  await page.goto('/sessions');
  await expect(page).toHaveTitle('sessions · away');
});

test('on a phone the views are a bar of icons at the foot, and a rail when it is held sideways', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await expect(page).toHaveTitle('home · away');
  const views = page.getByRole('navigation', { name: 'views' });
  // one navigation shows: the bar at the foot, with the views and without the settings
  await expect(views).toHaveCount(1);
  await expect(views.getByRole('link')).toHaveText(['', '', '']);
  const tabs = await views.getByRole('link').evaluateAll((links) =>
    links.map((link) => {
      const box = link.getBoundingClientRect();
      return { name: link.getAttribute('aria-label'), height: box.height, bottom: Math.round(box.bottom), current: link.getAttribute('aria-current') };
    }),
  );
  expect(tabs.map((tab) => tab.name)).toEqual(['home', 'sessions', 'administration']);
  expect(tabs[0]?.current).toBe('page');
  for (const tab of tabs) {
    expect(tab.height).toBeGreaterThanOrEqual(48);
    expect(tab.bottom).toBe(844);
  }
  await expect(page.getByRole('link', { name: 'away, home' })).toHaveText('away');
  // nothing needs scrolling sideways, and everything that is pressed is at least 48 px high
  expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBe(0);
  const avatar = page.getByRole('button', { name: 'Alex Urban, your account' });
  expect((await avatar.boundingBox())?.height).toBeGreaterThanOrEqual(48);
  await accessible(page);
  // the settings are in the avatar's menu
  await avatar.click();
  for (const item of await page.getByRole('menuitem').all()) {
    expect((await item.boundingBox())?.height).toBeGreaterThanOrEqual(48);
  }
  await page.getByRole('menuitem', { name: 'settings' }).click();
  await expect(page).toHaveURL(`${AWAY}/settings`);
  expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBe(0);
  await accessible(page);

  // held sideways: the views are a rail down the left edge, and the page keeps its height
  await page.setViewportSize({ width: 780, height: 360 });
  await expect(views).toHaveCount(1);
  const rail = await views.boundingBox();
  expect(rail?.x).toBe(0);
  expect(rail?.width).toBe(80);
  await expect(views.getByRole('link')).toHaveCount(3);
  await accessible(page);
});

test('someone who administers nothing has no administration', async ({ page }) => {
  await service({ person: 'person' });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Robin Meyer, your account' }).locator('.av')).toHaveText('RM');
  const rail = page.getByRole('navigation', { name: 'views' });
  await expect(rail.getByRole('link')).toHaveCount(3);
  await expect(rail.getByRole('link', { name: 'administration' })).toHaveCount(0);
  await page.goto('/administration');
  await expect(page.getByRole('heading', { name: 'nothing is at this address' })).toBeVisible();
});

test('a sign-in that has run out asks for a new one over the page, and signing in again leads back', async ({ page, context }) => {
  // what the service signs is good for a little over a minute, so the server renews it at once
  await service({ signs_for: '61', refuses_renewals: 'true' });
  await page.goto('/settings');
  await expect(page).toHaveTitle('settings · away');
  const before = (await context.cookies(AWAY))[0]?.value;
  const card = page.getByRole('alertdialog', { name: 'sign in again' });
  await expect(async () => {
    await page.reload();
    await expect(card).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 20_000 });
  await expect(card).toContainText('Your sign-in has run out.');
  await expect(card.getByRole('button', { name: 'sign in' })).toBeFocused();
  await accessible(page);

  await service({ reset: 'true' });
  await card.getByRole('button', { name: 'sign in' }).click();
  await expect(page).toHaveURL(`${AWAY}/settings`);
  await expect(card).toBeHidden();
  expect((await context.cookies(AWAY))[0]?.value).toBe(before);
});

test('someone who gives up on the sign-in page sees how one signs in, and goes in no circles', async ({ page }) => {
  await service({ sends_back_with: 'access_denied' });
  const { asked } = watch(page);
  await page.goto('/sessions');
  await expect(page).toHaveURL(`${AWAY}/?sign-in=none`);
  await expect(page.getByRole('heading', { level: 1, name: 'away' })).toBeVisible();
  await expect(page.getByRole('status')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'sign in' })).toBeVisible();
  await page.waitForTimeout(500);
  expect(asked.filter((request) => request.url().startsWith(`${SIGN_IN_SERVICE}/authorize?`))).toHaveLength(1);
  await accessible(page);
  await service({ reset: 'true' });
  await page.getByRole('button', { name: 'sign in' }).click();
  await expect(page).toHaveTitle('home · away');
});

test('a sign-in service that does not answer is said not to answer, with the reason and whom to turn to', async ({ page }) => {
  await page.goto(`${AWAY_WITHOUT_SERVICE}/settings`);
  await expect(page).toHaveURL(`${AWAY_WITHOUT_SERVICE}/?sign-in=failed`);
  await expect(page).toHaveTitle("couldn't sign you in · away");
  await expect(page.getByRole('heading', { name: "couldn't sign you in" })).toBeVisible();
  await expect(page.getByText("The sign-in service didn't answer.")).toBeVisible();
  await expect(page.getByText('Questions go to it-help@example.com.')).toBeVisible();
  const details = page.locator('details');
  await expect(details.locator('p')).toBeHidden();
  await details.locator('summary').click();
  await expect(details.locator('p')).toContainText("The sign-in service: it couldn't be reached");
  await accessible(page);
  // trying again fails the same way, and says so again
  await page.getByRole('button', { name: 'try again' }).click();
  await expect(page).toHaveURL(`${AWAY_WITHOUT_SERVICE}/?sign-in=failed`);
  await expect(page.getByRole('heading', { name: "couldn't sign you in" })).toBeVisible();
  // a reload, with nothing new to say, shows how one signs in
  await page.reload();
  await expect(page.getByRole('button', { name: 'sign in' })).toBeVisible();
});
