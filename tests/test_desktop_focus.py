"""Exercise focus callbacks and deadlines without sleeps."""

from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
RUNNER = r"""
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const source = readFileSync('clients/desktop/ui-tests/common.js', 'utf8');
const start = source.indexOf('const documentFocused =');
const end = source.indexOf('// With axe-core loaded', start);
assert(start >= 0 && end > start);
let focused = false;
const listeners = new Set();
const timers = new Map();
let token = 0;
const context = {
  document: {hasFocus: () => focused},
  window: {
    addEventListener: (_type, listener) => listeners.add(listener),
    removeEventListener: (_type, listener) => listeners.delete(listener),
  },
  startTimer: (callback) => {timers.set(++token, callback); return token;},
  stopTimer: (id) => timers.delete(id),
};
vm.runInNewContext(
  source.slice(start, end) + ';globalThis.wait = documentFocused;', context,
);
let settled = false;
const pending = context.wait().then(() => {settled = true;});
await Promise.resolve();
assert.equal(settled, false);
assert.equal(listeners.size, 1);
focused = true;
for (const listener of [...listeners]) listener();
await pending;
assert.equal(listeners.size, 0);
assert.equal(timers.size, 0);
await context.wait();
assert.equal(listeners.size, 0);
assert.equal(timers.size, 0);
focused = false;
const expired = context.wait();
const expire = [...timers.values()][0];
timers.clear();
expire();
await assert.rejects(expired, /native document focus deadline/);
assert.equal(listeners.size, 0);
"""


class DesktopFocusTests(unittest.TestCase):
    def test_callback_immediate_readiness_and_deadline_cleanup(self) -> None:
        process = subprocess.run(
            ["node", "--input-type=module"],
            input=RUNNER,
            text=True,
            capture_output=True,
            cwd=ROOT,
            check=False,
        )
        self.assertEqual(
            process.returncode, 0, process.stdout + process.stderr
        )


if __name__ == "__main__":
    unittest.main()
