#!/usr/bin/env python3
"""model.py [--check] [--json PATH] : the goals of spec/goals and the capability model of
spec/capabilities, checked, and their tables written into the pages.

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
        with open(path, 'w', encoding='utf-8') as fh:
            json.dump(model, fh, ensure_ascii=False, indent=1)
            fh.write('\n')


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
    summary = (f'{len(goals)} goals, {len(qualities)} qualities; {len(caps)} capabilities in {len(areas)} areas, '
               f'{len(expectations)} expectations; {len(screens)} figures in {len({f["chapter"] for f in screens})} chapters')
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
    for p in problems:
        print(p)
    print(f'{len(problems)} problems; {summary}')
    sys.exit(1 if problems else 0)


if __name__ == '__main__':
    main()
