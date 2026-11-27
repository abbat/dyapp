import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(
  new URL('../../clients/desktop/ui-tests/common.js', import.meta.url),
  'utf8',
);
const contrastSource = await readFile(
  new URL('../../clients/desktop/ui-tests/criteria-axe.js', import.meta.url),
  'utf8',
);
test('contrast placement uses current nested content space and retains oversized text behavior', () => {
  const context = {
    smoke: {},
    window: { innerHeight: 600 },
    getComputedStyle: (element) => element.style,
  };
  vm.runInNewContext(
    contrastSource + '\nglobalThis.alignContrast = contrastScrollAlignment;',
    context,
  );
  const outer = { clientHeight: 500, style: { paddingTop: '20px', paddingBottom: '30px' } };
  const inner = { clientHeight: 300, style: { paddingTop: '40px', paddingBottom: '50px' } };
  const target = (height) => ({ getBoundingClientRect: () => ({ height }) });
  const ancestors = [
    [outer, 0, 0],
    [inner, 0, 0],
  ];
  assert.equal(context.alignContrast(target(200), ancestors), 'center');
  assert.equal(context.alignContrast(target(220), ancestors), 'start');
  inner.clientHeight = 400;
  assert.equal(context.alignContrast(target(220), ancestors), 'center');
  context.window.innerHeight = 180;
  assert.equal(context.alignContrast(target(200), ancestors), 'start');
});
const pending = () => {
  let resolve;
  let reject;
  const promise = new Promise((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
};
const makeWindow = (listeners, invoke) => ({
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
  __TAURI__: { core: { invoke } },
});
const loadWindow = (smoke, window, timers) =>
  vm.runInNewContext(source, {
    smoke,
    window,
    setTimeout: (callback) => {
      timers.add(callback);
      return callback;
    },
    clearTimeout: (callback) => timers.delete(callback),
  });
const fixture = () => {
  const listeners = new Set();
  const timers = new Set();
  const native = pending();
  const chrome = { width: 0, height: 0 };
  let started = pending();
  let snapshots = 0;
  let snapshotHook;
  native.calls = 0;
  const window = makeWindow(listeners, (command, { set }) => {
    assert.equal(command, 'ui_test_window');
    assert.equal(listeners.size, 1, 'listener must precede native invocation');
    if (set === null) {
      snapshots += 1;
      const bounds = {
        width: window.innerWidth * window.devicePixelRatio + chrome.width,
        height: window.innerHeight * window.devicePixelRatio + chrome.height,
      };
      return snapshotHook?.(bounds) ?? Promise.resolve([bounds]);
    }
    assert.equal(set.width, 440);
    native.calls += 1;
    started.resolve();
    return native.promise;
  });
  const smoke = {};
  loadWindow(smoke, window, timers);
  return {
    window,
    native,
    timers,
    chrome,
    get started() {
      return started.promise;
    },
    get snapshots() {
      return snapshots;
    },
    onSnapshot: (hook) => {
      snapshotHook = hook;
    },
    resize: (height = 1400) => {
      started = pending();
      return smoke.resizeWindow({ width: 440, height, x: 0, y: 0 });
    },
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

test('native/WebView insets come from current geometry at each display scale', limits, async () => {
  for (const scale of [1, 1.25, 2]) {
    const run = fixture();
    run.window.devicePixelRatio = scale;
    run.chrome.width = 12 * scale;
    run.chrome.height = 28 * scale;
    let completed = false;
    const resized = run.resize().then(() => {
      completed = true;
    });
    await run.started;
    run.native.resolve();
    await run.native.promise;
    run.window.innerWidth = Math.round(440 / scale);
    run.window.innerHeight = Math.round(1400 / scale);
    run.emit();
    assert.equal(completed, false, 'window content size is not the WebView viewport');
    run.window.innerWidth = Math.round((440 - run.chrome.width) / scale);
    run.window.innerHeight = Math.round((1400 - run.chrome.height) / scale);
    run.emit();
    await resized;
    run.clean();
  }
});

test(
  'a viewport change while reading native bounds requires a fresh snapshot',
  limits,
  async () => {
    const run = fixture();
    run.onSnapshot(() => {
      if (run.snapshots === 1) {
        run.window.innerHeight = 600;
        run.emit();
      }
    });
    const resized = run.resize();
    await run.started;
    assert.equal(run.snapshots, 2);
    run.native.resolve();
    run.window.innerWidth = 440;
    run.window.innerHeight = 1400;
    run.emit();
    await resized;
    run.clean();
  },
);

test(
  'a native baseline arriving after the deadline cannot dispatch a late resize',
  limits,
  async () => {
    const run = fixture();
    const held = pending();
    run.onSnapshot(() => held.promise);
    const expired = assert.rejects(run.resize(), /viewport geometry did not finish/);
    for (const fire of run.timers) fire();
    await expired;
    held.resolve([{ width: 1024, height: 720 }]);
    // This completion callback runs after the settled native promise's microtasks, without a delay.
    await new Promise(setImmediate);
    assert.equal(run.native.calls, 0);
    run.clean();
  },
);

test(
  'native acknowledgement cannot admit stale or partially resized viewport',
  limits,
  async () => {
    const run = fixture();
    let completed = false;
    const resized = run.resize().then(() => {
      completed = true;
    });
    const geometryDeadline = [...run.timers][0];
    await run.started;
    assert.equal(run.timers.has(geometryDeadline), false, 'resize needs a fresh phase deadline');
    assert.equal(run.timers.size, 1);
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
    await run.started;
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
    await run.started;
    run.native.resolve();
    await first;
    run.clean();
    const second = run.resize(720);
    await run.started;
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
  await refused.started;
  refused.native.reject(new Error('controlled refusal'));
  await failure;
  refused.clean();
  const stalled = fixture();
  const expired = assert.rejects(stalled.resize(), /viewport 1024x720 at DPR 1/);
  await stalled.started;
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
