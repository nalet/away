// what the goal map colours its tiles by, for the page as it is built and for the
// browser when someone switches: one rule for both, as src/lib/capmap-lenses.mjs is for
// the capability map. a tile is a group of capabilities (an area) under the goal it is
// for; it holds its values as data attributes (see src/pages/spec/goals/map.astro).
export const LENSES = [
  {
    // how much of the group the first release holds; once something is built, how much
    // of it is there
    key: 'plan',
    label: 'first release',
    classes: [
      ['built', 'all of it built', 'success'],
      ['partly', 'some of it built', 'warning'],
      ['all', 'all of it in the first release', 'info-strong'],
      ['some', 'some of it in the first release', 'info'],
      ['later', 'none of it in the first release', 'neutral'],
    ],
    of: (d) => d.plan,
    lab: (d) => d.labPlan,
  },
  {
    // a capability left out on purpose: not planned as a whole, or for one kind of connection
    key: 'out',
    label: 'left out',
    classes: [
      ['out', 'holds what is not planned', 'out'],
      ['none', 'nothing of it is left out', 'neutral'],
    ],
    of: (d) => d.out,
    lab: (d) => d.labOut,
  },
];

export const lensByKey = Object.fromEntries(LENSES.map((l) => [l.key, l]));

// a tile's class, tone and label under a lens
export function classify(d, lensKey) {
  const lens = lensByKey[lensKey] ?? LENSES[0];
  const cls = lens.of(d);
  const [, name, tone] = lens.classes.find(([key]) => key === cls) ?? [cls, cls, 'neutral'];
  return { cls, tone, label: lens.lab ? lens.lab(d) : name };
}
