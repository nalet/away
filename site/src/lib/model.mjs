// the model of this repository (spec/goals, spec/capabilities, spec/screens,
// spec/architecture, spec/plan), for the maps
// (src/pages/spec/) and the product page. scripts/sync.mjs has scripts/model.py write
// it, with every state worked out, to src/generated/model.json.
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { BASE } from '../../site.mjs';

const MODEL = join(process.cwd(), 'src/generated/model.json');

let model;
export function load() {
  if (model === undefined) model = existsSync(MODEL) ? index(JSON.parse(readFileSync(MODEL, 'utf8'))) : null;
  return model;
}

function index(m) {
  m.areaByKey = new Map(m.areas.map((a) => [a.key, a]));
  m.partByKey = new Map(m.parts.map((p) => [p.key, p]));
  m.capByKey = new Map(m.capabilities.map((c) => [c.key, c]));
  m.goalByKey = new Map(m.goals.map((g) => [g.key, g]));
  m.expectationByKey = new Map(m.expectations.map((e) => [e.key, e]));
  m.goalKindLabel = new Map(m.goalKinds.map((k) => [k.key, k.label]));
  for (const c of m.capabilities) {
    c.part = m.areaByKey.get(c.area).part;
    c.url = `${BASE}/spec/capabilities/${c.part}/${c.key}/`;
  }
  // the screens: a figure leads to its place in its chapter
  m.chapters ??= [];
  m.screens ??= [];
  m.chapterByKey = new Map(m.chapters.map((c) => [c.key, c]));
  m.screenByKey = new Map(m.screens.map((f) => [f.key, f]));
  for (const c of m.chapters) c.url = route(c.page);
  for (const f of m.screens) f.url = `${m.chapterByKey.get(f.chapter).url}#fig-${f.key}`;
  for (const c of m.capabilities) c.figures = (c.drawn ?? []).map((k) => m.screenByKey.get(k)).filter(Boolean);
  // how it is built: the components that carry a capability, the work packages that
  // deliver it and the decision records that govern it
  m.components ??= [];
  m.packages ??= [];
  m.decisions ??= [];
  m.componentByKey = new Map(m.components.map((c) => [c.key, c]));
  m.packageByKey = new Map(m.packages.map((p) => [p.key, p]));
  m.decisionByNo = new Map(m.decisions.map((d) => [d.adr, d]));
  for (const p of m.packages) p.url = `${BASE}/spec/plan/#${p.anchor}`;
  for (const d of m.decisions) d.url = route(d.page);
  for (const c of m.capabilities) {
    c.carriedBy = (m.carried?.[c.key] ?? []).map((k) => m.componentByKey.get(k)).filter(Boolean);
    c.builtIn = (m.builtIn?.[c.key] ?? []).map((b) => ({ ...b, pack: m.packageByKey.get(b.package) })).filter((b) => b.pack);
    c.decidedBy = (m.decides?.[c.key] ?? []).map((n) => m.decisionByNo.get(n)).filter(Boolean);
  }
  for (const g of m.goals) {
    g.url = `${BASE}/spec/goals/#${slug(g.goal)}`;
    g.neededBy = m.goals.filter((o) => o.needs.includes(g.key)).map((o) => o.key);
    g.areaList = g.areas.map((k) => m.areaByKey.get(k)).filter(Boolean);
    g.caps = m.capabilities.filter((c) => g.areas.includes(c.area));
    g.expectations = m.expectations.filter((e) => e.goal === g.key);
    g.qualities = m.qualities.filter((q) => q.goal === g.key);
  }
  return m;
}

// where a capability stands, as the maps name and colour it. "out" is the tone of what
// is left out on purpose: hollow, with a line of its own.
export const STATUS = {
  built: { name: 'built', tone: 'success' },
  partial: { name: 'partly built', tone: 'warning' },
  v1: { name: 'first release', tone: 'info' },
  later: { name: 'later', tone: 'neutral' },
  no: { name: 'not planned', tone: 'out' },
  '—': { name: 'does not apply', tone: 'none' },
};
// a status as a data attribute's value
export const statusAttr = (s) => (s === '—' ? 'none' : s);

// what the model says of an expectation
export const EXPECTED = {
  there: 'success',
  partly: 'warning',
  'first release': 'info',
  'first release in part': 'info-part',
  later: 'neutral',
  'not planned': 'out',
};

export const WHO = {
  person: 'for a person working on a machine',
  admin: 'for an administrator',
  operator: 'for whoever runs Away',
  system: 'done by Away itself',
  anyone: 'for anyone',
};

// spec/goals/README.md -> /away/spec/goals/ (as scripts/sync.mjs)
export const route = (file) => BASE + '/' + file.replace(/(^|\/)README\.md$/, '').replace(/\.md$/, '') + '/';
// the anchor a heading gets (as scripts/model.py and the site have it)
export const slug = (text) => text.toLowerCase().replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '').replaceAll(' ', '-');

export function partLink(m, partKey, areaName) {
  return route(m.partByKey.get(partKey).page) + (areaName ? '#' + slug(areaName) : '');
}
export const expectationsLink = (m, goalKey) => `${BASE}/spec/capabilities/expectations/#${slug(m.goalByKey.get(goalKey)?.goal ?? '')}`;
