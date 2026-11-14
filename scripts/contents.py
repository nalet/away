#!/usr/bin/env python3
"""contents.py [--output PATH] : the list of what Away's image holds, written as a CycloneDX
document: the program, every library compiled into it with its version, its licence and
the checksum of its source, and the compiler's own libraries.

The image holds one file, the program, so what it holds is what the program was built
from. That is read from cargo itself, for the processors the image is published for:
only what is compiled for them is listed, not what a build for another system would take.

It needs Python 3 and cargo, and nothing else."""
import json
import subprocess
import sys

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

    components = [component(p) for p in own if p['name'] != 'away'] + [component(p) for p in others] + [
        {'type': 'library', 'name': 'rust-std', 'version': compiler, 'bom-ref': f'pkg:generic/rust-std@{compiler}',
         'description': "The compiler's own libraries, which every Rust program holds.",
         'licenses': [{'expression': 'MIT OR Apache-2.0'}]},
        {'type': 'library', 'name': 'musl', 'bom-ref': 'pkg:generic/musl',
         'description': "The C library the program is linked with, as the compiler's libraries for this target bring it.",
         'licenses': [{'expression': 'MIT'}]},
    ]
    document = {
        'bomFormat': 'CycloneDX',
        'specVersion': '1.5',
        'version': 1,
        'metadata': {'component': component(program, 'application'), 'properties': [{'name': 'away:built-for', 'value': ', '.join(TARGETS)}]},
        'components': components,
    }
    text = json.dumps(document, indent=1, ensure_ascii=False) + '\n'
    if output:
        with open(output, 'w', encoding='utf-8') as fh:
            fh.write(text)
        print(f'{len(components)} parts beside the program: {len(own) - 1} of Away, {len(others)} libraries, the compiler\'s and the C library; written to {output}')
    else:
        sys.stdout.write(text)


if __name__ == '__main__':
    main()
