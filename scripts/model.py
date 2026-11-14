#!/usr/bin/env python3
"""model.py [--check] [--json PATH] : the model of the specification, checked, and its tables
written into the pages: the goals of spec/goals, the capabilities of spec/capabilities, the
screens of spec/screens, the architecture of spec/architecture and the work packages of
spec/plan.

What Away is for is two files in spec/goals/:
  goals.csv         one row a goal: what people want of it, what that takes, what being
                    open adds; and the goals each needs
  qualities.csv     one row a quality a goal has to be met with: what measures it, its
                    target (the owner's to set) and a proposal for it
What Away does is three files in spec/capabilities/:
  areas.csv         the areas, each with its part and the goal it is for
  capabilities.csv  one row a capability: who it is for, and where it stands for a session
                    over each protocol (rdp, vnc, ssh) or for Away itself (away)
  expectations.csv  one row a thing people expect of remote access they run themselves,
                    with the capabilities that meet it
Every area and every expectation is for one goal, so each capability leads up to a goal.
What Away looks like is the chapters of spec/screens/ and one file there:
  screens.csv       one row a figure: its chapter, what it shows, its size, and the
                    capabilities it draws. A figure's picture is img/<key>.png, drawn by
                    scripts/render-mockups.mjs from the chapter's img/ch*.html.
How Away is built is four files in spec/architecture/ and the decision records beside them:
  components.csv    one row a component: its layer, its kind, when it comes, its code, what
                    runs it and what it keeps
  connections.csv   one row a connection: from one component to another, of what kind, on what
  carries.csv       one row a capability of the first release: the components that carry it
  decisions.csv     one row a decision record: the components and capabilities it governs
  decisions/        the records themselves, 0001-<words>.md and so on
In what order it is built is two files in spec/plan/:
  packages.csv      one row a work package: a proof or a build, how far it is, the packages it
                    comes after, the decisions it rests on, and the gate that ends it
  delivers.csv      one row a capability a package delivers, for every kind of connection it
                    is planned for or for the ones named

Where a capability stands is one of:
  built     there
  partial   there, with the gap its note names
  v1        planned for the first release
  later     wanted, after the first release
  no        not planned
  —         does not apply
A capability is either about a session, and then stands somewhere for at least one
protocol and has — for Away itself; or it is about Away around the sessions, and then it
is the other way round.

It checks, and names every problem it finds:
  - every file has its columns, every value is one the model allows, every key is unique,
    kebab-case and at most 32 characters;
  - every goal a goal needs exists, and no goal needs itself by way of others;
  - every area, expectation and quality is for a goal that exists, every capability is in
    an area that exists, every expectation names capabilities that exist;
  - every goal has an area, an expectation or a quality; every area has a capability;
  - every figure has its picture and is in its chapter, in the file's order and under its
    number; every picture and every drawing in the chapters' sources is a figure; every
    capability a figure draws exists;
  - every component is in a layer and of a kind that exist, and a connection names it; every
    connection is between components that exist, of a kind that exists, there once, and no
    sooner than both its ends;
  - every capability of the first release is carried by a component of Away's own, and by none
    that comes later; every decision record has its number, its status and its sections, and
    one row; every row names components and capabilities that exist;
  - every work package comes after packages listed before it and rests on a decision; one
    that has begun comes after packages that are done and rests on decisions that are
    accepted; a proof delivers nothing and a build something; every capability of the first release is
    delivered once for each kind of connection it is planned for, or by rows that say which
    part of it each delivers;
  - the pages' generated blocks are what the model says (--check), else they are written.
    A block is the text between "<!-- generated: name -->" and "<!-- end: name -->";
    everything outside the blocks is written by hand.
Numbers on the pages come from here, never typed.

model.py --json PATH writes the model instead, with each capability's and expectation's
state worked out, for the site's maps. It changes no page, and it writes the file even
when the check finds problems: they are in it, under problems."""
import csv
import json
import os
import re
import sys
import unicodedata
from collections import Counter

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
SPEC = os.path.join(ROOT, 'spec')
GOALS = os.path.join(SPEC, 'goals')
CAP = os.path.join(SPEC, 'capabilities')
SCREENS = os.path.join(SPEC, 'screens')
ARCH = os.path.join(SPEC, 'architecture')
RECORDS = os.path.join(ARCH, 'decisions')
PLAN = os.path.join(SPEC, 'plan')

GOAL_KINDS = [('use', 'what people want'), ('enable', 'what that takes'), ('open', 'what being open adds')]
CHARACTERISTICS = ['functional suitability', 'performance efficiency', 'compatibility', 'interaction capability',
                   'reliability', 'security', 'maintainability', 'flexibility', 'safety']
PARTS = [('session', 'in a session', 'session.md'), ('app', 'around the sessions', 'app.md'),
         ('admin', 'administration', 'admin.md'), ('platform', 'the platform', 'platform.md')]
WAYS = [('rdp', 'RDP'), ('vnc', 'VNC'), ('ssh', 'SSH')]
ITSELF = [('away', 'Away')]
WHERE = WAYS + ITSELF
STATUS = ['built', 'partial', 'v1', 'later', 'no', '—']
# a capability in one word, and the same words for a cell of the tables
STATES = ['built', 'partial', 'v1', 'later', 'no']
STATE_WORD = {'built': 'built', 'partial': 'partly built', 'v1': 'first release', 'later': 'later',
              'no': 'not planned', '—': 'does not apply'}
WHO = ['person', 'admin', 'operator', 'system', 'anyone']
EXPECT_STATES = ['there', 'partly', 'first release', 'first release in part', 'later', 'not planned']
KEY = re.compile(r'^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$')

GOAL_COLS = ['key', 'goal', 'kind', 'needs', 'why']
QUALITY_COLS = ['key', 'goal', 'characteristic', 'quality', 'measure', 'target', 'proposal']
AREA_COLS = ['key', 'part', 'goal', 'area', 'intro']
CAP_COLS = ['key', 'area', 'capability', 'who'] + [k for k, _ in WHERE] + ['note']
EXPECT_COLS = ['key', 'goal', 'expectation', 'capabilities', 'note']
SCREEN_COLS = ['key', 'chapter', 'figure', 'size', 'capabilities', 'note']
SIZES = ['computer', 'phone', 'phone-side']
# whose capabilities have something to see: a figure can draw them
SEEN_BY = ['person', 'admin']

# the architecture: a component is in a layer; the layers are the lanes of the picture
LAYERS = [('browser', 'In the browser', "What a person's browser runs."),
          ('parts', "Away's parts", "What the chart runs from Away's one image."),
          ('code', "Away's own protocol code", 'The libraries the parts and the session view are built from.'),
          ('rests', 'What Away rests on', 'What the cluster gives it, and the database.'),
          ('outside', 'Outside Away', 'What Away reaches and does not run.'),
          ('making', 'Making and shipping it', 'What builds, tests, installs and describes Away.')]
# the layers whose components are Away's own: only they carry a capability
OWN = ['browser', 'parts', 'code', 'making']
COMPONENT_KINDS = ['web', 'service', 'relay', 'library', 'contract', 'database', 'files', 'store', 'proxy', 'identity',
                   'machine', 'directory', 'authority', 'logs', 'monitoring', 'chart', 'pipeline', 'tests', 'registry', 'docs']
# the kinds of component that are code in this repository: one that is there names where
CODE_KINDS = ['web', 'service', 'relay', 'library', 'contract', 'chart', 'pipeline', 'tests', 'docs']
RELEASES = ['built', 'v1', 'later']
RELEASE_WORD = {'built': 'there', 'v1': 'first release', 'later': 'later'}
CONNECT = [('loads', "the browser gets the pages' files from the other"),
           ('calls', 'it sends requests to the other'),
           ('streams', 'it keeps a connection to the other open for as long as a session lasts, with messages both ways'),
           ('sends to', "it sends the browser to the other's page"),
           ('signs in', "it takes people's sign-ins from the other"),
           ('opens', "it opens the other, speaking the other's own protocol"),
           ('asks', 'it asks the other to make something for one session'),
           ('reads', 'it reads what the other holds, and changes nothing'),
           ('writes', 'it changes what the other holds'),
           ('fetches', 'it gets a file from the other'),
           ('passes on', 'it hands what arrives on to the other'),
           ('puts out', 'it writes lines that the other collects'),
           ('measures', "it reads the other's measures"),
           ('uses', "it is built from the other's code"),
           ('publishes', 'it puts what it built where the other holds or shows it'),
           ('tests', 'it runs its tests against the other')]
RECORD_STATES = ['Proposed', 'Accepted', 'Superseded', 'Rejected']
PACKAGE_KINDS = [('proof', 'a proof'), ('build', 'a build')]
# a package is proposed until the owner names it, then agreed; begun and done say how far it is
PACKAGE_STATES = ['proposed', 'agreed', 'begun', 'done']
COMPONENT_COLS = ['key', 'component', 'layer', 'kind', 'release', 'code', 'runs', 'keeps', 'note']
CONNECTION_COLS = ['from', 'to', 'kind', 'on', 'release', 'note']
CARRY_COLS = ['capability', 'components', 'note']
DECISION_COLS = ['adr', 'components', 'capabilities', 'note']
PACKAGE_COLS = ['key', 'package', 'kind', 'state', 'after', 'decisions', 'gate', 'note']
DELIVER_COLS = ['package', 'capability', 'ways', 'note']
# where a capability stands when it is of the first release
FIRST = ('built', 'partial', 'v1')

problems = []


def problem(where, what):
    problems.append(f'{where}: {what}')


def read(name, cols, base=CAP):
    path = os.path.join(base, name)
    if not os.path.exists(path):
        problem(name, 'missing')
        return []
    with open(path, encoding='utf-8', newline='') as fh:
        r = csv.DictReader(fh)
        if r.fieldnames != cols:
            problem(name, f'columns are {r.fieldnames}, not {cols}')
            return []
        return [{k: (v or '').strip() for k, v in row.items() if k} for row in r]


def split(v):
    return [x.strip() for x in v.split(';') if x.strip()]


def slug(text):
    """the anchor GitHub and the site give a heading"""
    kept = ''.join(ch for ch in text.lower() if ch in ' -' or unicodedata.category(ch)[0] in 'LMN'
                   or unicodedata.category(ch) == 'Pc')
    return kept.replace(' ', '-')


def keys_of(rows, name, most=32):
    """the rows by key; a key that is malformed or there twice is a problem"""
    out = {}
    for r in rows:
        w = f'{name} {r.get("key")}'
        if not KEY.match(r['key']) or len(r['key']) > most:
            problem(w, f'key is not kebab-case of at most {most} characters')
        if r['key'] in out:
            problem(w, 'key twice')
        out[r['key']] = r
    return out


def sentence(w, column, text, most, stop=False):
    """a cell of words: there, not too long, and with or without a full stop at its end"""
    if not text or len(text) > most:
        problem(w, f'{column} is empty or longer than {most} characters')
    elif text.endswith('.') != stop:
        problem(w, f'{column} {"has no" if stop else "ends with a"} full stop')


# ------------------------------------------------------------------- the checks
def check_goals(goals, qualities):
    kinds = [k for k, _ in GOAL_KINDS]
    keys = keys_of(goals, 'goals.csv')
    for g in goals:
        w = f'goals.csv {g["key"]}'
        sentence(w, 'goal', g['goal'], 60)
        if '"' in g['goal']:
            problem(w, 'goal holds a double quote')
        if g['kind'] not in kinds:
            problem(w, f'kind {g["kind"]!r} is none of {kinds}')
        sentence(w, 'why', g['why'], 220, stop=True)
        for k in split(g['needs']):
            if k not in keys:
                problem(w, f'needs: {k} is not in goals.csv')

    def reaches(k, seen):
        for n in split(keys[k]['needs']):
            if n in keys and (n in seen or reaches(n, seen | {n})):
                return True
        return False
    for g in goals:
        if reaches(g['key'], {g['key']}):
            problem(f'goals.csv {g["key"]}', 'needs itself, by way of the goals it needs')
    keys_of(qualities, 'qualities.csv')
    for q in qualities:
        w = f'qualities.csv {q["key"]}'
        if q['goal'] not in keys:
            problem(w, f'goal {q["goal"]!r} is not in goals.csv')
        if q['characteristic'] not in CHARACTERISTICS:
            problem(w, f'characteristic {q["characteristic"]!r} is none of {CHARACTERISTICS}')
        sentence(w, 'quality', q['quality'], 90)
        sentence(w, 'measure', q['measure'], 100)
        if len(q['target']) > 160 or len(q['proposal']) > 160:
            problem(w, 'target or proposal is longer than 160 characters')
    return keys


def check_model(goal_keys, areas, caps, expectations, qualities):
    parts = [p for p, _, _ in PARTS]
    area_keys = keys_of(areas, 'areas.csv')
    for a in areas:
        w = f'areas.csv {a["key"]}'
        if a['part'] not in parts:
            problem(w, f'part {a["part"]!r} is none of {parts}')
        if a['goal'] not in goal_keys:
            problem(w, f'goal {a["goal"]!r} is not in goals.csv')
        sentence(w, 'area', a['area'], 60)
        sentence(w, 'intro', a['intro'], 200, stop=True)
    names = Counter(a['area'] for a in areas)
    for a in areas:
        if names[a['area']] > 1:
            problem(f'areas.csv {a["key"]}', f'another area is named {a["area"]!r} too')
    cap_keys = keys_of(caps, 'capabilities.csv')
    for c in caps:
        w = f'capabilities.csv {c["key"]}'
        if c['area'] not in area_keys:
            problem(w, f'area {c["area"]!r} is not in areas.csv')
        sentence(w, 'capability', c['capability'], 110)
        if c['who'] not in WHO:
            problem(w, f'who {c["who"]!r} is none of {WHO}')
        for k, _ in WHERE:
            if c[k] not in STATUS:
                problem(w, f'{k} {c[k]!r} is none of {STATUS}')
        ways = [c[k] for k, _ in WAYS if c[k] != '—']
        if bool(ways) == (c['away'] != '—'):
            problem(w, 'stands somewhere for a protocol and for Away itself, or for neither: it is one or the other')
        if len(c['note']) > 160 or c['note'].endswith('.'):
            problem(w, 'note is longer than 160 characters or ends with a full stop')
        if 'no' in [c[k] for k, _ in WHERE] and not c['note']:
            problem(w, 'is not planned somewhere, and no note says why')
    for a in areas:
        if not any(c['area'] == a['key'] for c in caps):
            problem(f'areas.csv {a["key"]}', 'has no capability')
    keys_of(expectations, 'expectations.csv')
    for e in expectations:
        w = f'expectations.csv {e["key"]}'
        if e['goal'] not in goal_keys:
            problem(w, f'goal {e["goal"]!r} is not in goals.csv')
        sentence(w, 'expectation', e['expectation'], 90)
        named = split(e['capabilities'])
        if not named:
            problem(w, 'names no capability')
        for k in named:
            if k not in cap_keys:
                problem(w, f'capabilities: {k} is not in capabilities.csv')
        if len(e['note']) > 140 or e['note'].endswith('.'):
            problem(w, 'note is longer than 140 characters or ends with a full stop')
    used = {a['goal'] for a in areas} | {e['goal'] for e in expectations} | {q['goal'] for q in qualities}
    for k in goal_keys:
        if k not in used:
            problem(f'goals.csv {k}', 'no area, no expectation and no quality is for it')
    return area_keys, cap_keys


def chapters():
    """the chapters of spec/screens/, by file name without .md: each starts "# 6 A desktop" """
    out = {}
    if not os.path.isdir(SCREENS):
        return out
    for name in sorted(os.listdir(SCREENS)):
        if not name.endswith('.md'):
            continue
        text = open(os.path.join(SCREENS, name), encoding='utf-8').read()
        m = re.match(r'# (\d+) (.+)\n', text)
        if not m:
            problem(f'spec/screens/{name}', 'does not start with "# <number> <title>"')
            continue
        out[name[:-3]] = {'key': name[:-3], 'no': int(m.group(1)), 'title': m.group(2).strip(), 'file': name, 'text': text}
    numbers = Counter(c['no'] for c in out.values())
    for c in out.values():
        if numbers[c['no']] > 1:
            problem(f'spec/screens/{c["file"]}', f'chapter number {c["no"]} is taken twice')
    return out


def check_screens(screens, chaps, cap_keys):
    """every figure, with its number: 6.3 is the third figure of chapter 6"""
    keys_of(screens, 'screens.csv', 40)
    img = os.path.join(SCREENS, 'img')
    pictures = {n[:-4] for n in os.listdir(img) if n.endswith('.png')} if os.path.isdir(img) else set()
    drawn = set()
    if os.path.isdir(img):
        for name in sorted(os.listdir(img)):
            if name.endswith('.html'):
                source = re.sub(r'<!--.*?-->', '', open(os.path.join(img, name), encoding='utf-8').read(), flags=re.S)
                drawn |= set(re.findall(r'<figure class="shot" id="([^"]+)"', source))
    seen = Counter()
    last = {}
    for f in screens:
        w = f'screens.csv {f["key"]}'
        sentence(w, 'figure', f['figure'], 100)
        if f['size'] not in SIZES:
            problem(w, f'size "{f["size"]}" is none of {SIZES}')
        for k in split(f['capabilities']):
            if k not in cap_keys:
                problem(w, f'draws the capability "{k}", which does not exist')
        c = chaps.get(f['chapter'])
        if not c:
            problem(w, f'chapter "{f["chapter"]}" has no file spec/screens/{f["chapter"]}.md')
            continue
        seen[f['chapter']] += 1
        f['no'] = f'{c["no"]}.{seen[f["chapter"]]}'
        if f['key'] not in pictures:
            problem(w, f'has no picture spec/screens/img/{f["key"]}.png')
        if f['key'] not in drawn:
            problem(w, 'is drawn in no img/ch*.html')
        at = c['text'].find(f'](img/{f["key"]}.png)')
        if at < 0:
            problem(w, f'is not in spec/screens/{c["file"]}')
            continue
        if at < last.get(f['chapter'], -1):
            problem(w, f'comes earlier in spec/screens/{c["file"]} than the figure before it in screens.csv')
        last[f['chapter']] = at
        m = re.match(r'\]\(img/[^)]+\)\s*\n\s*\n\*\*Figure (\d+\.\d+)\*\*', c['text'][at:])
        if not m:
            problem(w, f'has no caption "**Figure {f["no"]}** …" under it in spec/screens/{c["file"]}')
        elif m.group(1) != f['no']:
            problem(w, f'is figure {f["no"]} by its place, and its caption in spec/screens/{c["file"]} says {m.group(1)}')
    listed = {f['key'] for f in screens}
    for k in sorted(pictures - listed):
        problem(f'spec/screens/img/{k}.png', 'is no figure of screens.csv')
    for k in sorted(drawn - listed):
        problem(f'the drawing "{k}"', 'is no figure of screens.csv')
    for c in chaps.values():
        for k in re.findall(r'\]\(img/([^)]+)\.png\)', c['text']):
            if k not in listed:
                problem(f'spec/screens/{c["file"]}', f'shows img/{k}.png, which is no figure of screens.csv')
    return [f for f in screens if 'no' in f]


# ------------------------------------------------------------------- the states
def cap_state(c):
    """a capability in one word, over every protocol it stands for, or over Away itself:
    built where all of it is, partial where some of it is; where nothing is built, the
    soonest it is planned: v1, later, or no"""
    meant = [c[k] for k, _ in WHERE if c[k] in STATES]
    if meant and all(s == 'built' for s in meant):
        return 'built'
    if any(s in ('built', 'partial') for s in meant):
        return 'partial'
    return next((s for s in ('v1', 'later') if s in meant), 'no')


def expectation_state(e, cap_keys):
    """what the model says of an expectation, from the capabilities it names: there when
    every one is built, partly when some are; where nothing is built, whether the first
    release meets it: all of it, part of it, not until later, or not at all"""
    states = [cap_state(cap_keys[k]) for k in split(e['capabilities']) if k in cap_keys]
    if states and all(s == 'built' for s in states):
        return 'there'
    if any(s in ('built', 'partial') for s in states):
        return 'partly'
    if states and all(s == 'v1' for s in states):
        return 'first release'
    if 'v1' in states:
        return 'first release in part'
    return 'later' if 'later' in states else 'not planned'


def said(counter, order, words=None):
    return ', '.join(f'{counter[x]} {(words or {}).get(x, x)}' for x in order if counter[x])


# -------------------------------------------------------------------- the pages
def cell(v):
    return str(v).replace('|', '\\|').replace('\n', ' ')


def table(rows, header):
    lines = ['| ' + ' | '.join(header) + ' |', '|' + '|'.join('---' for _ in header) + '|']
    lines += ['| ' + ' | '.join(cell(x) for x in r) + ' |' for r in rows]
    return '\n'.join(lines)


def count(n, one, many=None):
    return f'{n} {one if n == 1 else (many or one + "s")}'


class Model:
    def __init__(self, goals, qualities, areas, caps, expectations, cap_keys, screens=(), chaps=None):
        self.goals, self.qualities, self.areas, self.caps, self.expectations = goals, qualities, areas, caps, expectations
        self.screens, self.chapters = list(screens), sorted((chaps or {}).values(), key=lambda c: c['no'])
        # the figures that draw a capability, by its key
        self.drawn = {}
        for f in self.screens:
            for k in split(f['capabilities']):
                self.drawn.setdefault(k, []).append(f)
        self.goal = {g['key']: g for g in goals}
        self.area = {a['key']: a for a in areas}
        self.cap = cap_keys
        self.page = {a['key']: next(pg for p, _, pg in PARTS if p == a['part']) for a in areas}
        self.state = {c['key']: cap_state(c) for c in caps}
        self.expect = {e['key']: expectation_state(e, cap_keys) for e in expectations}
        self.named = {k for e in expectations for k in split(e['capabilities'])}
        self.build = None

    def caps_of(self, area_keys):
        return [c for c in self.caps if c['area'] in area_keys]

    def goal_link(self, k, page=''):
        return f'[{self.goal[k]["goal"]}]({page}#{slug(self.goal[k]["goal"])})'

    def area_link(self, a, base='./'):
        return f'[{a["area"]}]({base}{self.page[a["key"]]}#{slug(a["area"])})'

    # ---- spec/goals/README.md
    def goals_blocks(self):
        kind_label = dict(GOAL_KINDS)
        # the picture: a box a kind, an arrow from a goal to each goal it needs; an arrow
        # every goal of a kind has is drawn once, from the kind
        pic = ['```mermaid', 'flowchart TD']
        nid = {g['key']: 'g_' + g['key'].replace('-', '_') for g in self.goals}
        for kind, label in GOAL_KINDS:
            sel = [g for g in self.goals if g['kind'] == kind]
            if sel:
                pic.append(f'  subgraph k_{kind}["{label}"]')
                pic.append('    ' + '; '.join(f'{nid[g["key"]]}["{g["goal"]}"]' for g in sel))
                pic.append('  end')
        edges = [(g['key'], n) for g in self.goals for n in split(g['needs']) if n in self.goal]
        for kind, _ in GOAL_KINDS:
            members = [g['key'] for g in self.goals if g['kind'] == kind]
            if len(members) < 2:
                continue
            for n in sorted({n for _, n in edges}, key=lambda k: list(self.goal).index(k)):
                if n not in members and all((m, n) in edges for m in members):
                    pic.append(f'  k_{kind} --> {nid[n]}')
                    edges = [e for e in edges if not (e[0] in members and e[1] == n)]
        pic += [f'  {nid[a]} --> {nid[b]}' for a, b in edges] + ['```']

        def expected(g):
            sel = [e for e in self.expectations if e['goal'] == g['key']]
            return len(sel), said(Counter(self.expect[e['key']] for e in sel), EXPECT_STATES)

        def served(g):
            sel = [a for a in self.areas if a['goal'] == g['key']]
            its = self.caps_of({a['key'] for a in sel})
            return sel, its, said(Counter(self.state[c['key']] for c in its), STATES, STATE_WORD)
        rows, out = [], []
        for g in self.goals:
            n, exp = expected(g)
            sel, its, made = served(g)
            qs = [q for q in self.qualities if q['goal'] == g['key']]
            rows.append([self.goal_link(g['key']), kind_label.get(g['kind'], g['kind']),
                         ', '.join(self.goal_link(k) for k in split(g['needs']) if k in self.goal) or '—',
                         f'{n}: {exp}' if n else '—',
                         f'{len(its)} in {count(len(sel), "area")}: {made}' if its else '—',
                         f'{len(qs)}, {sum(1 for q in qs if q["target"])} with a target' if qs else '—'])
            out.append(f'## {g["goal"]}\n')
            needs = [self.goal_link(k) for k in split(g['needs']) if k in self.goal]
            needed = [self.goal_link(o['key']) for o in self.goals if g['key'] in split(o['needs'])]
            out.append(g['why'] + (f' It needs: {", ".join(needs)}.' if needs else '') +
                       (f' Needed by: {", ".join(needed)}.' if needed else '') + '\n')
            lines = [f'- **Expected of it:** [{count(n, "expectation")}](../capabilities/expectations.md#{slug(g["goal"])}): {exp}.'
                     if n else '- **Expected of it:** nothing is listed yet.']
            lines.append('- **Capabilities:** ' + ', '.join(
                f'{self.area_link(a, "../capabilities/")} ({sum(1 for c in self.caps if c["area"] == a["key"])})'
                for a in sel) + f'. Of the {len(its)}: {made}.' if sel
                else '- **Capabilities:** no area of the capability model is for it yet.')
            out.append('\n'.join(lines) + '\n')
            if qs:
                body = [[q['quality'], q['characteristic'], q['measure'], q['target'] or 'not set', q['proposal'] or '—'] for q in qs]
                out.append(table(body, ['It has to', 'Kind', 'Measured by', 'Target', 'Proposed']) + '\n')
            else:
                out.append('No quality is stated for it yet.\n')
        return {'cascade': '\n'.join(pic) + '\n',
                'goals': table(rows, ['Goal', 'Is', 'Needs', 'Expected of it', 'Capabilities', 'Qualities']) + '\n',
                'by-goal': '\n'.join(out).rstrip() + '\n'}

    # ---- spec/capabilities/<part>.md
    def area_blocks(self, part):
        out = []
        for a in [a for a in self.areas if a['part'] == part]:
            rows = [c for c in self.caps if c['area'] == a['key']]
            out.append(f'## {a["area"]}\n')
            out.append(f'{a["intro"]} For the goal {self.goal_link(a["goal"], "../goals/README.md")}.\n')
            where = [(k, label) for k, label in WHERE if any(c[k] != '—' for c in rows)]
            body = [[c['capability'], c['who']] + [c[k] for k, _ in where] + [f'`{c["key"]}`'] for c in rows]
            out.append(table(body, ['Capability', 'Who'] + [label for _, label in where] + ['Key']) + '\n')
            notes = [f'- *{c["capability"]}*: {c["note"]}.' for c in rows if c['note']]
            if notes:
                out.append('\n'.join(notes) + '\n')
        return '\n'.join(out).rstrip() + '\n'

    # ---- spec/capabilities/README.md
    def overview(self):
        rows = []
        for p, label, page in PARTS:
            sel = [a for a in self.areas if a['part'] == p]
            n = Counter(self.state[c['key']] for c in self.caps_of({a['key'] for a in sel}))
            rows.append([f'[{label}](./{page})', len(sel), sum(n.values())] + [n[s] for s in STATES])
        n = Counter(self.state.values())
        rows.append(['all', len(self.areas), len(self.caps)] + [n[s] for s in STATES])
        first = table(rows, ['Part', 'Areas', 'Capabilities'] + [STATE_WORD[s] for s in STATES])
        rows = []
        for k, label in WHERE:
            n = Counter(c[k] for c in self.caps)
            rows.append(['Away itself' if k == 'away' else f'a session over {label}'] + [n[s] for s in STATUS])
        second = table(rows, ['For'] + [STATE_WORD[s] for s in STATUS])
        unnamed = sum(1 for c in self.caps if c['key'] not in self.named)
        return f'{first}\n\n{second}\n\nAn expectation names {len(self.caps) - unnamed} of the {len(self.caps)} ' \
               f'capabilities; no expectation names the other {unnamed}.\n'

    def expected_counts(self):
        rows = []
        for g in self.goals:
            sel = [e for e in self.expectations if e['goal'] == g['key']]
            if sel:
                n = Counter(self.expect[e['key']] for e in sel)
                rows.append([self.goal_link(g['key'], './expectations.md'), len(sel)] + [n[x] for x in EXPECT_STATES])
        n = Counter(self.expect.values())
        rows.append(['all', len(self.expectations)] + [n[x] for x in EXPECT_STATES])
        return table(rows, ['Expected, for the goal', 'Expectations'] + EXPECT_STATES) + '\n'

    # ---- spec/capabilities/expectations.md
    def expectations_block(self):
        out = []
        for g in self.goals:
            sel = [e for e in self.expectations if e['goal'] == g['key']]
            if not sel:
                continue
            out.append(f'## {g["goal"]}\n')
            out.append(f'For the goal {self.goal_link(g["key"], "../goals/README.md")}.\n')
            body = []
            for e in sel:
                met = [f'[`{k}`](./{self.page[self.cap[k]["area"]]}#{slug(self.area[self.cap[k]["area"]]["area"])}) '
                       f'{STATE_WORD[self.state[k]]}' for k in split(e['capabilities']) if k in self.cap]
                body.append([e['expectation'], self.expect[e['key']], ', '.join(met), e['note'] or '—'])
            out.append(table(body, ['Expected', 'State', 'Met by', 'Note']) + '\n')
        return '\n'.join(out).rstrip() + '\n'

    # ---- spec/capabilities/left-out.md
    def left_out_block(self):
        def row(c):
            a = self.area[c['area']]
            where = [('Away itself' if k == 'away' else label) for k, label in WHERE if c[k] == 'no']
            rest = [f'{label} {c[k]}' for k, label in WAYS if c[k] not in ('no', '—')]
            return [c['capability'], ', '.join(where) + (f' ({", ".join(rest)})' if rest else ''), c['note'] or '—',
                    self.area_link(a), f'`{c["key"]}`']
        whole = [c for c in self.caps if self.state[c['key']] == 'no']
        part = [c for c in self.caps if self.state[c['key']] != 'no' and any(c[k] == 'no' for k, _ in WHERE)]
        header = ['Left out', 'For', 'Why', 'Area', 'Key']
        out = [f'## Not planned at all\n\n{count(len(whole), "capability", "capabilities")}.\n', table([row(c) for c in whole], header) + '\n',
               f'## Not planned for one kind of connection\n\n{count(len(part), "capability", "capabilities")} that '
               f'{"is" if len(part) == 1 else "are"} planned for another.\n', table([row(c) for c in part], header) + '\n']
        gone = [e for e in self.expectations if self.expect[e['key']] == 'not planned']
        out.append(f'## Expectations the plan does not meet\n\n{count(len(gone), "expectation")}.\n')
        out.append(table([[e['expectation'], self.goal_link(e['goal'], './expectations.md'), e['note'] or '—'] for e in gone],
                         ['Expected', 'For the goal', 'Note']) + '\n')
        return '\n'.join(out)

    # ---- spec/screens/
    def to_see(self):
        """the capabilities of the first release that have something to see"""
        return [c for c in self.caps if c['who'] in SEEN_BY and self.state[c['key']] in ('built', 'partial', 'v1')]

    def cap_link(self, k, base='../capabilities/'):
        a = self.area[self.cap[k]['area']]
        return f'[{self.cap[k]["capability"]}]({base}{self.page[a["key"]]}#{slug(a["area"])})'

    def figures_block(self):
        rows = []
        for c in self.chapters:
            its = [f for f in self.screens if f['chapter'] == c['key']]
            if its:
                caps = {k for f in its for k in split(f['capabilities']) if k in self.cap}
                rows.append([c['no'], f'[{c["title"]}](./{c["file"]})', len(its), len(caps)])
        seen = self.to_see()
        shown = sum(1 for c in seen if c['key'] in self.drawn)
        every = {k for f in self.screens for k in split(f['capabilities']) if k in self.cap}
        rows.append(['', 'all', len(self.screens), len(every)])
        return (table(rows, ['No.', 'Chapter', 'Figures', 'Capabilities they draw']) +
                f'\n\nThe first release holds {len(seen)} capabilities for a person or an administrator. '
                f'A figure shows {shown} of them.\n')

    def not_drawn_block(self):
        rows = []
        for a in self.areas:
            its = [c for c in self.to_see() if c['area'] == a['key']]
            gone = [c for c in its if c['key'] not in self.drawn]
            if gone:
                rows.append([self.area_link(a, '../capabilities/'), f'{len(its) - len(gone)} of {len(its)}',
                             '; '.join(f'{c["capability"]} `{c["key"]}`' for c in gone)])
        if not rows:
            return 'Every one of them is in a figure.\n'
        return table(rows, ['Area', 'In a figure', 'In no figure']) + '\n'

    def draws_block(self, chapter):
        rows = [[f'{f["no"]} {f["figure"]}', ', '.join(self.cap_link(k) for k in split(f['capabilities']) if k in self.cap) or '—']
                for f in self.screens if f['chapter'] == chapter]
        return table(rows, ['Figure', 'Draws']) + '\n'

    # ---- spec/README.md
    def state_block(self):
        n = Counter(self.state.values())
        e = Counter(self.expect.values())
        rows = [
            ['Goals', len(self.goals), ', '.join(f'{sum(1 for g in self.goals if g["kind"] == k)} {label}' for k, label in GOAL_KINDS)],
            ['Qualities', len(self.qualities), f'{sum(1 for q in self.qualities if q["target"])} with a target, '
                                               f'{sum(1 for q in self.qualities if q["proposal"])} with a proposal'],
            ['Expectations', len(self.expectations), said(e, EXPECT_STATES)],
            ['Capabilities', len(self.caps), f'in {count(len(self.areas), "area")}: ' + said(n, STATES, STATE_WORD)],
        ]
        if self.screens:
            seen = self.to_see()
            rows.append(['Figures', len(self.screens),
                         f'in {count(len({f["chapter"] for f in self.screens}), "chapter")}; they show '
                         f'{sum(1 for c in seen if c["key"] in self.drawn)} of the {len(seen)} capabilities the first '
                         f'release holds for a person or an administrator'])
        if self.build:
            rows += self.build.state_rows()
        return table(rows, ['', 'Rows', 'Of them']) + '\n'

    # ---- the model, for the site
    def export(self, path):
        model = {
            'parts': [{'key': p, 'label': label, 'page': f'spec/capabilities/{page}'} for p, label, page in PARTS],
            'where': [{'key': k, 'label': label, 'session': (k, label) in WAYS} for k, label in WHERE],
            'status': [{'key': s, 'label': STATE_WORD[s]} for s in STATUS],
            'areas': self.areas,
            'capabilities': [{
                'key': c['key'], 'area': c['area'], 'capability': c['capability'], 'who': c['who'], 'note': c['note'],
                'status': {k: c[k] for k, _ in WHERE}, 'state': self.state[c['key']],
                'expected': [e['key'] for e in self.expectations if c['key'] in split(e['capabilities'])],
                'drawn': [f['key'] for f in self.drawn.get(c['key'], [])],
            } for c in self.caps],
            'chapters': [{'key': c['key'], 'no': c['no'], 'title': c['title'], 'page': f'spec/screens/{c["file"]}'}
                         for c in self.chapters],
            'screens': [{'key': f['key'], 'chapter': f['chapter'], 'no': f['no'], 'figure': f['figure'], 'size': f['size'],
                         'capabilities': [k for k in split(f['capabilities']) if k in self.cap]} for f in self.screens],
            'goalKinds': [{'key': k, 'label': label} for k, label in GOAL_KINDS],
            'goals': [{'key': g['key'], 'goal': g['goal'], 'kind': g['kind'], 'needs': split(g['needs']), 'why': g['why'],
                       'areas': [a['key'] for a in self.areas if a['goal'] == g['key']]} for g in self.goals],
            'qualities': self.qualities,
            'expectations': [{'key': e['key'], 'goal': e['goal'], 'expectation': e['expectation'], 'note': e['note'],
                              'capabilities': split(e['capabilities']), 'state': self.expect[e['key']]} for e in self.expectations],
            'expectStates': EXPECT_STATES,
            'problems': list(problems),
        }
        if self.build:
            model.update(self.build.export())
        with open(path, 'w', encoding='utf-8') as fh:
            json.dump(model, fh, ensure_ascii=False, indent=1)
            fh.write('\n')


# ------------------------------------------------ the architecture and the plan
def note_ok(w, column, text, most):
    if len(text) > most or text.endswith('.'):
        problem(w, f'{column} is longer than {most} characters or ends with a full stop')


def read_records():
    """the decision records of spec/architecture/decisions/, by their number"""
    out = {}
    if not os.path.isdir(RECORDS):
        return out
    for name in sorted(os.listdir(RECORDS)):
        if name == 'README.md' or not name.endswith('.md'):
            continue
        w = f'spec/architecture/decisions/{name}'
        named = re.match(r'(\d{4})-[a-z0-9]+(?:-[a-z0-9]+)*\.md$', name)
        if not named:
            problem(w, 'is not named "<four digits>-<words>.md"')
            continue
        text = open(os.path.join(RECORDS, name), encoding='utf-8').read()
        head = re.match(r'# ADR-(\d{4}): (.+)\n', text)
        if not head or head.group(1) != named.group(1):
            problem(w, f'does not start with "# ADR-{named.group(1)}: <the decision>"')
            continue
        status = re.search(r'^\*\*Status:\*\* (\w+) · (\d{4}-\d{2}-\d{2})\.', text, re.M)
        if not status or status.group(1) not in RECORD_STATES:
            problem(w, f'has no line "**Status:** <one of {", ".join(RECORD_STATES)}> · <date>."')
            continue
        for part in ('## Context', '## Decision', '## Consequences', '### What this rules out'):
            if f'\n{part}\n' not in text:
                problem(w, f'has no section "{part}"')
        if named.group(1) in out:
            problem(w, f'another record has the number {named.group(1)}')
        out[named.group(1)] = {'adr': named.group(1), 'title': head.group(2).strip(), 'status': status.group(1),
                               'date': status.group(2), 'file': name}
    for i, n in enumerate(sorted(out), 1):
        if int(n) != i:
            problem(f'spec/architecture/decisions/{out[n]["file"]}', f'the records are not numbered one after the other: {i:04d} is missing')
            break
    return out


def check_architecture(components, connections, carries, decisions, recs, cap_keys, first):
    """first: the keys of the capabilities of the first release"""
    comp = keys_of(components, 'components.csv')
    layers = [k for k, _, _ in LAYERS]
    for c in components:
        w = f'components.csv {c["key"]}'
        sentence(w, 'component', c['component'], 120)
        if c['layer'] not in layers:
            problem(w, f'layer {c["layer"]!r} is none of {layers}')
        if c['kind'] not in COMPONENT_KINDS:
            problem(w, f'kind {c["kind"]!r} is none of {COMPONENT_KINDS}')
        if c['release'] not in RELEASES:
            problem(w, f'release {c["release"]!r} is none of {RELEASES}')
        for column, most in (('runs', 80), ('keeps', 170), ('note', 200)):
            note_ok(w, column, c[column], most)
        if c['layer'] not in OWN and c['code']:
            problem(w, "is not Away's own, and names code")
        if c['release'] == 'built':
            if not split(c['code']) and c['kind'] in CODE_KINDS:
                problem(w, 'is there, and names no code')
            for path in split(c['code']):
                if not os.path.exists(os.path.join(ROOT, path)):
                    problem(w, f'code: {path} is not in the repository')
    kinds = [k for k, _ in CONNECT]
    seen = set()
    for x in connections:
        w = f'connections.csv {x["from"]} {x["kind"]} {x["to"]}'
        ends = [comp[x[e]] for e in ('from', 'to') if x[e] in comp]
        for e in ('from', 'to'):
            if x[e] not in comp:
                problem(w, f'{e}: {x[e]} is not in components.csv')
        if x['kind'] not in kinds:
            problem(w, f'kind {x["kind"]!r} is none of {kinds}')
        if x['from'] == x['to']:
            problem(w, 'leads to itself')
        if (x['from'], x['to'], x['kind'], x['on']) in seen:
            problem(w, 'is there twice')
        seen.add((x['from'], x['to'], x['kind'], x['on']))
        note_ok(w, 'note', x['note'], 200)
        if x['release'] not in RELEASES:
            problem(w, f'release {x["release"]!r} is none of {RELEASES}')
        elif len(ends) == 2 and RELEASES.index(x['release']) < max(RELEASES.index(c['release']) for c in ends if c['release'] in RELEASES):
            problem(w, 'is planned sooner than a component it connects')
        if x['kind'] != 'uses' and x['release'] != 'later' and not x['on']:
            problem(w, 'does not say what it is on')
    linked = {x['from'] for x in connections} | {x['to'] for x in connections}
    for c in components:
        if c['key'] not in linked:
            problem(f'components.csv {c["key"]}', 'no connection names it')
    carried = {}
    for r in carries:
        w = f'carries.csv {r["capability"]}'
        if r['capability'] not in cap_keys:
            problem(w, 'is not in capabilities.csv')
            continue
        if r['capability'] in carried:
            problem(w, 'is there twice')
        carried[r['capability']] = r
        if not split(r['components']):
            problem(w, 'names no component')
        for k in split(r['components']):
            if k not in comp:
                problem(w, f'components: {k} is not in components.csv')
            elif comp[k]['layer'] not in OWN:
                problem(w, f"components: {k} is not Away's own, so it carries nothing")
            elif r['capability'] in first and comp[k]['release'] == 'later':
                problem(w, f'components: {k} comes later than the capability')
        note_ok(w, 'note', r['note'], 160)
    for k in first:
        if k not in carried:
            problem(f'capabilities.csv {k}', 'is of the first release, and carries.csv names no component that carries it')
    rows = {}
    for d in decisions:
        w = f'decisions.csv {d["adr"]}'
        if d['adr'] not in recs:
            problem(w, 'has no record in spec/architecture/decisions/')
        if d['adr'] in rows:
            problem(w, 'is there twice')
        rows[d['adr']] = d
        if not split(d['components']):
            problem(w, 'governs no component')
        for k in split(d['components']):
            if k not in comp:
                problem(w, f'components: {k} is not in components.csv')
        for k in split(d['capabilities']):
            if k not in cap_keys:
                problem(w, f'capabilities: {k} is not in capabilities.csv')
        note_ok(w, 'note', d['note'], 160)
    for n in recs:
        if n not in rows:
            problem(f'spec/architecture/decisions/{recs[n]["file"]}', 'has no row in decisions.csv')
    return comp


def check_plan(packages, delivers, recs, cap_keys, caps):
    kinds = [k for k, _ in PACKAGE_KINDS]
    ways = [k for k, _ in WHERE]
    pk = {}
    for i, p in enumerate(packages, 1):
        w = f'packages.csv {p["key"]}'
        if p['key'] != f'p{i:02d}':
            problem(w, f'the packages are numbered one after the other: p{i:02d} is expected here')
        if p['key'] in pk:
            problem(w, 'key twice')
        sentence(w, 'package', p['package'], 60)
        if p['kind'] not in kinds:
            problem(w, f'kind {p["kind"]!r} is none of {kinds}')
        if p['state'] not in PACKAGE_STATES:
            problem(w, f'state {p["state"]!r} is none of {PACKAGE_STATES}')
        underway = p['state'] in ('begun', 'done')
        for k in split(p['after']):
            if k not in pk:
                problem(w, f'after: {k} is no package listed before it')
            elif underway and pk[k]['state'] != 'done':
                problem(w, f'has begun, and {k}, which it comes after, is not done')
        if not split(p['decisions']):
            problem(w, 'rests on no decision')
        for n in split(p['decisions']):
            if n not in recs:
                problem(w, f'decisions: {n} is no decision record')
            elif underway and recs[n]['status'] != 'Accepted':
                problem(w, f'has begun, and ADR-{n}, which it rests on, is not accepted')
        sentence(w, 'gate', p['gate'], 640, stop=True)
        note_ok(w, 'note', p['note'], 160)
        pk[p['key']] = p
    cover = {}
    seen = set()
    for d in delivers:
        w = f'delivers.csv {d["package"]} {d["capability"]}'
        if d['package'] not in pk:
            problem(w, 'its package is not in packages.csv')
            continue
        if pk[d['package']]['kind'] != 'build':
            problem(w, 'its package is a proof, and a proof delivers no capability')
        c = cap_keys.get(d['capability'])
        if not c:
            problem(w, 'its capability is not in capabilities.csv')
            continue
        if (d['package'], d['capability']) in seen:
            problem(w, 'is there twice')
        seen.add((d['package'], d['capability']))
        planned = [k for k in ways if c[k] in FIRST]
        if not planned:
            problem(w, 'the capability is not of the first release')
        for k in split(d['ways']) or planned:
            if k not in ways:
                problem(w, f'ways: {k} is none of {ways}')
            elif k not in planned:
                problem(w, f'ways: the capability is not of the first release for {k}')
            else:
                cover.setdefault((d['capability'], k), []).append(d)
        note_ok(w, 'note', d['note'], 160)
    for c in caps:
        for k, label in WHERE:
            if c[k] not in FIRST:
                continue
            rows = cover.get((c['key'], k), [])
            where = 'Away itself' if k == 'away' else f'a session over {label}'
            if not rows:
                problem(f'capabilities.csv {c["key"]}', f'is of the first release for {where}, and no package delivers it')
            elif len(rows) > 1 and not all(r['note'] for r in rows):
                problem(f'delivers.csv {c["key"]}', f'is delivered for {where} by {", ".join(r["package"] for r in rows)}, '
                                                    'and not every one of those rows says which part of it')
    for p in packages:
        n = sum(1 for d in delivers if d['package'] == p['key'])
        if p['kind'] == 'build' and not n:
            problem(f'packages.csv {p["key"]}', 'is a build, and delivers no capability')
    return pk


class Build:
    """the architecture and the plan, with what each says of the capabilities"""

    def __init__(self, m, components, connections, carries, decisions, recs, packages, delivers):
        self.m, self.components, self.connections, self.recs = m, components, connections, recs
        self.packages, self.delivers = packages, delivers
        self.comp = {c['key']: c for c in components}
        self.first = [c for c in m.caps if m.state[c['key']] in FIRST]
        # a capability's components, and a component's capabilities of the first release
        self.carried = {r['capability']: [k for k in split(r['components']) if k in self.comp] for r in carries if r['capability'] in m.cap}
        self.notes = {r['capability']: r['note'] for r in carries}
        self.carries = {}
        for c in self.first:
            for k in self.carried.get(c['key'], []):
                self.carries.setdefault(k, []).append(c['key'])
        self.decisions = [d for d in decisions if d['adr'] in recs]
        self.governs, self.decides = {}, {}
        for d in self.decisions:
            for k in split(d['components']):
                self.governs.setdefault(k, []).append(d['adr'])
            for k in split(d['capabilities']):
                self.decides.setdefault(k, []).append(d['adr'])
        self.no = {p['key']: int(p['key'][1:]) for p in packages}
        self.pack = {p['key']: p for p in packages}
        # the rows that deliver a capability, with the ways each is for
        self.built_in = {}
        for d in delivers:
            c = m.cap.get(d['capability'])
            if c and d['package'] in self.pack:
                planned = [k for k, _ in WHERE if c[k] in FIRST]
                its = [k for k in (split(d['ways']) or planned) if k in planned]
                self.built_in.setdefault(d['capability'], []).append({'package': d['package'], 'ways': its, 'all': its == planned, 'note': d['note']})

    def short(self, key):
        return self.comp[key]['component'].split(':')[0]

    def record_link(self, n, base='./decisions/'):
        # a hyphen that doesn't break, so a narrow column keeps the number whole
        return f'[ADR\u2011{n}]({base}{self.recs[n]["file"]})'

    def package_link(self, key, base='../plan/'):
        p = self.pack[key]
        return f'[{self.no[key]}]({base}README.md#{slug(str(self.no[key]) + " " + p["package"])})'

    # ---- spec/architecture/README.md
    def picture(self):
        """the parts that run, and what each reaches: a lane a layer. what Away is built from and
        made with is left to the tables, and so is the way in, which everything from a browser
        passes through. what every one of Away's parts does alike says nothing of the shape
        and is left out; the page says so under the picture."""
        shown = ['browser', 'parts', 'rests', 'outside']
        comps = [c for c in self.components if c['layer'] in shown and c['release'] != 'later']
        keys = {c['key'] for c in comps}
        edges = {}
        for x in self.connections:
            if x['kind'] in ('uses', 'passes on') or x['release'] == 'later' or x['from'] not in keys or x['to'] not in keys:
                continue
            kinds = edges.setdefault((x['from'], x['to']), [])
            if x['kind'] not in kinds:
                kinds.append(x['kind'])
        parts = [c['key'] for c in comps if c['layer'] == 'parts']
        alike = False
        for other in [c['key'] for c in comps if c['layer'] != 'parts']:
            for out in (True, False):
                pairs = [(p, other) if out else (other, p) for p in parts]
                if len(parts) > 1 and all(pair in edges for pair in pairs) and len({tuple(edges[pair]) for pair in pairs}) == 1:
                    alike = True
                    for pair in pairs:
                        del edges[pair]
        # two components that reach each other in the same way share one arrow with two heads
        both = []
        for (a, b), kinds in list(edges.items()):
            if (a, b) in edges and (b, a) in edges and edges[(b, a)] == kinds:
                both.append((a, b, kinds))
                del edges[(a, b)], edges[(b, a)]
        linked = {k for pair in edges for k in pair} | {k for a, b, _ in both for k in (a, b)}

        def nid(k):
            return 'c_' + k.replace('-', '_')
        pic = ['```mermaid', 'flowchart TB']
        for layer, label, _ in LAYERS:
            its = [c for c in comps if c['layer'] == layer and c['key'] in linked]
            if its:
                pic.append(f'  subgraph l_{layer}["{label}"]')
                pic += [f'    {nid(c["key"])}["{self.short(c["key"])}"]' for c in its]
                pic.append('  end')
        for (a, b), kinds in edges.items():
            pic.append(f'  {nid(a)} -- "{", ".join(kinds)}" --> {nid(b)}')
        for a, b, kinds in both:
            pic.append(f'  {nid(a)} <-- "{", ".join(kinds)}" --> {nid(b)}')
        return '\n'.join(pic + ['```']) + '\n'

    def state(self):
        rows = []
        for layer, label, _ in LAYERS:
            n = Counter(c['release'] for c in self.components if c['layer'] == layer)
            rows.append([label, sum(n.values())] + [n[r] or '' for r in RELEASES])
        n = Counter(c['release'] for c in self.components)
        rows.append(['all', len(self.components)] + [n[r] or '' for r in RELEASES])
        first = table(rows, ['Layer', 'Components'] + [RELEASE_WORD[r] for r in RELEASES])
        n = Counter(x['kind'] for x in self.connections)
        second = table([[k, n[k], what] for k, what in CONNECT if n[k]] + [['all', len(self.connections), '']],
                       ['Connections', 'Rows', 'Means'])
        n = Counter(r['status'] for r in self.recs.values())
        third = table([[len(self.recs)] + [n[s] or '' for s in RECORD_STATES]], ['Decision records'] + RECORD_STATES)
        most = max((len(v) for v in self.carried.values()), default=0)
        alone = sum(1 for c in self.first if len(self.carried.get(c['key'], [])) == 1)
        return (f'{first}\n\n{second}\n\n{third}\n\nThe first release holds {len(self.first)} capabilities, and a component '
                f"of Away's own carries each: {alone} are carried by one component, the others by up to {most}.\n")

    # ---- spec/architecture/components.md
    def components_block(self):
        """a table a layer, kept narrow; what else is to say of a component is in a line under it"""
        out = []
        for layer, label, intro in LAYERS:
            its = [c for c in self.components if c['layer'] == layer]
            if not its:
                continue
            out.append(f'## {label}\n\n{intro}\n')
            rows = [[f'`{c["key"]}`', c['component'], c['kind'], RELEASE_WORD[c['release']],
                     len(self.carries.get(c['key'], [])) or '—'] for c in its]
            out.append(table(rows, ['Component', 'Is', 'Kind', 'Comes', 'Carries']) + '\n')
            said = []
            for c in its:
                bits = [c['note'] + '.'] if c['note'] else []
                if c['keeps']:
                    bits.append(f'Keeps: {c["keeps"]}.')
                if c['code']:
                    bits.append('Code: ' + ', '.join(f'`{x}`' for x in split(c['code'])) + '.')
                if c['runs']:
                    bits.append(f'Runs: {c["runs"]}.')
                if self.governs.get(c['key']):
                    bits.append('Governed by ' + ', '.join(self.record_link(n) for n in self.governs[c['key']]) + '.')
                if bits:
                    said.append(f'- `{c["key"]}`: ' + ' '.join(bits))
            if said:
                out.append('\n'.join(said) + '\n')
        return '\n'.join(out).rstrip() + '\n'

    # ---- spec/architecture/connections.md
    def connections_block(self):
        """a table a layer, by the component a connection starts from. nearly every one comes with
        the first release, so only the others say when they come, before their note."""
        when = {'built': 'There', 'later': 'Later'}
        out = []
        for layer, label, _ in LAYERS:
            its = [x for x in self.connections if self.comp.get(x['from'], {}).get('layer') == layer]
            if not its:
                continue
            out.append(f'## {label}\n')
            rows = []
            for x in its:
                note = ' '.join(b for b in ([when[x['release']] + '.'] if x['release'] in when else []) + ([x['note'] + '.'] if x['note'] else []))
                rows.append([f'`{x["from"]}`', x['kind'], f'`{x["to"]}`', x['on'] or '—', note or '—'])
            out.append(table(rows, ['From', '', 'To', 'On', 'Note']) + '\n')
        return '\n'.join(out).rstrip() + '\n'

    # ---- spec/architecture/carried.md
    def built_words(self, key, base='../plan/'):
        """the packages that deliver a capability: 6, or 9 (VNC), 10 (RDP)"""
        label = dict(WHERE)
        bits = []
        for d in sorted(self.built_in.get(key, []), key=lambda d: self.no[d['package']]):
            ways = '' if d['all'] else ' (' + ', '.join('Away' if k == 'away' else label[k] for k in d['ways']) + ')'
            bits.append(self.package_link(d['package'], base) + ways)
        return ', '.join(bits) or '—'

    def carried_block(self):
        out = []
        for p, label, page in PARTS:
            out.append(f'## {label[0].upper()}{label[1:]}\n')
            for a in [a for a in self.m.areas if a['part'] == p]:
                its = [c for c in self.first if c['area'] == a['key']]
                if not its:
                    continue
                out.append(f'### {a["area"]}\n')
                rows = [[c['capability'], ' '.join(f'`{k}`' for k in self.carried.get(c['key'], [])) or '—',
                         self.built_words(c['key']), f'`{c["key"]}`'] for c in its]
                out.append(table(rows, ['Capability', 'Carried by', 'Built in package', 'Key']) + '\n')
                said = [f'- *{c["capability"]}*: {self.notes[c["key"]]}.' for c in its if self.notes.get(c['key'])]
                if said:
                    out.append('\n'.join(said) + '\n')
        return '\n'.join(out).rstrip() + '\n'

    # ---- spec/architecture/decisions/README.md
    def decisions_block(self):
        """two tables: the records with their status and the packages that wait for them, which
        is what the owner decides by; then what each governs"""
        first, second = [], []
        for d in self.decisions:
            r = self.recs[d['adr']]
            link = f'[ADR\u2011{d["adr"]}](./{r["file"]})'
            used = [p['key'] for p in self.packages if d['adr'] in split(p['decisions'])]
            first.append([link, r['title'], r['status'], ', '.join(self.package_link(k, '../../plan/') for k in used) or '—'])
            second.append([link, ', '.join(f'`{k}`' for k in split(d['components'])),
                           ', '.join(f'`{k}`' for k in split(d['capabilities'])) or '—'])
        return (table(first, ['Record', 'Decision', 'Status', 'Work packages that rest on it']) + '\n\n## What each governs\n\n'
                'The components a record is about, and the capabilities it decides most. A component\'s key is '
                'explained on [components](../components.md), a capability\'s on [where each capability is carried](../carried.md).\n\n'
                + table(second, ['Record', 'Components', 'Capabilities']) + '\n')

    # ---- spec/plan/README.md
    def order_block(self):
        pic = ['```mermaid', 'flowchart TD']
        for p in self.packages:
            name = f'{self.no[p["key"]]} {p["package"]}'
            pic.append(f'  {p["key"]}(["{name}"])' if p['kind'] == 'proof' else f'  {p["key"]}["{name}"]')
        for p in self.packages:
            pic += [f'  {k} --> {p["key"]}' for k in split(p['after']) if k in self.pack]
        return '\n'.join(pic + ['```']) + '\n'

    def delivered(self, key):
        return [d for d in self.delivers if d['package'] == key and d['capability'] in self.m.cap]

    def packages_block(self):
        rows = []
        for p in self.packages:
            n = len(self.delivered(p['key']))
            rows.append([self.no[p['key']], f'[{p["package"]}](#{slug(str(self.no[p["key"]]) + " " + p["package"])})',
                         p['kind'], p['state'],
                         ', '.join(str(self.no[k]) for k in split(p['after']) if k in self.no) or '—',
                         ', '.join(self.record_link(n_, '../architecture/decisions/') for n_ in split(p['decisions']) if n_ in self.recs),
                         count(n, 'capability', 'capabilities') if n else '—'])
        builds = sum(1 for p in self.packages if p['kind'] == 'build')
        whole = sum(1 for c in self.first if all(any(k in d['ways'] for d in self.built_in.get(c['key'], []))
                                                 for k, _ in WHERE if c[k] in FIRST))
        return (table(rows, ['No.', 'Package', 'Is', 'State', 'After', 'Rests on', 'Delivers']) +
                f'\n\n{count(len(self.packages), "package")}: {count(len(self.packages) - builds, "proof")} and {count(builds, "build")}. '
                f'Together the builds deliver {whole} of the {len(self.first)} capabilities of the first release.\n')

    def by_package_block(self):
        label = dict(WHERE)
        out = []
        for p in self.packages:
            no = self.no[p['key']]
            out.append(f'### {no} {p["package"]}\n')
            after = [f'package {self.no[k]}' for k in split(p['after']) if k in self.no]
            rests = [self.record_link(n, '../architecture/decisions/') for n in split(p['decisions']) if n in self.recs]
            line = f'{dict(PACKAGE_KINDS).get(p["kind"], p["kind"]).capitalize()}'
            line += f', after {" and ".join(after)}.' if after else ', with nothing before it.'
            line += f' It rests on {", ".join(rests)}.' if rests else ''
            line += f' {p["note"]}.' if p['note'] else ''
            out.append(line + '\n')
            out.append(f'**Its gate.** {p["gate"]}\n')
            its = self.delivered(p['key'])
            if not its:
                continue
            rows = []
            for a in self.m.areas:
                bits = []
                for d in its:
                    c = self.m.cap[d['capability']]
                    if c['area'] != a['key']:
                        continue
                    planned = [k for k, _ in WHERE if c[k] in FIRST]
                    named = split(d['ways'])
                    ways = f' ({", ".join(label[k] for k in named if k in label)})' if named and named != planned else ''
                    bits.append(c['capability'] + ways + (f': {d["note"][0].lower()}{d["note"][1:]}' if d['note'] else ''))
                if bits:
                    rows.append([self.m.area_link(a, '../capabilities/'), '; '.join(bits)])
            out.append(table(rows, ['Area', 'What it delivers']) + '\n')
        return '\n'.join(out).rstrip() + '\n'

    # ---- spec/README.md
    def state_rows(self):
        n = Counter(c['release'] for c in self.components)
        status = Counter(r['status'] for r in self.recs.values())
        builds = sum(1 for p in self.packages if p['kind'] == 'build')
        return [
            ['Components', len(self.components), f'in {count(len(LAYERS), "layer")}: ' + said(n, RELEASES, RELEASE_WORD)],
            ['Connections', len(self.connections), f'of {count(len({x["kind"] for x in self.connections}), "kind")}'],
            ['Decision records', len(self.recs), said(status, RECORD_STATES, {s: s.lower() for s in RECORD_STATES})],
            ['Work packages', len(self.packages), f'{count(len(self.packages) - builds, "proof")} and {count(builds, "build")}: '
                                                  + said(Counter(p['state'] for p in self.packages), PACKAGE_STATES)],
        ]

    # ---- the model, for the site
    def export(self):
        return {
            'layers': [{'key': k, 'label': label, 'intro': intro} for k, label, intro in LAYERS],
            'components': [{'key': c['key'], 'component': c['component'], 'name': self.short(c['key']), 'layer': c['layer'],
                            'kind': c['kind'], 'release': c['release'], 'carries': self.carries.get(c['key'], []),
                            'decisions': self.governs.get(c['key'], [])} for c in self.components],
            'decisions': [{'adr': d['adr'], 'title': self.recs[d['adr']]['title'], 'status': self.recs[d['adr']]['status'],
                           'page': f'spec/architecture/decisions/{self.recs[d["adr"]]["file"]}'} for d in self.decisions],
            'packages': [{'key': p['key'], 'no': self.no[p['key']], 'package': p['package'], 'kind': p['kind'], 'state': p['state'],
                          'anchor': slug(str(self.no[p['key']]) + ' ' + p['package'])} for p in self.packages],
            'carried': self.carried,
            'builtIn': self.built_in,
            'decides': self.decides,
        }


def fill(path, blocks, write):
    text = open(path, encoding='utf-8').read() if os.path.exists(path) else ''
    new = text
    for name, body in blocks.items():
        pat = re.compile(rf'(<!-- generated: {re.escape(name)} -->\n).*?(<!-- end: {re.escape(name)} -->)', re.S)
        if not pat.search(new):
            problem(os.path.relpath(path, ROOT), f'has no block "{name}"')
            continue
        new = pat.sub(lambda m: m.group(1) + body + m.group(2), new)
    if new != text:
        if write:
            open(path, 'w', encoding='utf-8').write(new)
        else:
            problem(os.path.relpath(path, ROOT), 'its generated blocks are not what the model says: run scripts/model.py')


def main():
    write = '--check' not in sys.argv
    goals = read('goals.csv', GOAL_COLS, GOALS)
    qualities = read('qualities.csv', QUALITY_COLS, GOALS)
    areas = read('areas.csv', AREA_COLS)
    caps = read('capabilities.csv', CAP_COLS)
    expectations = read('expectations.csv', EXPECT_COLS)
    goal_keys = check_goals(goals, qualities)
    area_keys, cap_keys = check_model(goal_keys, areas, caps, expectations, qualities)
    chaps = chapters()
    screens = check_screens(read('screens.csv', SCREEN_COLS, SCREENS), chaps, cap_keys)
    # what the check has named as unknown is left out of the pages
    m = Model(goals, [q for q in qualities if q['goal'] in goal_keys],
              [a for a in areas if a['goal'] in goal_keys and a['part'] in [p for p, _, _ in PARTS]],
              [c for c in caps if c['area'] in area_keys and all(c[k] in STATUS for k, _ in WHERE)],
              [e for e in expectations if e['goal'] in goal_keys], cap_keys, screens, chaps)
    m.caps = [c for c in m.caps if c['area'] in m.area]
    # how it is built, and in what order
    components = read('components.csv', COMPONENT_COLS, ARCH)
    connections = read('connections.csv', CONNECTION_COLS, ARCH)
    carries = read('carries.csv', CARRY_COLS, ARCH)
    decisions = read('decisions.csv', DECISION_COLS, ARCH)
    packages = read('packages.csv', PACKAGE_COLS, PLAN)
    delivers = read('delivers.csv', DELIVER_COLS, PLAN)
    recs = read_records()
    first = [c['key'] for c in m.caps if m.state[c['key']] in FIRST]
    comp = check_architecture(components, connections, carries, decisions, recs, cap_keys, first)
    check_plan(packages, delivers, recs, cap_keys, m.caps)
    b = Build(m, [c for c in components if c['layer'] in [k for k, _, _ in LAYERS] and c['release'] in RELEASES],
              [x for x in connections if x['from'] in comp and x['to'] in comp], carries, decisions, recs,
              [p for p in packages if re.match(r'^p\d\d$', p['key'])], delivers)
    m.build = b
    summary = (f'{len(goals)} goals, {len(qualities)} qualities; {len(caps)} capabilities in {len(areas)} areas, '
               f'{len(expectations)} expectations; {len(screens)} figures in {len({f["chapter"] for f in screens})} chapters; '
               f'{len(components)} components, {len(connections)} connections, {len(recs)} decision records; '
               f'{len(packages)} work packages')
    if '--json' in sys.argv:
        at = sys.argv.index('--json') + 1
        if at >= len(sys.argv):
            sys.exit('--json needs the file to write')
        m.export(sys.argv[at])
        print(f'{len(problems)} problems; {summary} written to {sys.argv[at]}')
        return
    fill(os.path.join(SPEC, 'README.md'), {'state': m.state_block()}, write)
    fill(os.path.join(GOALS, 'README.md'), m.goals_blocks(), write)
    fill(os.path.join(CAP, 'README.md'), {'overview': m.overview(), 'expected': m.expected_counts()}, write)
    fill(os.path.join(CAP, 'expectations.md'), {'expectations': m.expectations_block()}, write)
    fill(os.path.join(CAP, 'left-out.md'), {'left-out': m.left_out_block()}, write)
    for p, _, page in PARTS:
        fill(os.path.join(CAP, page), {'areas': m.area_blocks(p)}, write)
    fill(os.path.join(SCREENS, 'README.md'), {'figures': m.figures_block(), 'not-drawn': m.not_drawn_block()}, write)
    for c in m.chapters:
        if any(f['chapter'] == c['key'] for f in m.screens):
            fill(os.path.join(SCREENS, c['file']), {'draws': m.draws_block(c['key'])}, write)
    fill(os.path.join(ARCH, 'README.md'), {'picture': b.picture(), 'state': b.state()}, write)
    fill(os.path.join(ARCH, 'components.md'), {'components': b.components_block()}, write)
    fill(os.path.join(ARCH, 'connections.md'), {'connections': b.connections_block()}, write)
    fill(os.path.join(ARCH, 'carried.md'), {'carried': b.carried_block()}, write)
    fill(os.path.join(RECORDS, 'README.md'), {'decisions': b.decisions_block()}, write)
    fill(os.path.join(PLAN, 'README.md'), {'order': b.order_block(), 'packages': b.packages_block(),
                                           'by-package': b.by_package_block()}, write)
    for p in problems:
        print(p)
    print(f'{len(problems)} problems; {summary}')
    sys.exit(1 if problems else 0)


if __name__ == '__main__':
    main()
