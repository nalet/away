// what the capability map colours its tiles by, for the page as it is built and for
// the browser when someone switches: one rule for both, so the tiles, their labels
// and the legend can't tell different stories. a tile holds its capability's values
// as data attributes (see src/pages/spec/capabilities/map.astro); a lens reads them.
export const WAYS = [
  ['all', 'as a whole'],
  ['rdp', 'RDP'],
  ['vnc', 'VNC'],
  ['ssh', 'SSH'],
  ['away', 'Away itself'],
];

export const LENSES = [
  {
    // where a capability stands: as a whole, or for one kind of connection
    key: 'stands',
    label: 'where it stands',
    classes: [
      ['built', 'built', 'success'],
      ['partial', 'partly built', 'warning'],
      ['v1', 'first release', 'info'],
      ['later', 'later', 'neutral'],
      ['no', 'not planned', 'out'],
      ['none', 'does not apply', 'none'],
    ],
    of: (d, way) => d[way],
  },
  {
    // what people expect of remote access (spec/capabilities/expectations.csv)
    key: 'expected',
    label: 'expected',
    classes: [
      ['named', 'an expectation names it', 'info'],
      ['none', 'no expectation names it', 'neutral'],
    ],
    of: (d) => d.expected,
    lab: (d) => d.labExpected,
  },
];

export const lensByKey = Object.fromEntries(LENSES.map((l) => [l.key, l]));

// a tile's class, tone and label under a lens (for where it stands, for one way)
export function classify(d, lensKey, way) {
  const lens = lensByKey[lensKey] ?? LENSES[0];
  const cls = lens.of(d, way);
  const [, name, tone] = lens.classes.find(([key]) => key === cls) ?? [cls, cls, 'neutral'];
  return { cls, tone, label: lens.lab ? lens.lab(d) : name };
}
