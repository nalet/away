#!/usr/bin/env python3
"""libraries.py [--check] : the libraries Away is built from directly, each with what it is
for, checked, and their table written into docs/libraries.md.

Cargo.toml names them under [workspace.dependencies], one a line, with what each is for
after a #:

  tokio = { version = "1", features = ["net"] } # runs many connections at once

The pages' libraries are named in web/package.json, and what each is for under "whatFor"
there.

It checks, and names every problem it finds:
  - every library of someone else's has a line that says what it is for;
  - every library the pages hold has a licence that deny.toml allows;
  - no part's own Cargo.toml names a library with a version of its own: each takes it from
    the workspace, so a library is named once;
  - the compiler the image is built with is the one rust-toolchain.toml names;
  - the table in docs/libraries.md is what Cargo.toml and Cargo.lock say (--check), else it
    is written.

It needs Python 3.11 or newer, and nothing else."""
import json
import os
import re
import sys
import tomllib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
PAGE = os.path.join(ROOT, 'docs', 'libraries.md')
BLOCK = re.compile(r'(<!-- generated: libraries -->\n).*?(<!-- end: libraries -->)', re.S)
PAGES = re.compile(r'(<!-- generated: pages -->\n).*?(<!-- end: pages -->)', re.S)

problems = []


def problem(where, what):
    problems.append(f'{where}: {what}')


def read(*path):
    with open(os.path.join(ROOT, *path), encoding='utf-8') as fh:
        return fh.read()


def direct():
    """the libraries of someone else's that the workspace names: name, version, features, what for"""
    text = read('Cargo.toml')
    named = tomllib.loads(text)['workspace']['dependencies']
    reasons = {}
    inside = False
    for line in text.splitlines():
        if line.startswith('['):
            inside = line.strip() == '[workspace.dependencies]'
            continue
        m = re.match(r'([A-Za-z0-9_-]+)\s*=\s*(.*)$', line) if inside else None
        if m:
            # what follows the last # outside a quoted text
            outside = re.sub(r'"[^"]*"', '""', m.group(2))
            reasons[m.group(1)] = outside.split('#', 1)[1].strip() if '#' in outside else ''
    out = []
    for name, spec in named.items():
        spec = {'version': spec} if isinstance(spec, str) else spec
        if 'path' in spec:
            continue  # Away's own
        if not reasons.get(name):
            problem(f'Cargo.toml {name}', 'no "# what it is for" at the end of its line')
        if 'version' not in spec:
            problem(f'Cargo.toml {name}', 'names no version')
        out.append({'name': name, 'wanted': spec.get('version', ''), 'features': spec.get('features', []), 'for': reasons.get(name, '')})
    return out


def locked():
    """every library Cargo.lock holds that is not Away's own: name -> versions"""
    out = {}
    for package in tomllib.loads(read('Cargo.lock')).get('package', []):
        if package.get('source'):
            out.setdefault(package['name'], []).append(package['version'])
    return out


def check_members(names):
    workspace = tomllib.loads(read('Cargo.toml'))['workspace']
    for member in workspace['members']:
        manifest = tomllib.loads(read(member, 'Cargo.toml'))
        for kind in ('dependencies', 'dev-dependencies', 'build-dependencies'):
            for name, spec in manifest.get(kind, {}).items():
                if not (isinstance(spec, dict) and spec.get('workspace') is True):
                    problem(f'{member}/Cargo.toml {name}', 'is not taken from the workspace: write `' + name + '.workspace = true`')
                elif name not in names and not name.startswith('away'):
                    problem(f'{member}/Cargo.toml {name}', 'is not under [workspace.dependencies] of the workspace')


def check_compiler():
    wanted = tomllib.loads(read('rust-toolchain.toml'))['toolchain']['channel']
    m = re.search(r'^FROM rust:([0-9.]+)-', read('Dockerfile'), re.M)
    if not m:
        problem('Dockerfile', 'does not begin from an image of the compiler, rust:<version>-...')
    elif not (wanted == m.group(1) or wanted.startswith(m.group(1) + '.')):
        problem('Dockerfile', f'builds with the compiler {m.group(1)}, and rust-toolchain.toml names {wanted}')


def pages():
    """the libraries the pages name, and every one their lock file holds: (named, held), where
    named is a list of name, version, what for, and whether it is only for building, and held
    is every package that goes into the pages with its version and its licence"""
    package = json.loads(read('web', 'package.json'))
    lock = json.loads(read('web', 'package-lock.json'))['packages']
    reasons = package.get('whatFor', {})
    named = []
    for kind, building in (('dependencies', False), ('devDependencies', True)):
        for name in package.get(kind, {}):
            if not reasons.get(name):
                problem(f'web/package.json {name}', 'has no entry under "whatFor" that says what it is for')
            entry = lock.get(f'node_modules/{name}')
            if entry is None:
                problem(f'web/package.json {name}', 'is not in web/package-lock.json: run npm install in web/')
            named.append({'name': name, 'version': (entry or {}).get('version', 'not in the lock file'), 'for': reasons.get(name, ''), 'building': building})
    for name in reasons:
        if name not in package.get('dependencies', {}) and name not in package.get('devDependencies', {}):
            problem(f'web/package.json whatFor {name}', 'says what a library is for that the pages no longer name')
    allowed = set(tomllib.loads(read('deny.toml'))['licenses']['allow'])
    held = []
    for path, entry in lock.items():
        # what only builds the pages, or only tells the compiler of types, is not in them
        if not path or entry.get('dev') or entry.get('devOptional'):
            continue
        name = path.split('node_modules/')[-1]
        licence = entry.get('license', '')
        if licence not in allowed:
            problem(f'web/package-lock.json {name}', f'has the licence "{licence}", which deny.toml does not allow')
        held.append({'name': name, 'version': entry.get('version', ''), 'licence': licence})
    return named, held


def pages_table(named, held):
    lines = ['| Library | What it is for | Version in this build | In the pages |', '|---|---|---|---|']
    for lib in named:
        lines.append(f'| [{lib["name"]}](https://www.npmjs.com/package/{lib["name"]}) | {lib["for"]} | {lib["version"]} | '
                     f'{"no, it only builds them" if lib["building"] else "yes"} |')
    lines.append('')
    lines.append(f'The pages name {len(named)} libraries, {sum(1 for lib in named if not lib["building"])} of which are in them. '
                 f'With what those bring with them, the pages hold {len(held)} in all, each in the version web/package-lock.json names.')
    return '\n'.join(lines) + '\n'


def table(libraries, held):
    lines = ['| Library | What it is for | Version in this build | With |', '|---|---|---|---|']
    for lib in libraries:
        version = ', '.join(held.get(lib['name'], [])) or 'not in Cargo.lock'
        features = ', '.join(f'`{f}`' for f in lib['features']) or '—'
        lines.append(f'| [{lib["name"]}](https://crates.io/crates/{lib["name"]}) | {lib["for"]} | {version} | {features} |')
    lines.append('')
    lines.append(f'Away names {len(libraries)} libraries. With what they bring with them, a build holds '
                 f'{sum(len(v) for v in held.values())} in all, each in the version Cargo.lock names.')
    return '\n'.join(lines) + '\n'


def main():
    write = '--check' not in sys.argv
    libraries = direct()
    held = locked()
    for lib in libraries:
        if lib['name'] not in held:
            problem(f'Cargo.toml {lib["name"]}', 'is not in Cargo.lock: run cargo build')
    check_members({lib['name'] for lib in libraries})
    check_compiler()
    text = read('docs', 'libraries.md') if os.path.exists(PAGE) else ''
    if not BLOCK.search(text):
        problem('docs/libraries.md', 'has no block "libraries"')
    else:
        new = BLOCK.sub(lambda m: m.group(1) + table(libraries, held) + m.group(2), text)
        named, in_pages = pages()
        if not PAGES.search(new):
            problem('docs/libraries.md', 'has no block "pages"')
        new = PAGES.sub(lambda m: m.group(1) + pages_table(named, in_pages) + m.group(2), new)
        if new != text:
            if write:
                with open(PAGE, 'w', encoding='utf-8') as fh:
                    fh.write(new)
            else:
                problem('docs/libraries.md', 'its tables are not what Cargo.toml, Cargo.lock and web/package.json say: run scripts/libraries.py')
    for p in problems:
        print(p)
    print(f'{len(problems)} problems; {len(libraries)} libraries named, {sum(len(v) for v in held.values())} in all')
    sys.exit(1 if problems else 0)


if __name__ == '__main__':
    main()
