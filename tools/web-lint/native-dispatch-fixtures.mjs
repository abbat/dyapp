import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(
  new URL('../../clients/desktop/ui-tests/main.js', import.meta.url),
  'utf8',
);
const start = source.indexOf('const interClientLinkStep =');
assert.ok(start >= 0, 'native smoke dispatch entry is missing');
const entry = source.slice(start);
const run = async (step) => {
  const calls = [];
  const invokeTest = async (command) => {
    if (command === 'ui_test_result') calls.push('result');
    return { ui_test_mode: 'file', ui_test_existing: false, ui_test_step: step }[command];
  };
  const smoke = {
    fail: (error) => {
      throw error;
    },
    uiSmokeGuards: {
      linkStep: async () => {
        calls.push(`legacy:${step}`);
        return 'peer';
      },
    },
    calibrateAccessibility: async () => {
      calls.push('calibrate');
    },
    uiSmokeReadyStep: async () => {
      calls.push('ready');
    },
  };
  await vm.runInNewContext(entry, {
    appApi: { identityShown: Promise.resolve({ status: 'setup' }) },
    invokeTest,
    smoke,
    checkModuleStartup: async () => {
      calls.push('module');
    },
    readySmokeSteps: () => ({}),
    runSmokeIdentityStep: async () => {
      calls.push('identity');
      return false;
    },
    runSmokeNetworkStep: async () => {
      calls.push('network');
      return false;
    },
    runSmokeFallbackStep: async () => {
      if (
        !['link-review:', 'publication:', 'received-views:'].some((prefix) =>
          step.startsWith(prefix),
        )
      )
        throw new Error(`Unknown UI smoke step: ${step}`);
      calls.push(`capture:${step}`);
    },
  });
  return calls;
};
const limits = { timeout: 5000 };

test(
  'only the four actual inter-client link steps bypass ordinary initialization',
  limits,
  async () => {
    for (const step of ['link-send:code', 'link-wait:seed', 'link-disconnect', 'link-wiped'])
      assert.deepEqual(await run(step), ['module', `legacy:${step}`, 'result']);
  },
);

test(
  'all focused capture families follow ordinary initialized fallback dispatch',
  limits,
  async () => {
    for (const family of ['link-review', 'publication', 'received-views']) {
      for (const theme of ['light', 'dark']) {
        for (const scale of ['100', '200']) {
          const step = `${family}:${theme}:${scale}`;
          assert.deepEqual(await run(step), [
            'module',
            'calibrate',
            'identity',
            'network',
            `capture:${step}`,
            'ready',
          ]);
        }
      }
    }
  },
);

test(
  'similar unknown link names are refused rather than sent to another client',
  limits,
  async () => {
    for (const step of [
      'link-review',
      'link-reviewed:x',
      'link-wait',
      'link-sendx',
      'link-disconnect:more',
    ])
      await assert.rejects(run(step), /Unknown UI smoke step/);
  },
);
