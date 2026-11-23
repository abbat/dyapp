import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const source = await readFile(
  new URL('../../clients/desktop/ui/search-recovery-state.js', import.meta.url),
  'utf8',
);
const { searchRecoveryState } = await import(
  `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`
);
const limits = { timeout: 5000 }; // Failure deadline, never transition synchronization.
const pending = () => {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
const saved = { version: 8, hidden: true, searchPending: true };
const create = (call, current = () => true) => searchRecoveryState(call, current, () => {});

test('retirement during state rendering cannot admit a native confirmation', limits, async () => {
  const calls = [];
  let retireOnChange = false;
  const model = searchRecoveryState(
    async (command) => {
      calls.push(command);
      if (command === 'prepare_search_recovery') return 'review';
    },
    () => true,
    () => {
      if (retireOnChange) void model.retire();
    },
  );
  await model.prepare(null);
  retireOnChange = true;
  await model.confirm(model.state.opening);
  assert.deepEqual(calls, ['prepare_search_recovery', 'dismiss_search_recovery']);
  assert.equal(model.state.saved, null);
});

test(
  'only explicit confirmation can commit; busy and unconfirmed retries cannot admit',
  limits,
  async () => {
    const calls = [];
    const completion = pending();
    const model = create(async (command, args) => {
      calls.push([command, args]);
      if (command === 'prepare_search_recovery') return 'original';
      if (command === 'confirm_search_recovery') return completion.promise;
    });
    await model.prepare(null);
    const opening = model.state.opening;
    await model.retry(opening);
    assert.equal(calls.length, 1);
    const confirmed = model.confirm(opening);
    assert.equal(model.state.busy, true);
    await model.confirm(opening);
    await model.prepare(opening);
    assert.equal(calls.length, 2);
    completion.resolve(saved);
    await confirmed;
    assert.equal(model.state.saved, saved);
    await model.confirm(opening);
    assert.equal(calls.length, 2);
    await model.retire();
    assert.deepEqual(calls.at(-1), ['dismiss_search_recovery', { token: 'original' }]);
  },
);

test('late prepare releases only its own token after retirement', limits, async () => {
  const entered = pending();
  const response = pending();
  const dismissed = [];
  const old = create(async (command, args) => {
    if (command === 'prepare_search_recovery') {
      entered.resolve();
      return response.promise;
    }
    dismissed.push(args.token);
  });
  const original = old.prepare(null);
  await entered.promise;
  await old.retire();
  const fresh = create(async (command) => {
    assert.equal(command, 'prepare_search_recovery');
    return 'successor';
  });
  await fresh.prepare(null);
  response.resolve('obsolete');
  await original;
  assert.deepEqual(dismissed, ['obsolete']);
  assert.equal(fresh.state.saved, null);
  assert.equal(fresh.state.failure, null);
  assert.equal(fresh.state.busy, false);
});

test(
  'late confirmation and captured callbacks cannot mutate an equal-coordinate successor',
  limits,
  async () => {
    const response = pending();
    const old = create(async (command) =>
      command === 'prepare_search_recovery' ? 'old' : response.promise,
    );
    await old.prepare(null);
    const captured = old.state.opening;
    const original = old.confirm(captured);
    const retired = old.retire();
    const fresh = create(async (command) => {
      assert.equal(command, 'prepare_search_recovery');
      return 'new';
    });
    await fresh.prepare(null);
    response.resolve(saved);
    await Promise.all([original, retired]);
    await old.confirm(captured);
    await old.retry(captured);
    await old.prepare(captured);
    assert.equal(old.state.saved, null);
    assert.equal(fresh.state.saved, null);
    assert.equal(fresh.state.busy, false);
  },
);

test('delivery failure retains the exact receipt; retry never confirms again', limits, async () => {
  let confirmed = 0;
  let retried = 0;
  const model = create(async (command) => {
    if (command === 'prepare_search_recovery') return 'review';
    if (command === 'confirm_search_recovery') {
      ++confirmed;
      return saved;
    }
    assert.equal(command, 'retry_search_recovery');
    if (++retried === 1) throw { kind: 'failed', reason: 'changed is only a diagnostic word' };
    return { ...saved, searchPending: false };
  });
  await model.prepare(null);
  const opening = model.state.opening;
  await model.confirm(opening);
  await model.retry(opening);
  assert.equal(model.state.saved, saved);
  assert.equal(model.state.failure, 'failed');
  await model.retry(opening);
  await model.retry(opening);
  assert.equal(confirmed, 1);
  assert.equal(retried, 2);
  assert.equal(model.state.saved.searchPending, false);
});

test(
  'typed review refusals require fresh preparation and invalidate captured actions',
  limits,
  async () => {
    for (const kind of [
      'closed',
      'changed',
      'reviewRequired',
      'linkPending',
      'versionExhausted',
      'wrongOwner',
    ]) {
      const calls = [];
      const model = create(async (command) => {
        calls.push(command);
        if (command === 'prepare_search_recovery') return 'review';
        if (command === 'confirm_search_recovery') return saved;
        if (command === 'retry_search_recovery')
          throw { kind, reason: 'ordinary transport failure' };
      });
      await model.prepare(null);
      const opening = model.state.opening;
      await model.confirm(opening);
      await model.retry(opening);
      assert.equal(model.state.failure, kind);
      await model.confirm(opening);
      await model.retry(opening);
      assert.equal(calls.length, 3);
      await model.prepare(opening);
      const count = calls.length;
      await model.confirm(opening);
      await model.retry(opening);
      await model.prepare(opening);
      assert.equal(calls.length, count);
      assert.equal(model.state.saved, null);
    }
  },
);

test('uncertain confirmation cannot resend or resign without fresh review', limits, async () => {
  let confirmed = 0;
  const model = create(async (command) => {
    if (command === 'prepare_search_recovery') return 'review';
    assert.equal(command, 'confirm_search_recovery');
    ++confirmed;
    throw new Error('changed closed wrongOwner are untrusted diagnostic words');
  });
  await model.prepare(null);
  const opening = model.state.opening;
  await model.confirm(opening);
  assert.equal(model.state.failure, 'failed');
  await model.confirm(opening);
  await model.retry(opening);
  assert.equal(confirmed, 1);
  assert.equal(model.state.saved, null);
});
