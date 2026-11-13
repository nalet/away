// copies the docs/ and spec/ folders of this repository into the site as its pages:
// docs/ is the documentation, spec/ the specification.
//
//   node scripts/sync.mjs
//
// every page keeps its path: spec/goals/README.md is /spec/goals/, docs/model.md is
// /docs/model/, and a README.md is its folder's page. the first "# heading" becomes the
// page's title, a link to another page (in either folder) becomes that page's address
// on the site, a link to any other file of the repository goes to it on GitHub, and
// images are copied next to their pages. the folders themselves are never changed.
//
// writes src/content/docs/{docs,spec}/ (the pages), src/generated/pages.json (each
// page's address and title, for the navigation) and src/generated/model.json (the
// model, from scripts/model.py, for the maps).
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, posix, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BASE, BRANCH, GITHUB } from '../site.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SOURCE = resolve(ROOT, '..');
const CONTENT = join(ROOT, 'src/content/docs');
const TREES = ['docs', 'spec'];
const IMAGE = /\.(png|svg|jpe?g|gif|webp|avif)$/i;

// spec/goals/README.md -> /spec/goals/ ; docs/model.md -> /docs/model/
const route = (path) => '/' + path.replace(/(^|\/)README\.md$/, '').replace(/\.md$/, '') + '/';
const outPath = (path) => path.replace(/(^|\/)README\.md$/, '$1index.md');

// every file of the copied folders, by its path in the repository
const files = [];
for (const tree of TREES) {
  (function walk(dir) {
    for (const name of readdirSync(dir).sort()) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) walk(path);
      else files.push(relative(SOURCE, path).split('\\').join('/'));
    }
  })(join(SOURCE, tree));
}
const known = new Set(files);
const pages = new Set(files.filter((f) => f.endsWith('.md')));
const problems = [];

// a link's target, as the site has it.
function rewrite(target, from) {
  if (/^(https?:|mailto:|#)/.test(target)) return target;
  const [path, hash = ''] = target.split(/(?=#)/);
  const resolved = posix.normalize(posix.join(posix.dirname(from), decodeURI(path)));
  if (pages.has(resolved)) return BASE + route(resolved) + hash;
  // an image is copied to the same place, so its relative link still holds
  if (IMAGE.test(resolved) && known.has(resolved)) return path.startsWith('.') ? target : './' + target;
  if (!resolved.startsWith('../') && existsSync(join(SOURCE, resolved))) {
    // another file of the repository: a CSV file, the licence, a script …
    const kind = statSync(join(SOURCE, resolved)).isDirectory() ? 'tree' : 'blob';
    return `${GITHUB}/${kind}/${BRANCH}/${resolved.replace(/\/$/, '')}${hash}`;
  }
  problems.push(`${from}: "${target}" points at nothing`);
  return target;
}

// [text](target) and ![alt](target), outside code. a link text may hold one level of [brackets].
const LINK = /(!?\[(?:[^[\]]|\[[^\]]*\])*\]\()([^)\s]+)((?:\s+"[^"]*")?\))/g;
function rewriteText(text, from) {
  // a link inside a `code span` is text: find the spans (a run of backticks up to the
  // next run of the same length), and leave every link that starts in one alone.
  const runs = [...text.matchAll(/`+/g)].map((m) => [m.index, m[0].length]);
  const spans = [];
  for (let i = 0; i < runs.length; i++) {
    const close = runs.findIndex(([, length], j) => j > i && length === runs[i][1]);
    if (close === -1) continue;
    spans.push([runs[i][0], runs[close][0]]);
    i = close;
  }
  const inCode = (at) => spans.some(([start, end]) => at > start && at < end);
  return text.replace(LINK, (link, open, target, close, at) => (inCode(at) ? link : open + rewrite(target, from) + close));
}

function convert(path) {
  const lines = readFileSync(join(SOURCE, path), 'utf8').split('\n');
  let fence = null;
  let title = null;
  const body = [];
  let prose = [];
  const flush = () => {
    if (prose.length) body.push(rewriteText(prose.join('\n'), path));
    prose = [];
  };
  for (const line of lines) {
    const mark = line.match(/^\s*(`{3,}|~{3,})/);
    if (fence) {
      body.push(line);
      if (mark && line.trim().startsWith(fence) && line.trim().replace(/[`~]/g, '') === '') fence = null;
      continue;
    }
    if (mark) {
      flush();
      fence = mark[1];
      body.push(line);
      continue;
    }
    if (title === null && /^# /.test(line)) {
      title = line.slice(2).trim();
      continue;
    }
    prose.push(line);
  }
  flush();
  if (title === null) {
    problems.push(`${path}: no "# heading" for the title`);
    title = path;
  }
  const plain = title.replace(/`([^`]*)`/g, '$1').replace(/\*\*([^*]+)\*\*/g, '$1');
  const front = ['---', `title: ${JSON.stringify(plain)}`, `editUrl: ${GITHUB}/edit/${BRANCH}/${path}`, '---', ''];
  return { title: plain, text: front.join('\n') + body.join('\n').replace(/^\n+/, '') };
}

for (const tree of TREES) rmSync(join(CONTENT, tree), { recursive: true, force: true });
const index = [];
for (const path of files) {
  const dest = join(CONTENT, path.endsWith('.md') ? outPath(path) : path);
  if (path.endsWith('.md')) {
    const { title, text } = convert(path);
    mkdirSync(dirname(dest), { recursive: true });
    writeFileSync(dest, text);
    index.push({ source: path, route: route(path), title });
  } else if (IMAGE.test(path)) {
    mkdirSync(dirname(dest), { recursive: true });
    cpSync(join(SOURCE, path), dest);
  }
}
mkdirSync(join(ROOT, 'src/generated'), { recursive: true });
writeFileSync(join(ROOT, 'src/generated/pages.json'), JSON.stringify(index, null, 2) + '\n');
console.log(`${index.length} pages and ${files.filter((f) => IMAGE.test(f)).length} images (${TREES.join(', ')})`);

// the model, with every state worked out by the repository's own scripts/model.py, for
// the maps (src/pages/spec/, src/lib/model.mjs). a model with problems stops the build:
// the site that is live stays as it was.
const MODEL = join(ROOT, 'src/generated/model.json');
rmSync(MODEL, { force: true });
try {
  console.log(execFileSync('python3', [join(SOURCE, 'scripts/model.py'), '--json', MODEL], { encoding: 'utf8' }).trim());
  const { problems: found = [] } = JSON.parse(readFileSync(MODEL, 'utf8'));
  for (const p of found) problems.push(`the model: ${p}`);
} catch (error) {
  problems.push(error.code === 'ENOENT' ? 'python3 is needed to work out the model' : `scripts/model.py --json failed: ${error.message}`);
}
if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
