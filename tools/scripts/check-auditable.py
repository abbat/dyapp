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


FIX = "docs/development/code-quality.md#dependency-checks-cargo-deny"


def main(raw=False):
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
    if raw:
        print(f'{binary}: {len(packages)} packages in the embedded '
              'dependency list')
    else:
        print('OK auditable')


if __name__ == '__main__':
    raw = '--raw' in sys.argv[1:]
    if raw:
        sys.argv.remove('--raw')
        main(raw=True)
    else:
        try:
            main()
        except (Exception, SystemExit) as error:
            # This script is copied alone into the release image. Keep its
            # frontend self-contained; tests bind FIX to check-output.json.
            message = str(error)
            directory = os.environ.get('DYAPP_CHECK_LOG_DIR',
                                       '/tmp/ai/check-output')
            log = None
            try:
                os.makedirs(directory, exist_ok=True)
                with tempfile.NamedTemporaryFile(
                        mode='w', prefix='auditable-', suffix='.log',
                        dir=directory, delete=False) as output:
                    output.write(message + '\n')
                    log = output.name
            except OSError as log_error:
                message += f'\nCannot retain diagnostic: {log_error}'
            cause = message.splitlines()[0][:512] if message else 'exit 1'
            print(f'FAIL auditable: {cause}\nFIX: {FIX}\nDETAILS:')
            print(message[:65536])
            if len(message) > 65536:
                print('[diagnostic truncated; see raw log]')
            if log:
                print(f'{log}: raw captured output')
            sys.exit(1)
