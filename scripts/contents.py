#!/usr/bin/env python3
"""contents.py [--output PATH] [--parts PATH [--check]] : the list of what Away's image
holds, written as a CycloneDX document: the program, every library compiled into it with
its version, its licence and the checksum of its source, and the compiler's own libraries.

With --parts it writes the same list in short, each part with its version and its licence,
to the file the program holds and shows on its own page about itself. With --check it
writes nothing and fails when that file is not what the lock files say.

The image holds one file, the program, so what it holds is what the program was built
from. That is read from cargo itself, for the processors the image is published for:
only what is compiled for them is listed, not what a build for another system would take.
The pages are part of the program, so the libraries they are built from are listed too,
from web/package-lock.json.

It needs Python 3 and cargo, and nothing else."""
import base64
import hashlib
import json
import os
import subprocess
import sys
import uuid

TARGETS = ['x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl']


def cargo(*args):
    return subprocess.run(['cargo', *args], check=True, capture_output=True, text=True).stdout


def compiled(target):
    """the packages that go into the program for one processor: the metadata, and the ids
    reached from the program by what it is built from, not by what its tests or its build use"""
    meta = json.loads(cargo('metadata', '--locked', '--format-version', '1', '--filter-platform', target))
    nodes = {n['id']: n for n in meta['resolve']['nodes']}
    # a library that only writes code while the program is compiled is not in the program
    at_compile_time = {p['id'] for p in meta['packages'] if any('proc-macro' in t['kind'] for t in p['targets'])}
    start = next(p['id'] for p in meta['packages'] if p['name'] == 'away' and p['source'] is None)
    seen, todo = set(), [start]
    while todo:
        at = todo.pop()
        if at in seen or at in at_compile_time:
            continue
        seen.add(at)
        for dep in nodes[at]['deps']:
            if any(kind['kind'] is None for kind in dep['dep_kinds']):
                todo.append(dep['pkg'])
    return meta, seen


def main():
    output = sys.argv[sys.argv.index('--output') + 1] if '--output' in sys.argv else None
    packages, held = {}, set()
    for target in TARGETS:
        meta, ids = compiled(target)
        held |= ids
        packages.update({p['id']: p for p in meta['packages']})
    checksums = {}
    for line in open('Cargo.lock', encoding='utf-8').read().split('[[package]]'):
        fields = dict(part.split(' = ', 1) for part in line.strip().splitlines() if ' = ' in part and not part.startswith(' '))
        if 'checksum' in fields:
            checksums[(fields['name'].strip('"'), fields['version'].strip('"'))] = fields['checksum'].strip('"')
    own = sorted((packages[i] for i in held if packages[i]['source'] is None), key=lambda p: p['name'])
    others = sorted((packages[i] for i in held if packages[i]['source'] is not None), key=lambda p: (p['name'], p['version']))
    program = next(p for p in own if p['name'] == 'away')
    compiler = cargo('--version').split()[1]

    def component(p, kind='library'):
        out = {'type': kind, 'name': p['name'], 'version': p['version'], 'purl': f'pkg:cargo/{p["name"]}@{p["version"]}',
               'bom-ref': f'pkg:cargo/{p["name"]}@{p["version"]}'}
        if p.get('description'):
            out['description'] = ' '.join(p['description'].split())
        if p.get('license'):
            out['licenses'] = [{'expression': p['license']}]
        checksum = checksums.get((p['name'], p['version']))
        if checksum:
            out['hashes'] = [{'alg': 'SHA-256', 'content': checksum}]
        return out

    # the pages are part of the program, and so is every library they are built from
    in_pages = []
    lock = json.load(open(os.path.join('web', 'package-lock.json'), encoding='utf-8'))['packages']
    for path, entry in sorted(lock.items()):
        # what only builds the pages, or only tells the compiler of types, is not in them
        if not path or entry.get('dev') or entry.get('devOptional'):
            continue
        name = path.split('node_modules/')[-1]
        ref = f'pkg:npm/{name.replace("@", "%40")}@{entry["version"]}'
        part = {'type': 'library', 'name': name, 'version': entry['version'], 'purl': ref, 'bom-ref': ref}
        if entry.get('license'):
            part['licenses'] = [{'expression': entry['license']}]
        kind, _, letters = entry.get('integrity', '').partition('-')
        if kind == 'sha512' and letters:
            part['hashes'] = [{'alg': 'SHA-512', 'content': base64.b64decode(letters).hex()}]
        in_pages.append(part)

    components = [component(p) for p in own if p['name'] != 'away'] + [component(p) for p in others] + in_pages + [
        {'type': 'library', 'name': 'rust-std', 'version': compiler, 'bom-ref': f'pkg:generic/rust-std@{compiler}',
         'description': "The compiler's own libraries, which every Rust program holds.",
         'licenses': [{'expression': 'MIT OR Apache-2.0'}]},
        {'type': 'library', 'name': 'musl', 'bom-ref': 'pkg:generic/musl',
         'description': "The C library the program is linked with, as the compiler's libraries for this target bring it.",
         'licenses': [{'expression': 'MIT'}]},
    ]
    if '--parts' in sys.argv:
        return parts(sys.argv[sys.argv.index('--parts') + 1], components, '--check' in sys.argv)
    document = {
        'bomFormat': 'CycloneDX',
        'specVersion': '1.5',
        'version': 1,
        'metadata': {'component': component(program, 'application'), 'properties': [{'name': 'away:built-for', 'value': ', '.join(TARGETS)}]},
        'components': components,
    }
    # the document's number is made from what it lists, so the same contents give the same
    # document, and tools that ask for a number find one
    listed = hashlib.sha256(json.dumps(document, sort_keys=True).encode()).hexdigest()
    document = {'bomFormat': document.pop('bomFormat'), 'specVersion': document.pop('specVersion'),
                'serialNumber': f'urn:uuid:{uuid.uuid5(uuid.NAMESPACE_OID, listed)}', **document}
    text = json.dumps(document, indent=1, ensure_ascii=False) + '\n'
    if output:
        with open(output, 'w', encoding='utf-8') as fh:
            fh.write(text)
        print(f'{len(components)} parts beside the program: {len(own) - 1} of Away, {len(others)} libraries, {len(in_pages)} of the pages, '
              f'the compiler\'s and the C library; written to {output}')
    else:
        sys.stdout.write(text)


def parts(path, components, check):
    """what the program is built from, in short: one part a line, by name"""
    def licence(part):
        # one way of writing a choice, whichever way a library wrote it
        return ' OR '.join(piece.strip() for piece in part.get('licenses', [{}])[0].get('expression', '').split('/'))

    listed = sorted(({'name': p['name'], 'version': p.get('version', ''), 'licence': licence(p)} for p in components
                     if not p['name'].startswith('away-')), key=lambda p: (p['name'].lower(), p['version']))
    text = '[\n' + ',\n'.join(json.dumps(p, ensure_ascii=False) for p in listed) + '\n]\n'
    if check:
        held = open(path, encoding='utf-8').read() if os.path.exists(path) else ''
        if held != text:
            print(f'{path} is not what Cargo.lock and web/package-lock.json say: run python3 scripts/contents.py --parts {path}')
            return 1
        print(f'0 problems; {len(listed)} parts in {path}')
        return 0
    with open(path, 'w', encoding='utf-8') as fh:
        fh.write(text)
    print(f'{len(listed)} parts written to {path}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
