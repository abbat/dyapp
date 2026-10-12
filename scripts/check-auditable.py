#!/usr/bin/env python3
"""Checks that a release binary carries its cargo-auditable dependency list.

Usage: check-auditable.py <binary> <Cargo.lock>. Reads the zlib-compressed
JSON in the ELF section .dep-v0 (with objcopy) and fails unless it names
dyapp-bootstrap and every listed package matches a name and version in
Cargo.lock.
"""
import json
import os
import subprocess
import sys
import tempfile
import tomllib
import zlib


def inventory(binary):
    with tempfile.TemporaryDirectory() as tmp:
        section = os.path.join(tmp, 'dep-v0')
        copy = subprocess.run(
            ['objcopy', '--dump-section', f'.dep-v0={section}', binary,
             os.path.join(tmp, 'copy')],
            capture_output=True, text=True)
        # A missing section is only a warning to objcopy: it writes no file.
        if copy.returncode != 0 or not os.path.exists(section):
            sys.exit(f'{binary}: no .dep-v0 section, '
                     f'not built with cargo auditable\n{copy.stderr}')
        with open(section, 'rb') as f:
            data = f.read()
    return json.loads(zlib.decompress(data))['packages']


def main():
    binary, lock = sys.argv[1:]
    with open(lock, 'rb') as f:
        locked = {(p['name'], p['version'])
                  for p in tomllib.load(f)['package']}
    packages = {(p['name'], p['version']) for p in inventory(binary)}
    unknown = packages - locked
    if unknown:
        sys.exit(f'{binary}: not in {lock}: {sorted(unknown)}')
    if not any(name == 'dyapp-bootstrap' for name, _ in packages):
        sys.exit(f'{binary}: dyapp-bootstrap missing from the dependency list')
    print(f'{binary}: {len(packages)} packages in the embedded '
          'dependency list')


if __name__ == '__main__':
    main()
