// the site's sections, in the order of the top bar, and the pages of each, in the order
// of its side navigation. a page is named by its path in the repository: docs/ holds the
// documentation, spec/ the specification. a section's address says which of the two it
// belongs to, and the top bar shows the sections of the one being read. the names are
// chrome: lowercase.
//
// this file only gives pages their place and their name; the repository decides which
// pages there are. an entry that is an address (it starts with a /) is a page of the
// site itself, such as the goal map (src/pages/spec/goals/map.astro) or the page of
// all screens (src/pages/spec/screens/all.astro). a page it doesn't
// list goes at the end of its folder's section (FOLDERS), under its title, and a folder
// no section has becomes a section of its own, last in the top bar. a page it lists
// that the repository no longer has is left out. `npm run build` names both, and
// neither stops it.
import pages from './generated/pages.json' with { type: 'json' };

const SECTIONS = [
  {
    id: 'docs',
    label: 'overview',
    pages: [
      ['docs/README.md', 'documentation'],
      ['docs/model.md', 'changing the model'],
    ],
  },
  {
    id: 'spec',
    label: 'overview',
    pages: [['spec/README.md', 'the specification']],
  },
  {
    // what Away is for: the goals, what each needs and how well it has to be met
    id: 'goals',
    label: 'goals',
    pages: [
      ['/spec/goals/map/', 'the map'],
      ['spec/goals/README.md', 'what it is for'],
    ],
  },
  {
    id: 'capabilities',
    label: 'capabilities',
    pages: [
      ['/spec/capabilities/map/', 'the map'],
      ['spec/capabilities/README.md', 'the model'],
      ['spec/capabilities/expectations.md', 'expectations'],
      ['spec/capabilities/session.md', 'in a session'],
      ['spec/capabilities/app.md', 'around the sessions'],
      ['spec/capabilities/admin.md', 'administration'],
      ['spec/capabilities/platform.md', 'the platform'],
      ['spec/capabilities/left-out.md', 'what is left out'],
    ],
  },
  {
    // what Away looks like: the screens of the first release, chapter by chapter
    id: 'screens',
    label: 'screens',
    pages: [
      ['/spec/screens/all/', 'all screens'],
      ['spec/screens/README.md', '1 introduction'],
      ['spec/screens/frame.md', '2 the frame'],
      ['spec/screens/start.md', '3 getting in'],
      ['spec/screens/machines.md', '4 machines'],
      ['spec/screens/opening.md', '5 opening a machine'],
      ['spec/screens/desktop.md', '6 a desktop'],
      ['spec/screens/programs.md', '7 programs'],
      ['spec/screens/shell.md', '8 a shell'],
      ['spec/screens/sessions.md', '9 sessions'],
      ['spec/screens/phone.md', '10 on a phone'],
      ['spec/screens/settings.md', '11 settings'],
      ['spec/screens/admin-machines.md', '12 administration: machines'],
      ['spec/screens/admin-rules.md', '13 administration: rules'],
      ['spec/screens/admin-records.md', '14 administration: sessions and records'],
      ['spec/screens/admin-status.md', '15 administration: status'],
      ['spec/screens/errors.md', '16 when something goes wrong'],
      ['spec/screens/words.md', '17 words'],
    ],
  },
];

// the folder whose unlisted pages each section takes
const FOLDERS = { docs: ['docs'], spec: ['spec'], goals: ['spec/goals'], capabilities: ['spec/capabilities'], screens: ['spec/screens'] };

const bySource = new Map(pages.map((p) => [p.source, p]));
const isAddress = (path) => path.startsWith('/');
const slug = (route) => route.replace(/^\/|\/$/g, '');

// listed here, gone from the repository
export const gone = SECTIONS.flatMap((s) => s.pages.map(([path]) => path)).filter((path) => !isAddress(path) && !bySource.has(path));
const sections = SECTIONS.map((s) => ({ ...s, pages: s.pages.filter(([path]) => isAddress(path) || bySource.has(path)) }));

// in the repository, not listed here
const listed = new Set(sections.flatMap((s) => s.pages.map(([path]) => path)));
export const unlisted = [...bySource.keys()].filter((path) => !listed.has(path)).sort();
for (const path of unlisted) {
  const parts = path.split('/');
  const folder = parts.length > 2 ? parts.slice(0, 2).join('/') : parts[0];
  const id = Object.keys(FOLDERS).find((key) => FOLDERS[key].includes(folder)) ?? folder.replaceAll('/', '-');
  let section = sections.find((s) => s.id === id);
  if (!section) {
    section = { id, label: folder.split('/').pop().replaceAll('-', ' '), pages: [] };
    sections.push(section);
    FOLDERS[id] = [folder];
  }
  if (path === `${folder}/README.md`) section.pages.unshift([path, 'overview']);
  else section.pages.push([path, bySource.get(path).title.toLowerCase()]);
}

// an address here is without the site's base: Starlight adds it to a page's link, and
// the top bar (src/components/Header.astro) adds it to a section's.
export const topics = sections
  .filter((s) => s.pages.length)
  .map((section) => {
    const items = section.pages.map(([path, label]) => (isAddress(path) ? { label, link: path } : { label, slug: slug(bySource.get(path).route) }));
    return { id: section.id, label: section.label, link: items[0].link ?? '/' + items[0].slug + '/', items };
  });
