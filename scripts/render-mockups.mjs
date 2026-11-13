#!/usr/bin/env node
// Renders the specification's drawings: every <figure class="shot" id="…"> in an
// .html file of spec/screens/img/ (ch02-frame.html, ch04-machines.html …) becomes
// img/<id>.png beside it, at twice the size.
//
//   npx -p playwright@1.63.0 node scripts/render-mockups.mjs [chapter …] [id …]
//   (or PLAYWRIGHT=/path/to/node_modules/playwright node scripts/render-mockups.mjs ch04)
//
// With no argument it draws every figure. A word that starts a file's name (ch04,
// ch04-machines) draws that file's figures; any other word is a figure's id.
//
// Edit the drawings in the HTML, never the PNGs. The fonts come from the site's
// packages, so run `npm ci` in site/ once before.
import { createRequire } from 'node:module';
import { readdirSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const dir = path.join(root, 'spec/screens/img');
const all = readdirSync(dir).filter((name) => name.endsWith('.html')).sort();
const words = process.argv.slice(2);
const files = words.filter((w) => all.some((name) => name.startsWith(w)));
const ids = words.filter((w) => !files.includes(w));

const browser = await chromium.launch();
let problems = 0;
try {
  // wide enough for a whole computer screen (1280) with its frame and notes
  const page = await browser.newPage({ deviceScaleFactor: 2, viewport: { width: 1600, height: 1000 } });
  // a drawing's own errors (a missing icon, a <ds-*> tag that didn't expand, a mark with no part) are printed
  let current = '';
  page.on('console', (msg) => { if (msg.type() === 'error') { problems++; console.log(`  ! ${current}: ${msg.text()}`); } });
  page.on('pageerror', (err) => { problems++; console.log(`  ! ${current}: ${err.message}`); });
  const rendered = new Map(); // id → the file it was rendered from: the first file wins
  for (const html of all) {
    if (files.length && !files.some((w) => html.startsWith(w))) continue;
    current = html;
    await page.goto(pathToFileURL(path.join(dir, html)).href, { waitUntil: 'networkidle' });
    await page.evaluate(() => document.fonts.ready);
    const missing = await page.evaluate(() =>
      ['Inter', 'JetBrains Mono'].filter((f) => !document.fonts.check(`16px "${f}"`)));
    if (missing.length) throw new Error(`${html}: fonts did not load: ${missing.join(', ')} (run npm ci in site/)`);
    for (const id of await page.$$eval('figure.shot', (els) => els.map((el) => el.id))) {
      if (ids.length && !ids.includes(id)) continue;
      if (rendered.has(id)) { problems++; console.log(`  ! ${html}: figure "${id}" is also in ${rendered.get(id)}, which wins`); continue; }
      rendered.set(id, html);
      const file = path.join(dir, `${id}.png`);
      await page.locator(`#${id}`).screenshot({ path: file });
      console.log(`  ${path.relative(root, file)}`);
    }
  }
  console.log(`${rendered.size} figures${problems ? `, ${problems} problems` : ''}`);
} finally {
  await browser.close();
}
process.exit(problems ? 1 : 0);
