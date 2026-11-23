import { checkControls, isVisible } from './forward-reachability.js';
import { searchRecoveryFixture } from './search-recovery-fixture.js';

const check = (value, message) => {
  if (!value) throw new Error(message);
};

/** Explicit completions cover presentation admission; these samples cannot authorize real recovery. */
export const checkSearchRecoveryFixture = async () => {
  const fixture = searchRecoveryFixture;
  const { state } = fixture;
  const card = document.getElementById('search-recovery');
  const action = document.getElementById('search-recovery-action');
  const cancel = document.getElementById('search-recovery-cancel');
  const editor = document.getElementById('profile-name');
  const owner = document.getElementById('search-recovery-owner');
  const original = editor.value;
  check(isVisible(card), 'Recovery card is absent');
  check(
    owner.textContent === document.getElementById('peer-id').textContent,
    'Original owner changed',
  );
  check(state.retries === 0 && state.reads === 0, 'Opening recovery performed an action');
  if (state.stage === 'busy') {
    const before = state.confirmations;
    fixture.begin();
    fixture.dismiss();
    check(state.present && state.confirmations === before, 'Pending action repeated or dismissed');
    check(action.disabled && cancel.disabled, 'Pending controls are enabled');
    fixture.complete('error');
  }
  await checkControls([cancel, ...(isVisible(action) ? [action] : [])]);
  if (state.stage === 'sent') {
    const before = state.confirmations;
    fixture.begin();
    check(state.confirmations === before, 'Completed recovery confirmed again');
  } else {
    const before = state.confirmations;
    const needsConfirm = state.stage === 'review';
    action.click();
    check(state.stage === 'busy', 'Action did not enter pending presentation');
    check(state.confirmations === before + Number(needsConfirm), 'Retry/read signed new intent');
    fixture.begin();
    check(state.confirmations === before + Number(needsConfirm), 'Pending confirmation repeated');
    fixture.complete('stale');
    action.click();
    check(
      state.reads > 0 && state.confirmations === before + Number(needsConfirm),
      'Stale review was confirmed',
    );
    fixture.complete('review');
    action.click();
    check(
      state.confirmations === before + Number(needsConfirm) + 1,
      'Fresh explicit review did not confirm once',
    );
    fixture.complete('pending');
    action.click();
    const signed = state.confirmations;
    check(state.retries > 0 && state.stage === 'busy', 'Queued retry did not start');
    fixture.complete('sent');
    check(
      state.confirmations === signed && action.hidden,
      'Delivery completion signed or offered confirmation',
    );
  }
  check(editor.value === original, 'Recovery changed unsaved editor fields');
  cancel.click();
  check(!state.present && !isVisible(card), 'Close did not dismiss presentation');
  const counts = [state.confirmations, state.retries, state.reads].join();
  fixture.retire();
  fixture.begin();
  fixture.complete('review');
  fixture.dismiss();
  check(
    [state.confirmations, state.retries, state.reads].join() === counts,
    'Retired callbacks acted',
  );
  check(
    !state.present && editor.value === original,
    'Retired completion revived or changed content',
  );
};
