#!/usr/bin/env python3
"""libraries.py [--check] : the libraries Away is built from directly, each with what it is
for, checked, and their table written into docs/libraries.md.

Cargo.toml names them under [workspace.dependencies], one a line, with what each is for
after a #:

  tokio = { version = "1", features = ["net"] } # runs many connections at once

It checks, and names every problem it finds:
  - every library of someone else's has a line that says what it is for;
  - no part's own Cargo.toml names a library with a version of its own: each takes it from
    the workspace, so a library is named once;
  - the compiler the image is built with is the one rust-toolchain.toml names;
  - the table in docs/libraries.md is what Cargo.toml and Cargo.lock say (--check), else it
    is written.

It needs Python 3.11 or newer, and nothing else."""
import os
import re
import sys
import tomllib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
PAGE = os.path.join(ROOT, 'docs', 'libraries.md')
BLOCK = re.compile(r'(<!-- generated: libraries -->\n).*?(<!-- end: libraries -->)', re.S)

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


def table(libraries, held):
    lines = ['| Library | What it is for | Version in this build | With |', '|---|---|---|---|']
    for lib in libraries:
        version = ', '.join(held.get(lib['name'], [])) or 'not in Cargo.lock'
        features = ', '.join(f'`{f}`' for f in lib['features']) or '—'
        lines.append(f'| [{lib["name"]}](https://crates.io/crates/{lib["name"]}) | {lib["for"]} | {version} | {features} |')
    others = sum(len(v) for v in held.values()) - sum(len(held.get(lib['name'], [])) for lib in libraries)
    lines.append('')
    lines.append(f'Away names {len(libraries)} libraries. They bring {others} more with them, which makes '
                 f'{sum(len(v) for v in held.values())} in all.')
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
        if new != text:
            if write:
                with open(PAGE, 'w', encoding='utf-8') as fh:
                    fh.write(new)
            else:
                problem('docs/libraries.md', 'its table is not what Cargo.toml and Cargo.lock say: run scripts/libraries.py')
    for p in problems:
        print(p)
    print(f'{len(problems)} problems; {len(libraries)} libraries named, {sum(len(v) for v in held.values())} in all')
    sys.exit(1 if problems else 0)


if __name__ == '__main__':
    main()
