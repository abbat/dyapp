"""Ready-screen geometry follows completion signals, never a frame count."""

from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
RUNNER = r"""
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const source = readFileSync('clients/desktop/ui-tests/ready-step.js', 'utf8');
const start = source.indexOf('  const readyRect =');
const end = source.indexOf('  const checkBackup =', start);
assert(start >= 0 && end > start);
let observer;
let disconnected = 0;
const listeners = new Map();
const timers = new Map();
let token = 0;
let rect = {x: 37, y: -1200, width: 100, height: 40};
let top = null;
let scrolls = 0;
const area = {
  getBoundingClientRect: () => ({...rect}),
  scrollIntoView: () => {scrolls++;},
};
const context = {
  innerWidth: 600, innerHeight: 400, scrollY: 1232,
  document: {elementFromPoint: (x, y) => {
    assert.equal(x, rect.x + rect.width / 2);
    assert.equal(y, rect.y + rect.height / 2);
    return top;
  }},
  window: {
    addEventListener: (name, callback) => listeners.set(name, callback),
    removeEventListener: (name, callback) => {
      assert.equal(listeners.get(name), callback);
      listeners.delete(name);
    },
  },
  IntersectionObserver: class {
    constructor(callback) {observer = callback;}
    observe(target) {assert.equal(target, area);}
    disconnect() {disconnected++;}
  },
  startTimer: (callback) => {timers.set(++token, callback); return token;},
  stopTimer: (id) => timers.delete(id),
};
vm.runInNewContext(source.slice(start, end) +
  ';globalThis.reveal = revealReady;', context);
let settled = false;
const pending = context.reveal(area).then(value => {
  settled = true; return value;
});
assert.equal(scrolls, 1);
observer();
await Promise.resolve();
assert.equal(settled, false);
// Actual layout/scroll completion supplies new geometry, not an elapsed delay.
rect = {x: 53, y: 64, width: 120, height: 32};
top = area;
listeners.get('scroll')();
const result = await pending;
assert.deepEqual({...result}, rect);
assert.equal(listeners.size, 0);
assert.equal(timers.size, 0);
assert.equal(disconnected, 1);
observer(); // A queued callback after cleanup must be harmless.
// A visible rectangle covered by another element is still a real failure.
top = {};
const covered = context.reveal(area);
observer();
assert.equal(timers.size, 1);
const expire = [...timers.values()][0];
expire();
await assert.rejects(covered, /ready heading not hit-testable at 53 64/);
assert.equal(listeners.size, 0);
assert.equal(timers.size, 0);
assert.equal(disconnected, 2);
// Already-ready targets still wait for the observer readiness signal.
top = area;
const immediate = context.reveal(area);
assert.equal(timers.size, 1);
observer();
await immediate;
assert.equal(timers.size, 0);
assert.equal(listeners.size, 0);

const stepStart = source.indexOf('  smoke.uiSmokeReadyStep =');
const stepEnd = source.lastIndexOf('\n})();');
let blurred = false;
let reported = false;
let complete;
let entered;
const began = new Promise(resolve => {entered = resolve;});
const heading = {textContent: 'Profile'};
const stepContext = {
  smoke: {}, appApi: {strings: {}},
  document: {
    fonts: {ready: Promise.resolve()},
    activeElement: {blur: () => {blurred = true;}},
    getElementById: id => id === 'identity'
      ? {hidden: true} : {textContent: 'peer'},
    querySelector: () => heading,
  },
  peerReflow: () => {
    assert.equal(blurred, true,
      'old validation focus survived viewport reflow');
    return {narrowLines: 2, wideLines: 1};
  },
  revealReady: target => {
    assert.equal(target, heading);
    entered();
    return new Promise(resolve => {complete = resolve;});
  },
  getComputedStyle: () => ({}),
  readyChecks: (_mode, ctx) => {
    assert.equal(ctx.rect, rect);
    return {};
  },
  reportReady: () => {reported = true;},
};
vm.runInNewContext(source.slice(stepStart, stepEnd), stepContext);
const step = stepContext.smoke.uiSmokeReadyStep(
  'file', true, 'profile-restart');
await began;
assert.equal(reported, false);
assert.equal(blurred, true);
complete(rect);
await step;
assert.equal(reported, true);
"""


class ReadyGeometry(unittest.TestCase):
    def test_readiness_and_cleanup_follow_actual_callbacks(self) -> None:
        process = subprocess.run(
            ["node", "--input-type=module"],
            input=RUNNER,
            text=True,
            capture_output=True,
            cwd=ROOT,
            check=False,
            timeout=15,
        )
        self.assertEqual(
            process.returncode, 0, process.stdout + process.stderr
        )
