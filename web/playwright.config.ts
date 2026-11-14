// The pages' tests: a real browser goes through Away as a person does. The server runs
// beside a stand-in for a sign-in service, which the tests tell how to behave; a second
// server is told of a sign-in service that isn't there.
//
//   cargo build --package away-server --example pages --features stand-in
//   npm --prefix web test
import { defineConfig, devices } from '@playwright/test';

const inThePipeline = Boolean((globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env.CI);
const server = '../target/debug/examples/pages';
// the machines and the rules of the tests, as a copy that a test may change
export const FILES = '../target/pages-test-files';
const withFiles = `rm -rf ${FILES} && cp -R ../crates/away-server/tests/fixtures/files ${FILES} && exec ${server}`;

export const AWAY = 'http://127.0.0.1:18080';
export const SIGN_IN_SERVICE = 'http://127.0.0.1:18081';
export const AWAY_WITHOUT_SERVICE = 'http://127.0.0.1:18082';

export default defineConfig({
  testDir: 'tests',
  // the tests share one sign-in service and tell it how to behave, so one runs at a time
  fullyParallel: false,
  workers: 1,
  forbidOnly: inThePipeline,
  reporter: inThePipeline ? [['list'], ['github']] : 'list',
  use: { baseURL: AWAY, trace: 'retain-on-failure' },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 800 } } }],
  webServer: [
    { command: withFiles, env: { PORT: '18080', SIGN_IN_PORT: '18081', FILES }, url: `${AWAY}/api/who`, reuseExistingServer: !inThePipeline },
    { command: server, env: { PORT: '18082', SIGN_IN: 'absent' }, url: `${AWAY_WITHOUT_SERVICE}/api/who`, reuseExistingServer: !inThePipeline },
  ],
});
