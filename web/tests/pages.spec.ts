// Getting in and the frame, as a person meets them in a browser.
import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';
import type { Page, Request } from '@playwright/test';

import { readFileSync, writeFileSync } from 'node:fs';

import { AWAY, AWAY_THAT_KEEPS, AWAY_WITHOUT_SERVICE, DATABASE, FARM, FILES, SIGN_IN_SERVICE } from '../playwright.config';

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
  await expect(rail.getByRole('link')).toHaveCount(6);
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
  await expect(page.locator('section.grp h2')).toHaveText(['engineering 3', 'design 1', 'finance 6', 'other machines 1']);
  expect(await tiles(page, 'engineering 3')).toEqual([
    ['build-01', 'shell'],
    ['build-02', 'shell'],
    ['dev-linux-03', 'Linux desktop, shell'],
  ]);
  // the machines first, and then every program as an entry of its own
  expect(await tiles(page, 'finance 6')).toEqual([
    ['fin-app-02', 'Windows desktop, 4 programs'],
    ['hr-app-01', 'Windows desktop'],
    ['Ledger', 'program on fin-app-02'],
    ['Payroll', 'program on fin-app-02'],
    ['Reports', 'program on fin-app-02'],
    ['Mail', 'program on fin-app-02'],
  ]);
  // a program that the files give an icon shows it, as a picture that Away itself serves
  const icon = page.getByRole('link', { name: /^Ledger/ }).locator('img.icon');
  await expect(icon).toHaveAttribute('src', /^\/api\/icons\/[0-9a-f]{32}\.png$/);
  expect(await icon.evaluate((picture: HTMLImageElement) => picture.complete && picture.naturalWidth)).toBe(32);
  await expect(page.getByRole('link', { name: /^Payroll/ }).locator('img')).toHaveCount(0);
  await expect(page.getByRole('link', { name: /^Payroll/ }).locator('svg')).toHaveCount(1);
  // a program is opened by a click on it: its address names its machine and itself
  await expect(page.getByRole('link', { name: /^Ledger/ })).toHaveAttribute('href', '/machines/fin-app-02/programs/Ledger');
  expect(await tiles(page, 'design 1')).toEqual([['mac-design-01', 'Mac desktop']]);
  expect(await tiles(page, 'other machines 1')).toEqual([['Desktops', 'Windows desktop']]);
  expect(complaints).toEqual([]);
  await accessible(page);

  // someone who is in staff alone: what the one rule gives
  await service({ person: 'person' });
  await page.goto('/sign-in?to=/');
  await expect(page.getByRole('button', { name: 'Robin Meyer, your account' })).toBeVisible();
  await expect(page.locator('section.grp h2')).toHaveText(['finance 6', 'other machines 1']);
  await expect(page.getByRole('link', { name: /build-01/ })).toHaveCount(0);
  // and a machine that no rule gives them is not there for them, by its address either
  await page.goto('/machines/build-01');
  await expect(page.getByRole('heading', { name: 'no machine of that name for you' })).toBeVisible();
  await expect(page.getByText("It doesn't exist, or no rule gives it to you.")).toBeVisible();
  await accessible(page);
  await page.getByRole('link', { name: 'go home' }).click();
  await expect(page).toHaveTitle('home · away');
});

test('a machine is found by its name, its address or its group, and a program by its name', async ({ page }) => {
  await page.goto('/');
  const find = page.getByRole('searchbox', { name: 'find a machine or a program…' });
  const shown = () => page.locator('a.m .name').allTextContents();
  await find.focus();
  await page.keyboard.type('BUILD');
  expect(await shown()).toEqual(['build-01', 'build-02']);
  // a group's name finds its machines
  await find.fill('finance');
  expect(await shown()).toEqual(['fin-app-02', 'hr-app-01', 'Ledger', 'Payroll', 'Reports', 'Mail']);
  await expect(page.locator('section.grp h2')).toHaveText(['finance 6']);
  // a program is found by its own name
  await find.fill('ledg');
  expect(await shown()).toEqual(['Ledger']);
  // and an address its machine
  await find.fill('farm.corp');
  expect(await shown()).toEqual(['Desktops']);
  await find.fill('xyz');
  await expect(page.getByRole('status')).toContainText('Nothing matches “xyz”.');
  await expect(page.getByRole('status')).toContainText('The search looks at the names, the addresses and the groups of the machines you can reach, and at the names of their programs.');
  await accessible(page);
  await find.fill('');
  expect((await shown()).length).toBe(11);
});

test('a native desktop uses its existing tile and a bookmark opens the same instructions', async ({ page }) => {
  const { asked } = watch(page);
  const sockets: string[] = [];
  page.on('websocket', (socket) => sockets.push(socket.url()));
  // Keep the command out of the shared clipboard used by other browser tests.
  await page.addInitScript(() => {
    let copied = '';
    Object.defineProperty(navigator, 'clipboard', {
      value: {
        writeText: async (text: string) => { copied = text; },
        readText: async () => copied,
      },
    });
  });
  const command = 'mstsc.exe /restrictedAdmin /v:hr-app-01.corp.example:3390';
  await page.route('**/api/machines', async (route) => {
    const response = await route.fetch();
    const catalog = (await response.json()) as { machines: { name: string; native_rdp?: string }[] };
    const machine = catalog.machines.find((one) => one.name === 'hr-app-01');
    expect(machine).toBeDefined();
    if (machine) machine.native_rdp = command;
    await route.fulfill({ response, json: catalog });
  });

  await page.goto('/');
  await expect(page.getByRole('link', { name: /^Desktops/ })).toHaveAttribute('href', '/machines/Desktops/desktop');
  await expect(page.getByRole('link', { name: /^Ledger/ })).toHaveAttribute('href', '/machines/fin-app-02/programs/Ledger');
  const tile = page.getByRole('link', { name: /^hr-app-01/ });
  await expect(tile).toHaveCount(1);
  await tile.click();
  await expect(page).toHaveURL(`${AWAY}/machines/hr-app-01`);
  const desktop = page.getByRole('region', { name: 'desktop' });
  await expect(desktop).toContainText('Restricted Admin in Remote Desktop Connection');
  await expect(desktop.getByRole('textbox', { name: 'Remote Desktop command' })).toHaveValue(command);
  await desktop.getByRole('button', { name: 'Copy command' }).click();
  await expect(desktop.getByRole('status')).toHaveText('Command copied.');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(command);
  await accessible(page);

  await page.goto('/machines/hr-app-01/desktop');
  await expect(desktop.getByRole('textbox', { name: 'Remote Desktop command' })).toHaveValue(command);
  expect(asked.filter((request) => request.url().includes('/api/sessions'))).toEqual([]);
  expect(sockets).toEqual([]);
});

test('one management tile opens Restricted Admin by default and offers certificate SSO explicitly', async ({ page }) => {
  const requests: URL[] = [];
  await page.route('**/api/machines', async (route) => {
    const response = await route.fetch();
    const catalog = (await response.json()) as { machines: { name: string; sign_in_methods?: string[] }[] };
    const machine = catalog.machines.find((one) => one.name === 'hr-app-01');
    expect(machine).toBeDefined();
    if (machine) machine.sign_in_methods = ['restricted-admin-sso', 'themselves'];
    await route.fulfill({ response, json: catalog });
  });
  await page.route('**/api/sessions?*', async (route) => {
    requests.push(new URL(route.request().url()));
    await route.fulfill({ status: 403, body: 'Test admission refused.' });
  });
  await page.goto('/');
  const tile = page.getByRole('link', { name: /^hr-app-01/ });
  await expect(tile).toHaveCount(1);
  await expect(tile).toHaveAttribute('href', '/machines/hr-app-01/desktop');
  await expect(page.getByRole('link', { name: 'Open with certificate SSO on hr-app-01', exact: true })).toHaveCount(1);
  await accessible(page);
  await tile.click();
  await expect.poll(() => requests.length).toBe(1);
  expect(requests[0]?.searchParams.get('signs_in')).toBeNull();
  await page.goto('/');
  await page.getByRole('link', { name: 'Open with certificate SSO on hr-app-01', exact: true }).click();
  await expect(page).toHaveURL(`${AWAY}/machines/hr-app-01/desktop/themselves`);
  await expect.poll(() => requests.length).toBe(2);
  expect(requests[1]?.searchParams.get('signs_in')).toBe('themselves');
  await page.reload();
  await expect.poll(() => requests.length).toBe(3);
  expect(requests[2]?.searchParams.get('signs_in')).toBe('themselves');
  await page.goto('/machines/hr-app-01');
  await expect(page.getByRole('link', { name: 'Open with Restricted Admin', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open with certificate SSO', exact: true })).toBeVisible();
  await accessible(page);
});

test('a machine has a page of its own, which says what it offers and what it is', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('link', { name: /^fin-app-02/ }).click();
  await expect(page).toHaveURL(`${AWAY}/machines/fin-app-02`);
  await expect(page).toHaveTitle('fin-app-02 · away');
  await expect(page.getByRole('heading', { level: 1, name: 'fin-app-02' })).toBeVisible();
  // it is a page of the machines
  await expect(page.getByRole('navigation', { name: 'views' }).getByRole('link', { name: 'machines' })).toHaveAttribute('aria-current', 'page');
  // everything it offers is opened by a click, so nothing is said of what Away doesn't open
  await expect(page.getByText("It doesn't open shells or other desktops yet.")).toHaveCount(0);
  await expect(page.getByRole('region', { name: 'programs 4' }).getByRole('link', { name: 'Ledger' })).toHaveAttribute('href', '/machines/fin-app-02/programs/Ledger');
  await expect(page.getByRole('region', { name: 'desktop' })).toContainText('Windows desktop');
  const programs = page.getByRole('region', { name: 'programs 4' });
  await expect(programs.getByRole('listitem')).toHaveText(['Ledger', 'Payroll', 'Reports', 'Mail']);
  await expect(programs.getByRole('listitem').nth(0).locator('img.icon')).toHaveAttribute('src', /^\/api\/icons\/[0-9a-f]{32}\.png$/);
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
  // a machine that offers what Away doesn't open yet says so
  await expect(page.getByText("This version of Away opens Windows desktops and the programs of Windows machines. It doesn't open shells or other desktops yet.")).toBeVisible();

  // on a phone nothing is wider than the screen
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBe(0);
  await accessible(page);
  await page.setViewportSize({ width: 1280, height: 800 });
  // and back to the machines, with the keyboard
  await page.getByRole('main').getByRole('link', { name: 'machines' }).focus();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${AWAY}/machines`);
});

test('the programs have a page of their own, and so have the machines', async ({ page }) => {
  await page.goto('/');
  const rail = page.getByRole('navigation', { name: 'views' });
  await rail.getByRole('link', { name: 'programs' }).click();
  await expect(page).toHaveURL(`${AWAY}/programs`);
  await expect(page).toHaveTitle('programs · away');
  await expect(page.getByRole('heading', { level: 1, name: 'programs' })).toBeAttached();
  // every program one may open, in one grid, in the order of the files
  await expect(page.locator('section.grp h2')).toHaveText(['programs 4']);
  expect(await tiles(page, 'programs 4')).toEqual([
    ['Ledger', 'program on fin-app-02'],
    ['Payroll', 'program on fin-app-02'],
    ['Reports', 'program on fin-app-02'],
    ['Mail', 'program on fin-app-02'],
  ]);
  const find = page.getByRole('searchbox', { name: 'find a program…' });
  await find.fill('pay');
  await expect(page.locator('a.m .name')).toHaveText(['Payroll']);
  await find.fill('xyz');
  await expect(page.getByRole('status')).toContainText('No program matches “xyz”.');
  await accessible(page);

  // the machines, in the groups of the files, without their programs
  await rail.getByRole('link', { name: 'machines' }).click();
  await expect(page).toHaveURL(`${AWAY}/machines`);
  await expect(page).toHaveTitle('machines · away');
  await expect(page.locator('section.grp h2')).toHaveText(['engineering 3', 'design 1', 'finance 2', 'other machines 1']);
  expect(await tiles(page, 'finance 2')).toEqual([
    ['fin-app-02', 'Windows desktop, 4 programs'],
    ['hr-app-01', 'Windows desktop'],
  ]);
  await page.getByRole('searchbox', { name: 'find a machine…' }).fill('mac');
  await expect(page.locator('a.m .name')).toHaveText(['mac-design-01']);
  await accessible(page);
  await page.getByRole('link', { name: /^mac-design-01/ }).click();
  await expect(page).toHaveURL(`${AWAY}/machines/mac-design-01`);

  // someone who is in staff alone: the programs and the machines the one rule gives
  await service({ person: 'person' });
  await page.goto('/sign-in?to=/machines');
  await expect(page.locator('section.grp h2')).toHaveText(['finance 2', 'other machines 1']);
  await page.goto('/programs');
  await expect(page.locator('section.grp h2')).toHaveText(['programs 4']);
});

test('those who administer see what is in force and the file each thing comes from', async ({ page }) => {
  await page.goto('/administration');
  await expect(page.getByRole('tab', { name: 'machines' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByText(/^In force since .+, when the files were last applied\. To change a machine, change its file\.$/)).toBeVisible();
  const machines = page.getByRole('table', { name: 'machines' });
  // the seven machines of the files, and the two that the farm publishes
  await expect(machines.getByRole('row')).toHaveCount(10);
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
  // in the files' order: the farm's file comes before the folder of rules
  await expect(rules.getByRole('row')).toHaveText([/rule\s*who\s*reaches\s*file/, /farm-users-published\s*farm-users\s*published\s*farm\.yaml:6/, /staff-finance\s*staff\s*finance, Desktops\s*rules\/access\.yaml:4/, /admins-engineering\s*away-admins\s*engineering, mac-design-01\s*rules\/access\.yaml:8/]);
  await expect(page.getByText('To change a rule, change its file.')).toBeVisible();
  await accessible(page);
  await page.reload();
  await expect(page.getByRole('tab', { name: 'rules' })).toHaveAttribute('aria-selected', 'true');
});

// Tells the stand-in for a farm what to publish from now on.
async function farm(told: Record<string, string>) {
  const answer = await fetch(`${FARM}/stand-in?${new URLSearchParams(told)}`, { method: 'POST' });
  expect(answer.ok).toBe(true);
}

test('a farm says itself what it publishes, and a new program shows without anybody touching a file', async ({ page }) => {
  // someone a rule gives what the farm publishes, and the finance machines as everyone of staff
  await service({ person: 'of the farm' });
  await page.goto('/programs');
  await expect(page.getByRole('button', { name: 'Dana Kim, your account' })).toBeVisible();
  const count = page.locator('section.grp h2');
  await expect(count).toHaveText(['programs 6']);
  // what the farm publishes comes where the farm is written: its file is the first
  expect((await tiles(page, 'programs 6')).slice(0, 2)).toEqual([
    ['Calculator', 'program on Work Resources'],
    ['Notepad', 'program on Work Resources'],
  ]);
  // a program's icon is the farm's own, and Away serves it
  const icon = page.getByRole('link', { name: /^Calculator/ }).locator('img.icon');
  await expect(icon).toHaveAttribute('src', /^\/api\/icons\/[0-9a-f]{32}\.png$/);
  expect(await icon.evaluate((picture: HTMLImageElement) => picture.complete && picture.naturalWidth)).toBe(32);

  try {
    // the farm publishes a program more: Away shows it by itself
    await farm({ publishes: 'Paint', collection: 'Tools' });
    await expect(async () => {
      await page.reload();
      await expect(count).toHaveText(['programs 7'], { timeout: 1000 });
    }).toPass({ timeout: 20_000 });
    await expect(page.getByRole('link', { name: /^Paint/ })).toContainText('program on Work Resources');
  } finally {
    await farm({ withdraws: 'Paint' });
  }
  // and what the farm publishes no longer is gone
  await expect(async () => {
    await page.reload();
    await expect(count).toHaveText(['programs 6'], { timeout: 1000 });
  }).toPass({ timeout: 20_000 });

  // the farm's desktop is a machine, and its programs are those of a machine with the farm's name
  await page.goto('/machines');
  await expect(page.locator('section.grp h2')).toHaveText(['published 2', 'finance 2', 'other machines 1']);
  expect(await tiles(page, 'published 2')).toEqual([
    ['Farm Desktop', 'Windows desktop'],
    ['Work Resources', '2 programs on Windows'],
  ]);
  await page.getByRole('link', { name: /^Work Resources/ }).click();
  await expect(page.getByRole('region', { name: 'about this machine' })).toContainText('CollectionTools');
  await accessible(page);
});

test('those who administer see the farms, and what keeps a feed from being read', async ({ page }) => {
  await page.goto('/administration');
  await page.getByRole('tab', { name: 'farms' }).click();
  await expect(page).toHaveURL(`${AWAY}/administration/farms`);
  await expect(page.getByText('A farm says itself what it publishes: Away asks its feed every few minutes.')).toBeVisible();
  const farms = page.getByRole('table', { name: 'farms' });
  await expect(farms.getByRole('row').nth(0)).toHaveText(/farm\s*feed\s*groups\s*publishes\s*last read\s*file/);
  const row = farms.getByRole('row', { name: /^Work Resources/ });
  await expect(row).toContainText(`${FARM}/RDWeb/Feed/webfeed.aspx`);
  await expect(row).toContainText('published');
  await expect(row).toContainText('1 desktop, 2 programs');
  await expect(row).toContainText('farm.yaml:2');
  await accessible(page);

  // what the farm publishes is in force like the machines of the files
  await page.getByRole('tab', { name: 'machines' }).click();
  await expect(page.getByRole('table', { name: 'machines' }).getByRole('row', { name: /^Farm Desktop/ })).toContainText('farm-users-published gives it to farm-users');

  try {
    // the farm answers no longer: what it published stays, and the reason shows
    await farm({ answers: 'false' });
    const failed = page.getByRole('status').filter({ hasText: 'The feed of Work Resources could not be read.' });
    await expect(async () => {
      await page.reload();
      await expect(failed).toBeVisible({ timeout: 1000 });
    }).toPass({ timeout: 20_000 });
    await expect(failed).toContainText("The farm's feed answered with the status 503. What it published before stays.");
    await expect(page.getByRole('table', { name: 'machines' }).getByRole('row', { name: /^Farm Desktop/ })).toHaveCount(1);
    await accessible(page);
  } finally {
    await farm({ answers: 'true' });
  }
  await expect(async () => {
    await page.reload();
    await expect(page.getByRole('status').filter({ hasText: 'could not be read' })).toHaveCount(0, { timeout: 1000 });
  }).toPass({ timeout: 20_000 });
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
    await expect(refused).toContainText('In line 24, “prot” is not something a machine has. What was in force before stays.');
    await expect(page.getByRole('table', { name: 'machines' }).getByRole('row')).toHaveCount(10);
    await accessible(page);
    // and people go on seeing what they saw
    await page.goto('/');
    await expect(page.locator('section.grp h2')).toHaveText(['engineering 3', 'design 1', 'finance 6', 'other machines 1']);

    // the mistake put right: the machine is there, and nothing is refused any more
    writeFileSync(file, `${before}\n  - name: fin-app-03\n    kind: windows\n    address: fin-app-03.corp.example\n    groups: [finance]\n    desktop: true\n`);
    await expect(async () => {
      await page.reload();
      await expect(page.locator('section.grp h2')).toHaveText(['engineering 3', 'design 1', 'finance 7', 'other machines 1'], { timeout: 1000 });
    }).toPass({ timeout: 20_000 });
    await page.goto('/administration');
    await expect(page.getByRole('table', { name: 'machines' }).getByRole('row')).toHaveCount(11);
    await expect(refused).toHaveCount(0);
  } finally {
    writeFileSync(file, before);
  }
  // as it was before, for the tests that follow
  await expect(async () => {
    await page.goto('/');
    await expect(page.locator('section.grp h2')).toHaveText(['engineering 3', 'design 1', 'finance 6', 'other machines 1'], { timeout: 1000 });
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
  for (let stop = 0; stop < 9; stop += 1) {
    await page.keyboard.press('Tab');
    order.push(await focused(page));
  }
  expect(order).toEqual(['skip to the page', 'away, home', 'AU, Alex Urban, your account', 'home', 'programs', 'machines', 'sessions', 'administration', 'settings']);
  // a view under the keyboard's focus says its name
  await expect(page.locator('.rail .it-settings + .nc-tooltip__bubble')).toBeVisible();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${AWAY}/settings`);

  // the link that skips to the page
  await page.goto('/');
  await expect(page).toHaveTitle('home · away');
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
  await expect(views.getByRole('link')).toHaveText(['', '', '', '', '']);
  const tabs = await views.getByRole('link').evaluateAll((links) =>
    links.map((link) => {
      const box = link.getBoundingClientRect();
      return { name: link.getAttribute('aria-label'), height: box.height, bottom: Math.round(box.bottom), current: link.getAttribute('aria-current') };
    }),
  );
  expect(tabs.map((tab) => tab.name)).toEqual(['home', 'programs', 'machines', 'sessions', 'administration']);
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
  await expect(views.getByRole('link')).toHaveCount(5);
  // every view is on the screen, also on a low one
  await page.setViewportSize({ width: 700, height: 320 });
  for (const link of await views.getByRole('link').all()) await expect(link).toBeInViewport({ ratio: 1 });
  await accessible(page);
});

test('someone who administers nothing has no administration', async ({ page }) => {
  await service({ person: 'person' });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Robin Meyer, your account' }).locator('.av')).toHaveText('RM');
  const rail = page.getByRole('navigation', { name: 'views' });
  await expect(rail.getByRole('link')).toHaveCount(5);
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

test.describe('with a database', () => {
  test.skip(!DATABASE, 'no database to test against: AWAY_TEST_DATABASE is not set');

  test('what a person stars is on home, in the order they starred it, on every browser they sign in with', async ({ page, browser }) => {
    const seen = watch(page);
    // nothing is starred yet: home says where the programs and the machines are
    await page.goto(`${AWAY_THAT_KEEPS}/`);
    await expect(page.getByRole('heading', { name: 'nothing on home yet' })).toBeVisible();
    await expect(page.getByText('What you star is here. Every program and every machine you can reach is on its own page.')).toBeVisible();
    await accessible(page);
    await page.locator('.fill').getByRole('link', { name: 'programs' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/programs`);

    // a star is a button beside the tile's link: with the pointer, and with the keyboard
    const ledger = page.getByRole('button', { name: 'add to favourites: Ledger' });
    await expect(ledger).toHaveAttribute('aria-pressed', 'false');
    await expect(ledger).toHaveAttribute('title', 'add to favourites');
    await ledger.click();
    const starred = page.getByRole('button', { name: 'remove from favourites: Ledger' });
    await expect(starred).toHaveAttribute('aria-pressed', 'true');
    await page.getByRole('button', { name: 'add to favourites: Mail' }).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('button', { name: 'remove from favourites: Mail' })).toBeVisible();
    await accessible(page);

    // a machine is starred on its own page, beside its name, and a program there too
    await page.goto(`${AWAY_THAT_KEEPS}/machines`);
    await page.getByRole('link', { name: /^fin-app-02/ }).click();
    await expect(page.getByRole('button', { name: 'remove from favourites: Ledger' })).toBeVisible();
    await page.getByRole('button', { name: 'add to favourites: fin-app-02' }).click();
    await expect(page.getByRole('button', { name: 'remove from favourites: fin-app-02' })).toBeVisible();
    await accessible(page);

    // home holds them, the first starred first
    const home = [
      ['Ledger', 'program on fin-app-02'],
      ['Mail', 'program on fin-app-02'],
      ['fin-app-02', 'Windows desktop, 4 programs'],
    ];
    await page.goto(`${AWAY_THAT_KEEPS}/`);
    await expect(page.locator('section.grp h2')).toHaveText(['favourites 3']);
    expect(await tiles(page, 'favourites 3')).toEqual(home);
    await accessible(page);
    // the field finds among them
    await page.getByRole('searchbox', { name: 'find a machine or a program…' }).fill('mail');
    await expect(page.locator('section.grp h2')).toHaveText(['favourites 1']);
    await page.getByRole('searchbox', { name: 'find a machine or a program…' }).fill('');

    // they are kept with the person: another browser that signs in has them too
    const elsewhere = await browser.newContext();
    try {
      const other = await elsewhere.newPage();
      await other.goto(`${AWAY_THAT_KEEPS}/`);
      await expect(other.locator('section.grp h2')).toHaveText(['favourites 3']);
      expect(await tiles(other, 'favourites 3')).toEqual(home);
    } finally {
      await elsewhere.close();
    }

    // a star taken away on home takes the tile with it, and the programs' page knows
    await page.getByRole('button', { name: 'remove from favourites: Ledger' }).click();
    await expect(page.locator('section.grp h2')).toHaveText(['favourites 2']);
    await page.reload();
    // the page that was loaded anew has to have drawn them before they are read
    await expect(page.locator('section.grp h2')).toHaveText(['favourites 2']);
    expect(await tiles(page, 'favourites 2')).toEqual(home.slice(1));
    await page.goto(`${AWAY_THAT_KEEPS}/programs`);
    await expect(page.getByRole('button', { name: 'add to favourites: Ledger' })).toHaveAttribute('aria-pressed', 'false');
    await expect(page.getByRole('button', { name: 'remove from favourites: Mail' })).toHaveAttribute('aria-pressed', 'true');

    // with the last star gone, home says again where everything is
    await page.getByRole('button', { name: 'remove from favourites: Mail' }).click();
    await page.goto(`${AWAY_THAT_KEEPS}/machines/fin-app-02`);
    await page.getByRole('button', { name: 'remove from favourites: fin-app-02' }).click();
    await expect(page.getByRole('button', { name: 'add to favourites: fin-app-02' })).toBeVisible();
    await page.goto(`${AWAY_THAT_KEEPS}/`);
    await expect(page.getByRole('heading', { name: 'nothing on home yet' })).toBeVisible();

    // nothing was asked of anyone but Away, and the browser complained of nothing
    expect(seen.asked.filter((request) => !request.url().startsWith(AWAY_THAT_KEEPS) && !request.url().startsWith('http://127.0.0.1:18085')).map((request) => request.url())).toEqual([]);
    expect(seen.complaints).toEqual([]);
  });
});

// A pixel of a session's screen: red, green, blue. It is read off a copy, so that the
// page's own canvas is asked nothing it is not made for.
async function pixel(page: Page, x: number, y: number): Promise<number[]> {
  return page.getByRole('application').evaluate(
    (screen: HTMLCanvasElement, [left, top]) => {
      const copy = document.createElement('canvas');
      copy.width = 1;
      copy.height = 1;
      const drawn = copy.getContext('2d', { willReadFrequently: true })!;
      drawn.drawImage(screen, left, top, 1, 1, 0, 0, 1, 1);
      return Array.from(drawn.getImageData(0, 0, 1, 1).data.slice(0, 3));
    },
    [x, y] as const,
  );
}

test.describe('a session', () => {
  test.skip(!DATABASE, 'no database to test against: AWAY_TEST_DATABASE is not set');

  test("a desktop opens: the steps, the machine's question, its screen, keys and the pointer, and its end", async ({ page }) => {
    const seen = watch(page);
    // a machine that offers its desktop and nothing else is opened by a click on its tile
    await page.goto(`${AWAY_THAT_KEEPS}/machines`);
    await page.getByRole('link', { name: /^desk-01/ }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/desk-01/desktop`);
    await expect(page).toHaveTitle('desk-01 · away');
    await expect(page.getByRole('heading', { level: 1, name: 'desk-01' })).toBeVisible();
    await expect(page.getByText('Windows desktop')).toBeVisible();

    // the steps, and the machine's question for a name and a password
    const steps = page.getByRole('list', { name: 'the steps to desk-01' }).getByRole('listitem');
    await expect(page.getByText("desk-01 asks for a name and a password. Away passes them on and doesn't keep them.")).toBeVisible();
    await expect(steps).toHaveText([/^Reaching desk-01$/, /^Checking that it is desk-01$/, /^Signing in on desk-01/, /^Waiting for its first picture$/]);
    await expect(steps.nth(2)).toHaveAttribute('aria-current', 'step');
    await expect(page.getByLabel('Name')).toBeFocused();
    await expect(page.getByRole('button', { name: 'sign in' })).toBeDisabled();
    await accessible(page);

    // a password the machine doesn't take: it is asked for again, and the name stays
    await page.getByLabel('Name').fill('CORP\\alex.urban');
    await page.getByLabel('Password').fill('another password');
    await page.getByLabel('Password').press('Enter');
    await expect(page.getByRole('alert')).toHaveText("desk-01 didn't accept that name and password.");
    await expect(page.getByLabel('Name')).toHaveValue('CORP\\alex.urban');
    await expect(page.getByLabel('Password')).toHaveValue('');
    await expect(page.getByLabel('Password')).toBeFocused();
    await page.getByLabel('Password').fill('a password that exists for these tests alone');
    await page.getByRole('button', { name: 'sign in' }).click();

    // the machine's screen, pixel for pixel, and nothing of Away's around it but the handle
    const screen = page.getByRole('application', { name: 'the desktop of desk-01' });
    await expect(screen).toBeVisible();
    await expect(screen).toBeFocused();
    await expect.poll(() => pixel(page, 10, 10)).toEqual([241, 242, 243]);
    await expect.poll(() => pixel(page, 60, 20)).toEqual([80, 60, 40]);
    expect([await pixel(page, 160, 0), await pixel(page, 160, 1)]).toEqual([
      [0, 0, 0],
      [255, 255, 255],
    ]);
    expect(await screen.evaluate((canvas: HTMLCanvasElement) => [canvas.width, canvas.height])).toEqual([200, 120]);
    await expect(page.getByRole('navigation', { name: 'views' })).toHaveCount(0);
    await accessible(page);

    // a click and a key go to the machine, and what the machine draws for them shows
    await screen.click({ position: { x: 50, y: 60 } });
    await expect.poll(() => pixel(page, 55, 65)).toEqual([224, 64, 32]);
    await page.keyboard.press('a');
    await expect.poll(() => pixel(page, 5, 5)).toEqual([85, 225, 30]);

    // the session's bar: by its handle, and by Ctrl + Alt + Shift pressed and let go alone
    await page.getByRole('button', { name: "open the session's bar" }).click();
    const bar = page.getByRole('toolbar', { name: "the session's bar" });
    // a desktop has the bar across its top, with no rail: the machine has its own taskbar
    await expect(bar).toContainText('desk-01');
    await expect(page.locator('.srail')).toHaveCount(0);
    expect(await bar.getByRole('button').evaluateAll((buttons) => buttons.map((button) => button.getAttribute('aria-label')))).toEqual([
      'keys',
      'clipboard',
      'fill the screen',
      'end',
      'close the bar',
    ]);
    await accessible(page);

    // the keys a browser keeps for itself: a menu under their button, walked with the
    // arrows; Esc closes the menu and leaves the bar
    const keys = bar.getByRole('button', { name: 'keys' });
    await expect(keys).toBeFocused();
    await expect(keys).toHaveAttribute('aria-expanded', 'false');
    await keys.click();
    const kept = page.getByRole('menu', { name: 'keys' });
    await expect(kept.getByRole('menuitem')).toHaveText(['ctrl + alt + del', 'alt + tab', 'alt + f4', 'windows key', 'ctrl + w']);
    await expect(keys).toHaveAttribute('aria-expanded', 'true');
    await expect(kept.getByRole('menuitem', { name: 'ctrl + alt + del' })).toBeFocused();
    await accessible(page);
    await page.keyboard.press('ArrowUp');
    await expect(kept.getByRole('menuitem', { name: 'ctrl + w' })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(kept).toHaveCount(0);
    await expect(bar).toBeVisible();
    await expect(keys).toBeFocused();
    // one click sends the keys, in their order: the last to go down is Del, and the
    // machine has the window again
    await keys.click();
    await kept.getByRole('menuitem', { name: 'ctrl + alt + del' }).click();
    await expect(bar).toHaveCount(0);
    await expect(screen).toBeFocused();
    await expect.poll(() => pixel(page, 5, 5)).toEqual([85, 172, 83]);
    await page.getByRole('button', { name: "open the session's bar" }).click();
    await bar.getByRole('button', { name: 'keys' }).click();
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await expect.poll(() => pixel(page, 5, 5)).toEqual([85, 238, 17]);

    await page.getByRole('button', { name: "open the session's bar" }).click();
    await page.keyboard.press('Escape');
    await expect(bar).toHaveCount(0);
    await expect(screen).toBeFocused();

    // the clipboard: this browser lets no page read it, so the bar's panel has a field
    // for text to the machine and one for text from it
    await page.getByRole('button', { name: "open the session's bar" }).click();
    await bar.getByRole('button', { name: 'clipboard' }).click();
    const panel = page.getByRole('group', { name: 'clipboard' });
    await expect(panel).toContainText("This browser doesn't pass its clipboard on by itself. Paste here what desk-01 should have in its clipboard.");
    await expect(panel.getByRole('button', { name: 'send' })).toBeDisabled();
    await expect(panel.getByRole('button', { name: 'copy' })).toBeDisabled();
    await accessible(page);
    await panel.getByLabel('To the machine').fill('from the page');
    await panel.getByRole('button', { name: 'send' }).click();
    await page.keyboard.press('Escape');
    await expect(bar).toHaveCount(0);
    // the stand-in copies what it was handed, in capitals, when the key C goes down:
    // that is what the panel then holds from the machine
    await page.keyboard.press('c');
    await page.getByRole('button', { name: "open the session's bar" }).click();
    await bar.getByRole('button', { name: 'clipboard' }).click();
    await expect(panel.getByLabel('From the machine')).toHaveValue('FROM THE PAGE');
    await expect(panel.getByRole('button', { name: 'copy' })).toBeEnabled();
    await page.keyboard.press('Escape');
    await expect(bar).toHaveCount(0);

    // the desktop follows the window: at another size the machine offers its desktop
    // anew, and draws all of it; a key reaches it as before
    await page.setViewportSize({ width: 700, height: 500 });
    await expect.poll(() => screen.evaluate((canvas: HTMLCanvasElement) => [canvas.width, canvas.height])).toEqual([700, 500]);
    await expect.poll(() => pixel(page, 10, 20)).toEqual([30, 100, 70]);
    await expect.poll(() => pixel(page, 690, 490)).toEqual([156, 146, 222]);
    await page.keyboard.press('b');
    await expect.poll(() => pixel(page, 5, 5)).toEqual([85, 207, 48]);
    // and once more, to a larger one: all of that picture is drawn before the session goes on
    await page.setViewportSize({ width: 800, height: 600 });
    await expect.poll(() => screen.evaluate((canvas: HTMLCanvasElement) => [canvas.width, canvas.height])).toEqual([800, 600]);
    await expect.poll(() => pixel(page, 10, 20)).toEqual([30, 100, 70]);
    await expect.poll(() => pixel(page, 790, 590)).toEqual([100, 134, 154]);
    for (const key of ['Control', 'Alt', 'Shift']) await page.keyboard.down(key);
    for (const key of ['Shift', 'Alt', 'Control']) await page.keyboard.up(key);
    await expect(bar).toBeVisible();

    // ending it leads back to the machine
    await bar.getByRole('button', { name: 'end' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/desk-01`);

    // nothing was asked of anyone but Away, and the browser complained of nothing
    const elsewhere = seen.asked.map((request) => request.url()).filter((url) => !url.startsWith(AWAY_THAT_KEEPS) && !url.startsWith('http://127.0.0.1:18085'));
    expect(elsewhere).toEqual([]);
    expect(seen.complaints).toEqual([]);
  });

  test('a browser that lets the page read its clipboard passes it on by itself, both ways', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await page.goto(`${AWAY_THAT_KEEPS}/machines/desk-01/desktop`);
    await page.getByLabel('Name').fill('CORP\\alex.urban');
    await page.getByLabel('Password').fill('a password that exists for these tests alone');
    await page.evaluate(() => navigator.clipboard.writeText(''));
    await page.getByRole('button', { name: 'sign in' }).click();
    const screen = page.getByRole('application', { name: 'the desktop of desk-01' });
    await expect(screen).toBeFocused();
    await expect.poll(() => pixel(page, 10, 10)).toEqual([241, 242, 243]);

    // what is copied on the machine, on the key C, is in the person's clipboard
    await page.keyboard.press('c');
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe('copied on the machine\nin two lines');
    // what the person copies is told to the machine when its screen gets the focus
    // again: the stand-in copies it back in capitals
    await page.getByRole('button', { name: "open the session's bar" }).click();
    const bar = page.getByRole('toolbar', { name: "the session's bar" });
    await bar.getByRole('button', { name: 'clipboard' }).click();
    await expect(page.getByRole('group', { name: 'clipboard' })).toHaveText('Text you copy on one side can be pasted on the other.');
    await accessible(page);
    await page.evaluate(() => navigator.clipboard.writeText('copied by the person'));
    await page.keyboard.press('Escape');
    await expect(screen).toBeFocused();
    await expect
      .poll(async () => {
        await page.keyboard.press('c');
        return page.evaluate(() => navigator.clipboard.readText());
      })
      .toBe('COPIED BY THE PERSON');

    await page.getByRole('button', { name: "open the session's bar" }).click();
    await bar.getByRole('button', { name: 'end' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/desk-01`);
  });

  test('a desktop whose connection drops is opened again by itself, and a card says so after two seconds', async ({ page }) => {
    // the session's connection goes through the test, so that the test can cut it and
    // say what a gateway says when it stops
    const connections: { cut: () => void; says: (text: string) => void }[] = [];
    await page.routeWebSocket(/\/session$/, (browser) => {
      const gateway = browser.connectToServer();
      connections.push({
        cut: () => {
          browser.close();
          gateway.close();
        },
        says: (text) => browser.send(text),
      });
    });
    const signIn = async () => {
      await page.getByLabel('Name').fill('CORP\\alex.urban');
      await page.getByLabel('Password').fill('a password that exists for these tests alone');
      await page.getByRole('button', { name: 'sign in' }).click();
    };
    await page.goto(`${AWAY_THAT_KEEPS}/machines/desk-01/desktop`);
    await signIn();
    const screen = page.getByRole('application', { name: 'the desktop of desk-01' });
    await expect(screen).toBeFocused();
    await expect.poll(() => pixel(page, 10, 10)).toEqual([241, 242, 243]);
    const card = page.getByRole('alert');
    await expect(card).toHaveCount(0);

    // the connection drops while Away is not there: the page tries again and again, and
    // after two seconds a card over the last picture says so
    let tried = 0;
    await page.route('**/api/sessions?*', (route) => {
      tried += 1;
      return route.fulfill({ status: 503, body: 'not now' });
    });
    connections[0]!.cut();
    await expect(card.getByRole('heading', { name: 'connection lost' })).toBeVisible();
    await expect(card).toContainText(/Trying again, for the (second|third|fourth) time\. Your desktop is still running on desk-01\./);
    await expect(screen).toBeVisible();
    await accessible(page);
    // try now doesn't wait for the next try: Away is asked at once
    const before = tried;
    await card.getByRole('button', { name: 'try now' }).click();
    await expect.poll(() => tried).toBeGreaterThan(before);
    // once Away is there again the desktop comes back: a machine where people type asks again
    await page.unroute('**/api/sessions?*');
    await card.getByRole('button', { name: 'try now' }).click();
    await expect(page.getByText('desk-01 asks for a name and a password')).toBeVisible({ timeout: 20000 });
    await signIn();
    await expect(screen).toBeFocused();
    await expect(card).toHaveCount(0);
    await expect.poll(() => pixel(page, 10, 10)).toEqual([241, 242, 243]);

    // a gateway that stops, as when Away is updated, says so: the card says it too, and
    // leave gives up and shows the machine's page
    await page.route('**/api/sessions?*', (route) => route.fulfill({ status: 503, body: 'not now' }));
    connections[connections.length - 1]!.says('{"ended":"updating"}');
    await expect(card).toContainText('Away is being updated. Your desktop comes back by itself.');
    await card.getByRole('button', { name: 'leave' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/desk-01`);
  });

  test('a desktop that is opened in another tab is there, and the tab it was in says where it went', async ({ page, context }) => {
    const open = async (tab: Page) => {
      await tab.getByLabel('Name').fill('CORP\\alex.urban');
      await tab.getByLabel('Password').fill('a password that exists for these tests alone');
      await tab.getByRole('button', { name: 'sign in' }).click();
      await expect(tab.getByRole('application', { name: 'the desktop of desk-01' })).toBeVisible();
    };
    await page.goto(`${AWAY_THAT_KEEPS}/machines/desk-01/desktop`);
    await open(page);
    // the same desktop is opened in another tab, as on another device: it is there now
    const other = await context.newPage();
    await other.goto(`${AWAY_THAT_KEEPS}/machines/desk-01/desktop`);
    await open(other);
    await expect(page.getByRole('heading', { name: 'this desktop is open somewhere else' })).toBeVisible();
    await expect(page.getByText('You opened desk-01 in another window or on another device. A desktop is in one place at a time.')).toBeVisible();
    await expect(page.getByRole('application')).toHaveCount(0);
    await accessible(page);

    // bring it here: the machine asks, as where people type, and the other tab says where the desktop went
    await page.getByRole('button', { name: 'bring it here' }).click();
    await open(page);
    await expect(other.getByRole('heading', { name: 'this desktop is open somewhere else' })).toBeVisible();
    await expect(other.getByRole('button', { name: 'bring it here' })).toBeVisible();
    await other.close();
    await page.getByRole('button', { name: "open the session's bar" }).click();
    await page.getByRole('toolbar', { name: "the session's bar" }).getByRole('button', { name: 'end' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/desk-01`);
  });

  test("a farm's desktop opens on the machine its broker names, and a broker that names another machine is not followed", async ({ page }) => {
    const seen = watch(page);
    // a farm's desktop is opened by a click on its tile, as a machine's is
    await page.goto(`${AWAY_THAT_KEEPS}/machines`);
    await page.getByRole('link', { name: /^Farm Desk/ }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/Farm%20Desk/desktop`);
    await expect(page.getByText("Farm Desk asks for a name and a password. Away passes them on and doesn't keep them.")).toBeVisible();
    const steps = page.getByRole('list', { name: 'the steps to Farm Desk' }).getByRole('listitem');
    await expect(steps).toHaveText([/^Reaching Farm Desk$/, /^Checking that it is Farm Desk$/, /^Signing in on Farm Desk/, /^Waiting for its first picture$/]);
    await page.getByLabel('Name').fill('CORP\\alex.urban');
    await page.getByLabel('Password').fill('a password that exists for these tests alone');
    await page.getByRole('button', { name: 'sign in' }).click();

    // the screen is that of the machine the broker named, and it takes the pointer
    const screen = page.getByRole('application', { name: 'the desktop of Farm Desk' });
    await expect(screen).toBeVisible();
    await expect.poll(() => pixel(page, 10, 10)).toEqual([241, 242, 243]);
    await expect.poll(() => pixel(page, 60, 20)).toEqual([80, 60, 40]);
    await screen.click({ position: { x: 50, y: 60 } });
    await expect.poll(() => pixel(page, 55, 65)).toEqual([224, 64, 32]);
    await page.getByRole('button', { name: "open the session's bar" }).click();
    await page.getByRole('toolbar', { name: "the session's bar" }).getByRole('button', { name: 'end' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/Farm%20Desk`);
    // its page offers the desktop, and says nothing of what Away doesn't open
    await expect(page.getByRole('link', { name: 'Windows desktop' })).toBeVisible();
    await expect(page.getByRole('region', { name: 'about this machine' })).toContainText('CollectionDesks');
    await expect(page.getByText("It doesn't open programs, shells or other desktops yet.")).toHaveCount(0);

    // a broker that sends Away on to a machine which is not among those named for the collection
    await page.goto(`${AWAY_THAT_KEEPS}/machines/Lost%20Desk/desktop`);
    await page.getByLabel('Name').fill('CORP\\alex.urban');
    await page.getByLabel('Password').fill('a password that exists for these tests alone');
    await page.getByRole('button', { name: 'sign in' }).click();
    await expect(page.getByRole('alert')).toHaveText("Its farm sent Away on to a machine that isn't among those named for it, so Away doesn't connect there.");
    await expect(page.getByRole('button', { name: 'try again' })).toHaveCount(0);
    await page.getByText('details', { exact: true }).click();
    await expect(page.locator('.dtl p')).toContainText("which the files don't name for the collection");
    await accessible(page);
    expect(seen.complaints).toEqual([]);
  });

  test("a program opens in a window of its own on Away's ground, beside a rail that is its taskbar", async ({ page }) => {
    const seen = watch(page);
    // a program is opened by a click on it
    await page.goto(`${AWAY_THAT_KEEPS}/programs`);
    await page.getByRole('link', { name: /^Books/ }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/apps-01/programs/Books`);
    await expect(page).toHaveTitle('Books · away');
    await expect(page.getByRole('heading', { level: 1, name: 'Books' })).toBeVisible();
    await expect(page.getByText('program on apps-01')).toBeVisible();
    await expect(page.getByText("apps-01 asks for a name and a password. Away passes them on and doesn't keep them.")).toBeVisible();
    await accessible(page);
    await page.getByLabel('Name').fill('CORP\\alex.urban');
    await page.getByLabel('Password').fill('a password that exists for these tests alone');
    await page.getByRole('button', { name: 'sign in' }).click();

    // the window is where the machine has it on its desktop, and what is around it is not the machine's
    const screen = page.getByRole('application', { name: 'the windows of Books' });
    await expect(screen).toBeVisible();
    await expect.poll(() => pixel(page, 25, 15)).toEqual([241, 242, 243]);
    await expect.poll(() => pixel(page, 175, 105)).not.toEqual([0, 0, 0]);
    expect([await pixel(page, 5, 5), await pixel(page, 190, 115)]).toEqual([
      [0, 0, 0],
      [0, 0, 0],
    ]);
    const part = page.locator('#seen-parts rect');
    await expect(part).toHaveCount(1);
    expect(await part.evaluate((rect) => ['x', 'y', 'width', 'height'].map((name) => Number(rect.getAttribute(name)).toFixed(3)))).toEqual(['0.100', '0.083', '0.800', '0.833']);
    // only the window takes the pointer: beside it is Away's ground
    const hit = (x: number, y: number) =>
      screen.evaluate((canvas: HTMLCanvasElement, [x, y]) => {
        const shown = canvas.getBoundingClientRect();
        return document.elementFromPoint(shown.left + x!, shown.top + y!) === canvas;
      }, [x, y]);
    expect([await hit(25, 15), await hit(5, 5)]).toEqual([true, false]);
    await screen.click({ position: { x: 50, y: 60 } });
    await expect.poll(() => pixel(page, 55, 65)).toEqual([224, 64, 32]);

    // a program's rail stands always, as a taskbar: no handle to open, Away's mark at its
    // top, the windows as their own icons, each brought to the front or closed, and at its
    // foot the keyboard the machine types as and the time
    const bar = page.getByRole('toolbar', { name: "the session's bar" });
    await expect(page.getByRole('button', { name: "open the session's bar" })).toHaveCount(0);
    const mark = bar.getByRole('button', { name: 'open a program' });
    await expect(mark).toBeVisible();
    // the program's picture is the space beside the rail, not under it
    const box = await screen.boundingBox();
    expect(box && box.x).toBeGreaterThanOrEqual(56);
    const windows = bar.getByRole('list', { name: 'windows' }).getByRole('listitem');
    await expect(windows).toHaveCount(1);
    const front = windows.getByRole('button', { name: 'bring to the front: ledger' });
    await expect(front).toBeVisible();
    await expect(front).toHaveAttribute('aria-current', 'true');
    // the icon is the one the machine gave the window: blue, 32 pixels square, in a frame that covers nothing
    const icon = (x: number, y: number) =>
      front.locator('canvas').evaluate(
        (drawn: HTMLCanvasElement, [x, y]) => {
          // read from a copy, which is read once
          const copy = document.createElement('canvas');
          [copy.width, copy.height] = [drawn.width, drawn.height];
          const context = copy.getContext('2d', { willReadFrequently: true })!;
          context.drawImage(drawn, 0, 0);
          return [drawn.width, ...context.getImageData(x!, y!, 1, 1).data];
        },
        [x, y],
      );
    await expect.poll(() => icon(16, 16)).toEqual([32, 0x25, 0x63, 0xeb, 255]);
    expect(await icon(0, 0)).toEqual([32, 0, 0, 0, 0]);
    await expect(bar.locator('.lang [aria-hidden="true"]')).toHaveText('EN');
    await expect(bar.getByText('the keyboard: English (United States)')).toBeAttached();
    await expect(bar.getByLabel('the time on your device')).toHaveText(/^\d\d:\d\d/);
    // nothing in the rail is wider than the rail: it never scrolls sideways
    for (const part of [bar, bar.getByRole('list', { name: 'windows' })]) {
      expect(await part.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
    }
    await accessible(page);

    // the mark lists the machine's programs: one opens in the same session, in a window of its own
    await mark.click();
    const launcher = bar.getByRole('group', { name: 'programs of apps-01' });
    await expect(launcher.getByRole('button')).toHaveText(['Books', 'Wages', 'other machines']);
    await accessible(page);
    await launcher.getByRole('button', { name: 'Books' }).click();
    await expect(launcher).toHaveCount(0);
    await expect(windows).toHaveCount(2);
    // the windows keep the order they came in, as on a taskbar: the new one is in front, below the first
    await expect(windows.nth(1).getByRole('button')).toHaveAttribute('aria-current', 'true');
    // one brought to the front keeps its place, and the window has the keys
    await windows.nth(0).getByRole('button').click();
    await expect(windows.nth(0).getByRole('button')).toHaveAttribute('aria-current', 'true');
    await expect(windows.nth(1).getByRole('button')).not.toHaveAttribute('aria-current');
    await expect(screen).toBeFocused();
    // there is no cross to close a window: a window is closed as the program closes it
    await expect(bar.getByRole('button', { name: /^close/ })).toHaveCount(0);
    // one the machine doesn't start says so there, and the session goes on
    await mark.click();
    await launcher.getByRole('button', { name: 'Wages' }).click();
    await expect(launcher.getByRole('alert')).toHaveText("Wages didn't start on apps-01.");
    await expect(screen).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(launcher).toHaveCount(0);
    await expect(mark).toBeFocused();

    // the icons the programs put in the tray are behind a button at the rail's foot
    const trayButton = bar.getByRole('button', { name: 'the tray' });
    await trayButton.click();
    const tray = bar.getByRole('group', { name: 'the tray of apps-01' });
    await expect(tray.getByRole('button', { name: 'ledger' })).toBeVisible();
    await accessible(page);
    await tray.getByRole('button', { name: 'ledger' }).click({ button: 'right' });
    await expect(tray).toHaveCount(0);
    await expect(screen).toBeFocused();

    // the windows have the window's room: a window of another size is a desktop of that size, beside the rail
    await page.setViewportSize({ width: 1000, height: 700 });
    await expect.poll(() => screen.evaluate((canvas: HTMLCanvasElement) => [canvas.width, canvas.height]), { timeout: 10000 }).toEqual([944, 700]);

    // ending the session leaves the programs running on the machine, and leads back to it
    await bar.getByRole('button', { name: 'end' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/apps-01`, { timeout: 15000 });
    // its page offers the desktop and each program, and says nothing of what Away doesn't open
    await expect(page.getByRole('region', { name: 'programs 2' }).getByRole('link')).toHaveText(['Books', 'Wages']);
    await expect(page.getByText("It doesn't open shells or other desktops yet.")).toHaveCount(0);

    // a program the machine doesn't let be started says so, with the machine's reason behind it
    await page.getByRole('region', { name: 'programs 2' }).getByRole('link', { name: 'Wages' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/apps-01/programs/Wages`);
    await page.getByLabel('Name').fill('CORP\\alex.urban');
    await page.getByLabel('Password').fill('a password that exists for these tests alone');
    await page.getByRole('button', { name: 'sign in' }).click();
    await expect(page.getByRole('alert')).toHaveText("It took the sign-in and didn't start the program.");
    await page.getByText('details', { exact: true }).click();
    await expect(page.locator('.dtl p')).toContainText('it is not among those it publishes');
    await accessible(page);
    expect(seen.complaints).toEqual([]);
  });

  test("on a phone a program's rail waits at the left edge, and is swiped in over the picture", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`${AWAY_THAT_KEEPS}/machines/apps-01/programs/Books`);
    await page.getByLabel('Name').fill('CORP\\alex.urban');
    await page.getByLabel('Password').fill('a password that exists for these tests alone');
    await page.getByRole('button', { name: 'sign in' }).click();
    const screen = page.getByRole('application', { name: 'the windows of Books' });
    await expect(screen).toBeVisible();
    // the picture has the whole width, and the rail is not there: only the strip at the edge
    const bar = page.getByRole('toolbar', { name: "the session's bar" });
    await expect(bar).toHaveCount(0);
    const edge = page.getByRole('button', { name: "open the session's bar" });
    await expect(edge).toBeVisible();
    // nothing takes width from it: it sits in the middle of the whole width
    const box = await screen.boundingBox();
    expect(Math.abs(box!.x + box!.width / 2 - 195)).toBeLessThanOrEqual(2);
    // a swipe from the edge brings the rail in, with the mark, the window, the controls and the clock
    const at = await edge.boundingBox();
    await page.mouse.move(at!.x + 4, at!.y + at!.height / 2);
    await page.mouse.down();
    await page.mouse.move(at!.x + 140, at!.y + at!.height / 2, { steps: 8 });
    await page.mouse.up();
    await expect(bar).toBeVisible();
    await expect(bar.getByRole('button', { name: 'open a program' })).toBeFocused();
    await expect(bar.getByRole('list', { name: 'windows' }).getByRole('listitem')).toHaveCount(1);
    await expect(bar.getByLabel('the time on your device')).toBeVisible();
    await accessible(page);
    // a touch beside it puts it away again, and so does Esc after a tap on the edge
    await page.mouse.click(300, 400);
    await expect(bar).toHaveCount(0);
    await expect(edge).toBeVisible();
    await edge.click();
    await expect(bar).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(bar).toHaveCount(0);
    await edge.click();
    await bar.getByRole('button', { name: 'end' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/apps-01`, { timeout: 15000 });
  });

  test('a machine that is not there fails at its step and says why, and what Away does not open yet says so', async ({ page }) => {
    await page.goto(`${AWAY_THAT_KEEPS}/machines/gone-02/desktop`);
    const steps = page.getByRole('list', { name: 'the steps to gone-02' }).getByRole('listitem');
    await expect(page.getByRole('alert')).toHaveText('It refused the connection. Remote access may be switched off there.');
    await expect(steps.nth(0)).toContainText('Reaching gone-02');
    await expect(page.getByText('Questions go to it-help@example.com.')).toBeVisible();
    // the technical reason, for whoever is asked for help
    await page.getByText('details', { exact: true }).click();
    await expect(page.locator('.dtl p')).toContainText('refused the connection');
    await accessible(page);
    // trying again tries again, and the machines are a click away
    await page.getByRole('button', { name: 'try again' }).click();
    await expect(page.getByRole('alert')).toHaveText('It refused the connection. Remote access may be switched off there.');
    await page.getByRole('button', { name: 'machines' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines/gone-02`);

    // a shell, which this version doesn't open
    await page.goto(`${AWAY_THAT_KEEPS}/machines/build-01/desktop`);
    await expect(page.getByRole('heading', { name: "build-01 can't be opened yet" })).toBeVisible();
    await expect(page.getByText('This version of Away opens Windows desktops and programs, and nothing else yet.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'try again' })).toHaveCount(0);
    await accessible(page);
    // and a machine no rule gives is not there
    await page.goto(`${AWAY_THAT_KEEPS}/machines/nowhere/desktop`);
    await expect(page.getByRole('heading', { name: 'no machine of that name for you' })).toBeVisible();
    await page.getByRole('button', { name: 'machines' }).click();
    await expect(page).toHaveURL(`${AWAY_THAT_KEEPS}/machines`);
  });
});

test('without a database no machine is opened, and the page says why', async ({ page }) => {
  await page.goto('/machines/fin-app-02');
  // a machine that offers more than its desktop is opened from its page
  await page.getByRole('link', { name: 'Windows desktop' }).click();
  await expect(page).toHaveURL(`${AWAY}/machines/fin-app-02/desktop`);
  await expect(page.getByRole('heading', { name: "away can't open machines right now" })).toBeVisible();
  await expect(page.getByText("No machine is opened without a record, and Away can't write its records right now.")).toBeVisible();
  await accessible(page);
});

test("the session's messages are written and read as the examples say that the gateway is tried against too", async () => {
  const { begins, copied, credentials, did, leaves, readIcon, readRectangle, readSaid, readTrayPicture, seen, sized, start, trayDoes, windowDoes } = await import(
    '../src/session/wire'
  );
  const examples = JSON.parse(readFileSync('../crates/away-wire/fixtures/messages.json', 'utf8')) as Record<string, { text?: string; bytes?: string }[]>;
  const hex = (bytes: Uint8Array) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  const bytes = (written: string) => new Uint8Array((written.match(/../g) ?? []).map((pair) => parseInt(pair, 16)));

  // what the gateway says
  const said = (examples.to_browser ?? []).map((example) =>
    example.text
      ? readSaid(example.text)
      : (readRectangle(bytes(example.bytes!).buffer) ?? readIcon(bytes(example.bytes!).buffer) ?? readTrayPicture(bytes(example.bytes!).buffer)),
  );
  expect(said.slice(0, 5)).toEqual([{ step: 'reaching' }, { step: 'first-picture' }, { asks: { again: false } }, { asks: { again: true } }, { desktop: { width: 1280, height: 800 } }]);
  expect(said[5]).toEqual({ left: 64, top: 128, width: 200, height: 120, png: new Uint8Array([0x89, 0x50, 0x4e, 0x47]) });
  expect(said.slice(6, 9)).toEqual([{ drawn: 7 }, { failed: { step: 'reaching', why: 'no-answer', details: 'no answer within 10 seconds' } }, { ended: 'left' }]);
  expect(said.slice(9)).toEqual([
    { windows: [{ id: 65537, title: 'Ledger', shown: 'as-it-is', listed: true, seen: [[20, 10, 180, 110]] }], in_front: 65537 },
    { asks: { again: false, instead: true } },
    { clipboard: { to: true, from: false } },
    { copied: 'two\nlines' },
    { window: 65537, png: new Uint8Array([0x89, 0x50, 0x4e, 0x47]) },
    { started: { program: 'Wages', ok: false } },
    { tray: [{ window: 65537, icon: 1, tip: 'Ledger – 2 reminders' }] },
    { window: 65537, icon: 1, png: new Uint8Array([0x89, 0x50, 0x4e, 0x47]) },
  ]);
  expect([
    readSaid('no JSON'),
    readSaid('7'),
    readRectangle(new Uint8Array([2, 0, 0, 0, 0, 0, 0, 0, 0, 1]).buffer),
    readIcon(new Uint8Array([1, 0, 0, 0, 0, 0, 0, 0, 0, 1]).buffer),
    readIcon(new Uint8Array([2, 0, 1]).buffer),
    readTrayPicture(new Uint8Array([2, 0, 1, 0, 1, 0, 0, 0, 1, 0x89]).buffer),
  ]).toEqual([null, null, null, null, null, null]);

  // what the pages say
  const written = [
    begins('7f3a', 1280, 800, 2055, 200),
    credentials('CORP\\alex.urban', 'a "password"'),
    seen(7),
    leaves(),
    hex(did([{ moved: { x: 640, y: 400 } }])),
    hex(
      did([
        { button: { button: 0, down: true, x: 640, y: 400 } },
        { button: { button: 2, down: false, x: 1, y: 2 } },
        { wheel: { by: -120, sideways: false, x: 3, y: 4 } },
        { wheel: { by: 120, sideways: true, x: 3, y: 4 } },
        { key: { code: 0x1e, extended: false, down: true } },
        { key: { code: 0x4d, extended: true, down: false } },
        { character: { unit: 0x20ac, down: true } },
        { locks: { scroll: false, num: true, caps: true } },
      ]),
    ),
    windowDoes(65537, 'close'),
    sized(1440, 900, 200),
    copied('a text\nof two lines'),
    start('Wages'),
    trayDoes(65537, 1, 'menu'),
  ];
  expect(written).toEqual((examples.from_browser ?? []).map((example) => example.text ?? example.bytes));
});

test('without a database nothing can be starred, and the pages show no stars', async ({ page }) => {
  await page.goto('/programs');
  await expect(page.locator('section.grp h2')).toHaveText(['programs 4']);
  await expect(page.getByRole('button', { name: /favourites/ })).toHaveCount(0);
  await page.goto('/machines/fin-app-02');
  await expect(page.getByRole('heading', { name: 'fin-app-02' })).toBeVisible();
  await expect(page.getByRole('button', { name: /favourites/ })).toHaveCount(0);
});
