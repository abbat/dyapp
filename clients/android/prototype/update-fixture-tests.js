import { checkControls, isVisible } from './forward-reachability.js';
import { updateFixture } from './update-fixture.js';

const check = (value, message) => {
  if (!value) throw new Error(message);
};

/** Released completion signals, never elapsed time, drive this non-shipping notice sample. */
export const checkUpdateFixture = async () => {
  const screen = document.documentElement.dataset.screen;
  const fixture = updateFixture;
  const { state } = fixture;
  const dialog = document.getElementById('update-notice');
  const open = document.getElementById('update-notice-open');
  const later = document.getElementById('update-notice-later');
  const badge = document.getElementById('update-available-badge');
  const status = document.getElementById('update-notice-status');
  check(isVisible(dialog), 'Update notice was not mounted');
  check(
    dialog.querySelectorAll('[data-default]').length === 1 && open.hasAttribute('data-default'),
    'Update notice must have exactly one default Open action',
  );
  check(
    isVisible(badge) && badge.getAttribute('aria-label').includes('0.0.2'),
    'Availability badge missing',
  );
  check(
    document.getElementById('update-notice-offer').textContent.includes('0.0.2'),
    'Wrong offer version',
  );
  const saving = screen === 'update-notice-saving';
  check(open.disabled === saving && later.disabled === saving, 'Wrong pending action admission');
  check(
    isVisible(status) === !['update-notice-package', 'update-notice-source'].includes(screen),
    'Wrong notice status',
  );
  await checkControls(saving ? [] : [open, later]);
  if (saving) {
    fixture.launch();
    fixture.dismiss();
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    check(
      state.present && state.dismissals === 0 && state.opened.length === 0,
      'Pending callbacks escaped admission',
    );
    fixture.completeLater(false);
    check(
      state.present && !later.disabled && isVisible(status),
      'Failed save lost retryable notice',
    );
  }
  const before = state.dismissals;
  dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  if (screen === 'update-notice-open-error') {
    check(state.opened.length === 0 && isVisible(status), 'Browser failure was hidden');
    state.openFailed = false;
    open.click();
  }
  check(state.opened.length === 1, 'Open must hand off once');
  check(
    state.opened[0] === 'https://github.com/abbat/dyapp/releases/tag/v0.0.2',
    'Open changed the core-shaped destination',
  );
  fixture.launch();
  check(
    state.opened.length === 1 && state.present && state.dismissals === before,
    'Open repeated or dismissed the notice',
  );
  later.click();
  check(
    state.pending && state.present && open.disabled && later.disabled,
    'Later closed before a committed result',
  );
  fixture.completeLater(false);
  check(
    state.present && !later.disabled && isVisible(status),
    'Failed acknowledgement consumed the notice',
  );
  dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  check(state.pending, 'Back did not request guarded acknowledgement');
  fixture.completeLater(true);
  check(
    !state.present && !isVisible(dialog) && isVisible(badge),
    'Later removed the Settings offer or kept interruption',
  );
  check(
    document.activeElement.id === 'nav-settings',
    'Acknowledgement lost a reachable focus target',
  );
  const closed = state.dismissals;
  fixture.launch();
  fixture.dismiss();
  fixture.completeLater(true);
  check(
    state.dismissals === closed && state.opened.length === 1,
    'Retired notice callbacks acted again',
  );
};
