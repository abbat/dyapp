"""Exercise focus, paint and calibration callbacks without sleeps."""

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

const axeSource = readFileSync(
  'clients/desktop/ui-tests/criteria-axe.js', 'utf8',
);
const paintStart = axeSource.indexOf('const paintReady =');
const paintEnd = axeSource.indexOf("// axe's WCAG", paintStart);
assert(paintStart >= 0 && paintEnd > paintStart);
const paintTimers = new Map();
const paintFrames = new Map();
let paintToken = 0;
const paintContext = {
  setTimeout: (callback) => {
    paintTimers.set(++paintToken, callback); return paintToken;
  },
  clearTimeout: (id) => paintTimers.delete(id),
  requestAnimationFrame: (callback) => {
    paintFrames.set(++paintToken, callback); return paintToken;
  },
  cancelAnimationFrame: (id) => paintFrames.delete(id),
};
vm.runInNewContext(
  axeSource.slice(paintStart, paintEnd) +
    ';globalThis.paint = paintReady;', paintContext,
);
const advanceFrame = () => {
  const [id, callback] = [...paintFrames][0];
  paintFrames.delete(id);
  callback();
};
const painted = paintContext.paint();
assert.equal(paintTimers.size, 1);
advanceFrame();
assert.equal(paintTimers.size, 1);
advanceFrame();
await painted;
assert.equal(paintTimers.size, 0);
assert.equal(paintFrames.size, 0);
const latePaint = paintContext.paint();
const paintDeadline = [...paintTimers.values()][0];
paintTimers.clear();
paintDeadline();
await assert.rejects(latePaint, /accessibility paint deadline/);
assert.equal(paintFrames.size, 0);
let reachFrame;
let releaseFrame;
const reachedFrame = new Promise((resolve) => {reachFrame = resolve;});
const frameGate = new Promise((resolve) => {releaseFrame = resolve;});
const calibrationCalls = [];
const calibrationContext = {
  documentFocused: async () => {calibrationCalls.push('focus');},
  document: {fonts: {ready: Promise.resolve()}},
  paintReady: () => {reachFrame(); return frameGate;},
  smoke: {
    accessibilitySelfCheck: async () => {calibrationCalls.push('contrast');},
    modalAccessibilitySelfCheck: async () => {calibrationCalls.push('modal');},
  },
};
const calibrationStart = source.indexOf('  let calibration;');
const calibrationEnd = source.indexOf(
  '  return async (name)', calibrationStart,
);
assert(calibrationStart >= 0 && calibrationEnd > calibrationStart);
vm.runInNewContext(
  source.slice(calibrationStart, calibrationEnd), calibrationContext,
);
const calibration = calibrationContext.smoke.calibrateAccessibility();
assert.equal(
  calibrationContext.smoke.calibrateAccessibility(), calibration,
);
await reachedFrame;
assert.deepEqual(calibrationCalls, ['focus']);
releaseFrame();
await calibration;
assert.deepEqual(calibrationCalls, ['focus', 'contrast', 'modal']);
assert.equal(
  calibrationContext.smoke.calibrateAccessibility(), calibration,
);
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
