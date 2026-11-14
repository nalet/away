// The pages' tests: a real browser goes through Away as a person does. The server runs
// beside a stand-in for a sign-in service, which the tests tell how to behave; a second
// server is told of a sign-in service that isn't there.
//
//   cargo build --package away-server --example pages --features stand-in
//   npm --prefix web test
import { defineConfig, devices } from '@playwright/test';

const told = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};
const inThePipeline = Boolean(told.CI);
const server = '../target/debug/examples/pages';
// the machines and the rules of the tests, as a copy that a test may change
export const FILES = '../target/pages-test-files';
const fixtures = '../crates/away-server/tests/fixtures/files';
const withFiles = `rm -rf ${FILES} && cp -R ${fixtures} ${FILES} && exec ${server}`;
// a PostgreSQL for what Away keeps, as host:port, where there is one to test against:
// tests/machines/database/real.sh starts it. The pipeline always has one.
export const DATABASE = told.AWAY_TEST_DATABASE ?? '';
if (inThePipeline && !DATABASE) throw new Error("The pipeline's tests need a database: AWAY_TEST_DATABASE is not set.");

export const AWAY = 'http://127.0.0.1:18080';
export const SIGN_IN_SERVICE = 'http://127.0.0.1:18081';
export const AWAY_WITHOUT_SERVICE = 'http://127.0.0.1:18082';
// a stand-in for a farm's web access, which the server asks what it publishes
export const FARM = 'http://127.0.0.1:18083';
// a third server, which is given a database, and so keeps what people star
export const AWAY_THAT_KEEPS = 'http://127.0.0.1:18084';
// beside it a stand-in for a Windows machine, and the gateway: a desktop opens there
const FILES_KEPT = '../target/pages-test-files-kept';
const keeps = {
  command: `rm -rf ${FILES_KEPT} && cp -R ${fixtures} ${FILES_KEPT} && exec ${server}`,
  env: { PORT: '18084', SIGN_IN_PORT: '18085', DATABASE, MACHINE_PORT: '18086', FILES: FILES_KEPT },
  url: `${AWAY_THAT_KEEPS}/api/who`,
  reuseExistingServer: !inThePipeline,
};

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
    { command: withFiles, env: { PORT: '18080', SIGN_IN_PORT: '18081', FARM_PORT: '18083', FILES }, url: `${AWAY}/api/who`, reuseExistingServer: !inThePipeline },
    { command: server, env: { PORT: '18082', SIGN_IN: 'absent' }, url: `${AWAY_WITHOUT_SERVICE}/api/who`, reuseExistingServer: !inThePipeline },
    ...(DATABASE ? [keeps] : []),
  ],
});
