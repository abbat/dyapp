import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const source = await readFile(
  new URL('../../clients/desktop/ui/received-view-state.js', import.meta.url),
  'utf8',
);
const { receivedViewState } = await import(
  `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`
);
const pending = () => {
  let resolve;
  let reject;
  const promise = new Promise((done, failed) => {
    resolve = done;
    reject = failed;
  });
  return { promise, resolve, reject };
};
const limits = { timeout: 5000 }; // Failure deadlines; no transition depends on elapsed time.
const page = (target) => ({
  total: 71,
  matched: 71,
  number: 2,
  pages: 3,
  entries: [{ target, label: '2024-02-29' }],
  previous: { query: '', offset: 0 },
  next: { query: '', offset: 64 },
});

test(
  'core pages and cursors remain unchanged; pending refresh disables admission',
  limits,
  async () => {
    const requests = [];
    const responses = [pending(), pending()];
    const view = receivedViewState(
      (query, cursor) => {
        requests.push({ query, cursor });
        return responses[requests.length - 1].promise;
      },
      () => true,
      () => {},
    );
    const loading = view.load();
    assert.equal(view.state.busy, true);
    assert.equal(view.admits(null), false);
    const first = page('original-peer');
    responses[0].resolve(first);
    await loading;
    assert.equal(view.state.page, first);
    assert.equal(view.admits(first), true);
    const next = view.load('', first.next);
    assert.equal(view.admits(first), false);
    assert.equal(requests[1].cursor, first.next);
    const second = page('next-peer');
    responses[1].resolve(second);
    await next;
    assert.equal(view.state.page, second);
    assert.equal(view.admits(first), false);
    assert.equal(view.admits(second), true);
  },
);

test(
  'failed refresh preserves rows; changed-query failure cannot relabel old results',
  limits,
  async () => {
    const saved = page('saved-peer');
    let failing = false;
    const view = receivedViewState(
      async () => {
        if (failing) throw new Error('controlled read failure');
        return saved;
      },
      () => true,
      () => {},
    );
    await view.load();
    failing = true;
    await view.load();
    assert.equal(view.state.page, saved);
    assert.equal(view.state.failed, true);
    assert.equal(view.admits(saved), true);
    await view.load('other-query', null);
    assert.equal(view.state.page, null);
    assert.equal(view.state.failed, true);
    assert.equal(view.admits(saved), false);
  },
);

test(
  'cancelled and superseded completions cannot replace the current opening',
  limits,
  async () => {
    const responses = [pending(), pending(), pending()];
    let calls = 0;
    let current = true;
    const view = receivedViewState(
      () => responses[calls++].promise,
      () => current,
      () => {},
    );
    const old = view.load('old', null);
    view.cancel();
    const fresh = view.load('new', null);
    const originalPage = page('new-peer');
    responses[1].resolve(originalPage);
    await fresh;
    responses[0].resolve(page('old-peer'));
    await old;
    assert.equal(view.state.page, originalPage);
    const hidden = view.load();
    current = false;
    view.cancel();
    responses[2].reject(new Error('retired failure'));
    await hidden;
    assert.equal(view.state.page, originalPage);
    assert.equal(view.state.failed, false);
    assert.equal(view.admits(originalPage), false);
  },
);
