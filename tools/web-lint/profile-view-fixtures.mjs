import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

// Load the exact dependency-free browser module, independently of Node's .js package mode.
const source = await readFile(
  new URL('../../clients/desktop/ui/profile-views.js', import.meta.url),
  'utf8',
);
const { profileViewOpening } = await import(
  `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`
);
const pending = () => {
  let complete;
  const promise = new Promise((resolve) => {
    complete = resolve;
  });
  return { promise, complete };
};
const limits = { timeout: 5000 }; // Failure deadline only; all transitions use explicit completions.

test('a current explicit opening submits its original opaque handle once', limits, async () => {
  const calls = [];
  const opening = profileViewOpening(
    'original-peer',
    async (command, args) => {
      calls.push([command, args]);
      return command === 'prepare_profile_view' ? 'original-token' : null;
    },
    () => true,
  );
  await opening.done;
  assert.deepEqual(calls, [
    ['prepare_profile_view', { peerId: 'original-peer' }],
    ['submit_profile_view', { token: 'original-token' }],
  ]);
  await opening.retire();
  await opening.retire();
  assert.equal(calls.length, 3);
  assert.deepEqual(calls[2], ['retire_profile_view', { token: 'original-token' }]);
});

test('a preparation finishing after disposal only retires its own handle', limits, async () => {
  const prepared = pending();
  const calls = [];
  const opening = profileViewOpening(
    'old-peer',
    async (command, args) => {
      calls.push([command, args]);
      return command === 'prepare_profile_view' ? prepared.promise : null;
    },
    () => true,
  );
  await opening.retire();
  prepared.complete('old-token');
  await opening.done;
  assert.deepEqual(calls, [
    ['prepare_profile_view', { peerId: 'old-peer' }],
    ['retire_profile_view', { token: 'old-token' }],
  ]);
});

test('late work cannot submit or retire the replacement opening', limits, async () => {
  const prepared = pending();
  const calls = [];
  let selected = 'old';
  const old = profileViewOpening(
    'old-peer',
    async (command, args) => {
      calls.push([command, args]);
      return command === 'prepare_profile_view' ? prepared.promise : null;
    },
    () => selected === 'old',
  );
  selected = 'new';
  const fresh = profileViewOpening(
    'new-peer',
    async (command, args) => {
      calls.push([command, args]);
      return command === 'prepare_profile_view' ? 'new-token' : { status: 'queued' };
    },
    () => selected === 'new',
  );
  await fresh.done;
  prepared.complete('old-token');
  await old.done;
  assert.deepEqual(calls, [
    ['prepare_profile_view', { peerId: 'old-peer' }],
    ['prepare_profile_view', { peerId: 'new-peer' }],
    ['submit_profile_view', { token: 'new-token' }],
    ['retire_profile_view', { token: 'old-token' }],
  ]);
});

test('an admitted original result never triggers another submission', limits, async () => {
  const submitted = pending();
  const entered = pending();
  const calls = [];
  const opening = profileViewOpening(
    'peer',
    async (command, args) => {
      calls.push([command, args]);
      if (command === 'prepare_profile_view') return 'token';
      if (command === 'submit_profile_view') {
        entered.complete();
        return submitted.promise;
      }
      return null;
    },
    () => true,
  );
  await entered.promise;
  await opening.retire();
  submitted.complete({ status: 'queued', id: [1], detail: 'offline' });
  await opening.done;
  assert.deepEqual(
    calls.map(([command]) => command),
    ['prepare_profile_view', 'submit_profile_view', 'retire_profile_view'],
  );
});

test('hidden pages and failed preparation never schedule a disclosure', limits, async () => {
  let calls = 0;
  const hidden = profileViewOpening(
    'peer',
    async () => {
      calls += 1;
    },
    () => false,
  );
  await hidden.done;
  assert.equal(calls, 0);
  const failed = profileViewOpening(
    'peer',
    async () => {
      calls += 1;
      throw new Error('refused');
    },
    () => true,
  );
  await failed.done;
  assert.equal(calls, 1);
});

test('a precommit refusal is not automatically resubmitted', limits, async () => {
  const calls = [];
  const opening = profileViewOpening(
    'peer',
    async (command) => {
      calls.push(command);
      if (command === 'prepare_profile_view') return 'token';
      throw new Error('refused');
    },
    () => true,
  );
  await opening.done;
  assert.deepEqual(calls, ['prepare_profile_view', 'submit_profile_view']);
});
