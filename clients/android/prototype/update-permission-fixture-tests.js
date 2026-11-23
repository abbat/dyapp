import { checkControls, isVisible } from './forward-reachability.js';
import { updatePermissionFixture } from './update-permission-fixture.js';

const check = (value, message) => {
  if (!value) throw new Error(message);
};

/** Explicit platform-result release; elapsed time never authorizes a transition. */
export const checkUpdatePermissionFixture = async () => {
  const fixture = updatePermissionFixture;
  const { state } = fixture;
  const action = document.getElementById('update-permission-action');
  check(
    isVisible(document.getElementById('update-permission-status')),
    'Permission status is missing',
  );
  check(
    !isVisible(document.getElementById('update-notice-scrim')),
    'Permission row opened a dialog',
  );
  check(action.disabled === state.pending, 'Pending permission admission is wrong');
  check(
    document.getElementById('update-check').disabled,
    'Permission flow activated feed checking',
  );
  await checkControls(state.pending ? [] : [action]);
  if (state.pending) {
    fixture.launch();
    check(state.attempts === 0, 'Pending action launched again');
    fixture.setReadback(false);
    fixture.complete();
  }
  if (state.allowed == null) {
    fixture.launch();
    check(state.allowed == null && state.attempts === 0, 'Failed read fabricated permission');
    fixture.setReadback(false);
    fixture.launch();
    check(state.allowed === false && !state.pending, 'Read retry launched a prompt');
  }
  if (state.failed) {
    fixture.launch();
    check(
      !state.pending && isVisible(document.getElementById('update-permission-error')),
      'Failed platform launch lost retryable feedback',
    );
  }
  fixture.allowLaunch();
  const attempts = state.attempts;
  fixture.launch();
  fixture.launch();
  check(state.pending && state.attempts === attempts + 1, 'Duplicate launch escaped admission');
  fixture.setReadback(false);
  fixture.complete();
  check(!state.pending && state.allowed === false, 'Denial fabricated permission');
  fixture.launch();
  fixture.setReadback(true);
  fixture.complete();
  check(state.allowed === true && !state.pending, 'Result did not read current permission');
  check(
    document.getElementById('update-state').textContent.includes('0.0.2'),
    'Permission dismissed offer',
  );
  fixture.launch();
  fixture.retire();
  const before = JSON.stringify(state);
  fixture.setReadback(false);
  fixture.complete();
  fixture.launch();
  check(JSON.stringify(state) === before, 'Retired result changed permission state');
};
