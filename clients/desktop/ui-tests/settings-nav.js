(() => {
  const checkPreferenceSwitches = (check) => {
    for (const id of [
      'send-views',
      'send-read-receipts',
      'send-typing',
      'message-notifications',
      'tray-show',
      'tray-close',
    ]) {
      const input = document.getElementById(id);
      check(
        input.getAttribute('role') === 'switch' && input.classList.contains('switch'),
        'Immediate preference lacks switch semantics',
      );
    }
  };
  window.checkSettingsNavigation = async () => {
    await openArea('settings');
    const root = document.getElementById('settings-root');
    const back = document.getElementById('settings-back');
    const panes = [...document.querySelectorAll('[data-settings-pane]')];
    const check = (ok, reason) => {
      if (!ok) throw Error(reason);
    };
    check(
      !root.hidden && back.hidden && root.children.length === 7,
      'Settings root is not bounded',
    );
    for (const button of root.querySelectorAll('button')) {
      button.click();
      check(root.hidden && !back.hidden, 'Category navigation failed');
      check(
        panes.filter((pane) => !pane.hidden).length === 1,
        'Multiple settings categories visible',
      );
      back.click();
      check(!root.hidden && document.activeElement === button, 'Settings Back lost focus');
    }
    for (const [parent, child] of [
      ['account', 'identities'],
      ['account', 'devices'],
      ['privacy', 'blocked'],
      ['privacy', 'passed'],
    ]) {
      root.querySelector(`[data-settings-open="${parent}"]`).click();
      document
        .querySelector(`[data-settings-pane="${parent}"] [data-settings-open="${child}"]`)
        .click();
      check(
        !document.querySelector(`[data-settings-pane="${child}"]`).hidden,
        'Collection is not separate',
      );
      back.click();
      check(
        !document.querySelector(`[data-settings-pane="${parent}"]`).hidden,
        'Collection Back lost parent',
      );
      back.click();
    }
    window.openSettingsPane('privacy');
    document.dispatchEvent(new Event('dyapp:identity-change'));
    check(
      !root.hidden && panes.every((pane) => pane.hidden),
      'Identity change retained a category',
    );
    checkPreferenceSwitches(check);
  };
})();
