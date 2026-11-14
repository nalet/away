// The test against a real sign-in service, which tests/machines/sign-in/real.sh starts
// and names. The browser takes the service's certificate as it is: whether Away's server
// trusts it is what the test is about, and the server is told the authority.
import { defineConfig, devices } from '@playwright/test';

const told = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};

export default defineConfig({
  testDir: 'real',
  workers: 1,
  timeout: 240_000,
  reporter: told.CI ? [['list'], ['github']] : 'list',
  use: { baseURL: told.AWAY_AT, ignoreHTTPSErrors: true, trace: 'retain-on-failure' },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 800 } } }],
});
