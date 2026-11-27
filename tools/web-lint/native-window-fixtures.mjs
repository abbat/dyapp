import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(
  new URL('../../clients/desktop/ui-tests/common.js', import.meta.url),
  'utf8',
);
const pending = () => {
  let resolve;
  let reject;
  const promise = new Promise((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
};
const fixture = () => {
  const listeners = new Set();
  const timers = new Set();
  const native = pending();
  const window = {
    innerWidth: 1024,
    innerHeight: 720,
    devicePixelRatio: 1,
    addEventListener: (name, callback) => {
      assert.equal(name, 'resize');
      listeners.add(callback);
    },
    removeEventListener: (name, callback) => {
      assert.equal(name, 'resize');
      assert.ok(listeners.delete(callback));
    },
    __TAURI__: {
      core: {
        invoke: (command, { set }) => {
          assert.equal(command, 'ui_test_window');
          assert.equal(listeners.size, 1, 'listener must precede native invocation');
          assert.equal(set.width, 440);
          return native.promise;
        },
      },
    },
  };
  const smoke = {};
  vm.runInNewContext(source, {
    smoke,
    window,
    setTimeout: (callback) => {
      timers.add(callback);
      return callback;
    },
    clearTimeout: (callback) => timers.delete(callback),
  });
  return {
    window,
    native,
    timers,
    resize: (height = 1400) => smoke.resizeWindow({ width: 440, height, x: 0, y: 0 }),
    emit: () => {
      for (const callback of listeners) callback();
    },
    clean: () => {
      assert.equal(listeners.size, 0);
      assert.equal(timers.size, 0);
    },
  };
};
const limits = { timeout: 5000 };

test(
  'native acknowledgement cannot admit stale or partially resized viewport',
  limits,
  async () => {
    const run = fixture();
    let completed = false;
    const resized = run.resize().then(() => {
      completed = true;
    });
    run.native.resolve();
    await run.native.promise;
    assert.equal(completed, false);
    run.window.innerWidth = 440;
    run.emit();
    assert.equal(completed, false);
    run.window.innerHeight = 1400;
    run.emit();
    await resized;
    run.clean();
  },
);

test(
  'resize before IPC completion is rechecked against current geometry and DPR',
  limits,
  async () => {
    const run = fixture();
    run.window.devicePixelRatio = 2;
    const resized = run.resize();
    run.window.innerWidth = 220;
    run.window.innerHeight = 700;
    run.emit();
    // Retire that early matching frame before the OS operation acknowledges completion.
    run.window.innerWidth = 1024;
    run.native.resolve();
    await run.native.promise;
    assert.equal(run.timers.size, 1, 'stale early frame must not settle the transition');
    run.window.innerWidth = 220;
    run.emit();
    await resized;
    run.clean();
  },
);

test(
  'already matching viewport needs no new resize event; each call starts fresh',
  limits,
  async () => {
    const run = fixture();
    run.window.innerWidth = 440;
    run.window.innerHeight = 1400;
    const first = run.resize();
    run.native.resolve();
    await first;
    run.clean();
    const second = run.resize(720);
    await run.native.promise;
    assert.equal(run.timers.size, 1);
    run.window.innerHeight = 720;
    run.emit();
    await second;
    run.clean();
  },
);

test('native refusal and explicit deadline both reject and detach listeners', limits, async () => {
  const refused = fixture();
  const failure = assert.rejects(refused.resize(), /controlled refusal/);
  refused.native.reject(new Error('controlled refusal'));
  await failure;
  refused.clean();
  const stalled = fixture();
  const expired = assert.rejects(stalled.resize(), /viewport 1024x720 at DPR 1/);
  assert.equal(stalled.timers.size, 1);
  for (const fire of stalled.timers) fire();
  await expired;
  stalled.clean();
  // A late native acknowledgement cannot revive the retired transition or its listener.
  stalled.native.resolve();
  await stalled.native.promise;
  stalled.emit();
  stalled.clean();
});
