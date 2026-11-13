#!/usr/bin/env node
// Writes spec/screens/img/icons.js: the icons the drawings use, as the inner markup
// of each icon's 24 × 24 picture, taken from an installed lucide-react.
//
//   node scripts/make-icons.mjs /path/to/node_modules/lucide-react
//
// A drawing names an icon as <i data-i="terminal"></i> or icon="terminal" on a
// <ds-*> tag. An icon a drawing needs and this list lacks is added to NAMES, and the
// file is made again.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const NAMES = `
a-large-small activity app-window archive arrow-big-up arrow-down arrow-left arrow-left-right arrow-right arrow-up
arrow-up-down arrow-up-right badge-info book-open bug building building-2 cable calendar calendar-clock check
check-check chevron-down chevron-left chevron-right chevron-up chevrons-left chevrons-right chevrons-up-down circle
circle-alert circle-check circle-dot circle-help circle-power circle-slash circle-stop circle-user circle-x clipboard
clipboard-check clipboard-copy clipboard-paste clipboard-x clock cloud-off code cog command computer copy
corner-down-left cpu database door-open ellipsis ellipsis-vertical equal ethernet-port expand external-link eye
eye-off file-check file-clock file-code file-lock file-text file-warning file-x files filter fingerprint-pattern flag folder
folder-open fullscreen gauge git-branch globe grip-horizontal grip-vertical hand hard-drive hash history hourglass
house id-card info key key-round keyboard languages laptop laptop-minimal layers layout-grid life-buoy link link-2
list list-filter loader-circle lock lock-keyhole lock-open log-in log-out logs mail map-pin maximize maximize-2
minimize minimize-2 minus monitor monitor-check monitor-cog monitor-off monitor-smartphone monitor-x mouse
mouse-pointer mouse-pointer-2 move network octagon-alert option package palette panel-left panel-right panel-top
panel-top-close panel-top-open panels-top-left pause pencil pin play plug plus pointer power printer redo refresh-cw
rotate-ccw rotate-cw rows-3 scale scaling scan scroll-text search search-x send server server-cog server-off settings
settings-2 shield shield-alert shield-check shield-x shrink sliders-horizontal smartphone square
square-arrow-out-up-right square-terminal star star-off table tablet tag terminal text-cursor text-cursor-input
text-search timer timer-off toggle-right touchpad trash-2 triangle-alert type undo undo-2 unlink unplug upload user
user-check user-cog user-round users users-round wifi wifi-off wrench x zap zoom-in zoom-out
`.split(/\s+/).filter(Boolean);

const lucide = process.argv[2];
if (!lucide || !existsSync(path.join(lucide, 'dist/esm/icons'))) {
  throw new Error('usage: make-icons.mjs /path/to/node_modules/lucide-react');
}
const version = JSON.parse(readFileSync(path.join(lucide, 'package.json'), 'utf8')).version;
const attrs = (o) => Object.entries(o).filter(([k]) => k !== 'key').map(([k, v]) => `${k}="${v}"`).join(' ');
const icons = {};
const missing = [];
for (const name of [...new Set(NAMES)].sort()) {
  let file = path.join(lucide, 'dist/esm/icons', `${name}.mjs`);
  if (!existsSync(file)) { missing.push(name); continue; }
  let src = readFileSync(file, 'utf8');
  // an older name is a file that passes on the icon's newer one
  const alias = src.match(/export \{ default \} from '\.\/([a-z0-9-]+)\.mjs'/);
  if (alias) src = readFileSync(path.join(lucide, 'dist/esm/icons', `${alias[1]}.mjs`), 'utf8');
  // the icon's parts, as the package lists them: node: [["path", { d: "…", key: "…" }], …]
  const start = src.indexOf('[', src.indexOf('node:'));
  let end = start, depth = 0;
  if (start > 0) for (; end < src.length; end++) {
    if (src[end] === '[') depth++;
    else if (src[end] === ']' && --depth === 0) break;
  }
  if (start < 0 || end >= src.length) { missing.push(name); continue; }
  const parts = new Function(`return ${src.slice(start, end + 1)}`)();
  icons[name] = parts.map(([tag, a]) => `<${tag} ${attrs(a)}/>`).join('');
}
if (missing.length) throw new Error(`lucide-react ${version} has no: ${missing.join(', ')}`);
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../spec/screens/img/icons.js');
writeFileSync(out, `/* Icons from Lucide (https://lucide.dev), ISC License, Copyright (c) Lucide Contributors.
   Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted,
   provided that the above copyright notice and this permission notice appear in all copies.
   Made by scripts/make-icons.mjs from lucide-react ${version}; used by the img/*.html drawings. */
window.ICONS = ${JSON.stringify(icons)};
`);
console.log(`${Object.keys(icons).length} icons from lucide-react ${version} -> ${path.relative(process.cwd(), out)}`);
