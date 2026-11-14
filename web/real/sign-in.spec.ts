// Signing in through a real sign-in service, as a person does: by typing a name and a
// password on the service's own page. What the service says later is taken anew: a group
// that is taken away is gone in Away, and an account that is closed has to sign in again.
import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';

const told = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};
const AWAY = told.AWAY_AT ?? '';
const SERVICE = told.SERVICE_AT ?? '';
const ACCOUNT = told.ACCOUNT ?? '';

// Asks the sign-in service's own administration, as its administrator.
async function administration(path: string, how: { method?: string; body?: unknown } = {}) {
  const signedIn = await fetch(`${SERVICE}/realms/master/protocol/openid-connect/token`, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ grant_type: 'password', client_id: 'admin-cli', username: 'administrator', password: told.ADMINISTRATOR_PASSWORD ?? '' }),
  });
  expect(signedIn.ok).toBe(true);
  const { access_token: token } = (await signedIn.json()) as { access_token: string };
  const answer = await fetch(`${SERVICE}/admin/realms/staff${path}`, {
    method: how.method ?? 'GET',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: how.body === undefined ? undefined : JSON.stringify(how.body),
  });
  expect(answer.ok).toBe(true);
  return answer.status === 204 ? null : ((await answer.json()) as unknown);
}

// Types the person's name and password on the sign-in service's page.
async function type(page: Page) {
  await expect(page).toHaveURL(new RegExp(`^${SERVICE}/realms/staff/protocol/openid-connect/auth\\?`));
  await page.locator('#username').fill(ACCOUNT);
  await page.locator('#password').fill(told.ACCOUNT_PASSWORD ?? '');
  await page.locator('#kc-login').click();
}

test('a person signs in through a real sign-in service, and what it says later is taken anew', async ({ page, context }) => {
  // a link to a page of Away: the sign-in service asks who it is, and the page is where the browser ends
  await page.goto('/settings');
  await type(page);
  await expect(page).toHaveURL(`${AWAY}/settings`);
  const account = page.getByRole('region', { name: 'account' });
  await expect(account).toContainText('Alex Urban');
  await expect(account).toContainText('alex.urban@example.com');
  const rail = page.getByRole('navigation', { name: 'views' });
  await expect(rail.getByRole('link', { name: 'administration' })).toBeVisible();

  // the browser holds a cookie that names the sign-in, and nothing of the sign-in itself
  const cookies = await context.cookies(AWAY);
  expect(cookies.map((cookie) => [cookie.name, cookie.httpOnly])).toEqual([['__Host-away', true]]);
  const who = await page.evaluate(() => fetch('/api/who').then((answer) => answer.text()));
  expect(who).toContain('"administers":true');
  expect(who).not.toMatch(/eyJ|token/);

  // the person is taken out of the administrators' group at the sign-in service: with
  // the next renewal Away knows, and nothing was typed
  const [person] = (await administration(`/users?exact=true&username=${ACCOUNT}`)) as { id: string }[];
  const groups = (await administration('/groups')) as { id: string; name: string }[];
  const administrators = groups.find((group) => group.name === 'away-admins');
  expect(person && administrators).toBeTruthy();
  await administration(`/users/${person?.id}/groups/${administrators?.id}`, { method: 'DELETE' });
  await expect(async () => {
    await page.reload();
    await expect(rail.getByRole('link', { name: 'home' })).toBeVisible();
    await expect(rail.getByRole('link', { name: 'administration' })).toHaveCount(0, { timeout: 1000 });
  }).toPass({ timeout: 120_000, intervals: [3000] });
  await expect(page.getByRole('alertdialog')).toHaveCount(0);

  // signing out, and in again: the sign-in service may or may not ask once more
  await page.getByRole('button', { name: 'Alex Urban, your account' }).click();
  await page.getByRole('menuitem', { name: 'sign out' }).click();
  await expect(page).toHaveURL(`${AWAY}/?signed-out`);
  expect(await context.cookies(AWAY)).toEqual([]);
  await page.getByRole('button', { name: 'sign in' }).click();
  await page.waitForURL((address) => address.href === `${AWAY}/` || address.href.startsWith(`${SERVICE}/`));
  if (page.url().startsWith(`${SERVICE}/`)) await type(page);
  await expect(page).toHaveURL(`${AWAY}/`);
  await expect(page).toHaveTitle('home · away');

  // the account is closed at the sign-in service: the next renewal is refused, and Away
  // asks for a new sign-in over the page
  await administration(`/users/${person?.id}`, { method: 'PUT', body: { enabled: false } });
  const card = page.getByRole('alertdialog', { name: 'sign in again' });
  await expect(async () => {
    await page.reload();
    await expect(card).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 120_000, intervals: [3000] });
});
