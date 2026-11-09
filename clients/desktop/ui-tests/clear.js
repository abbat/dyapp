// The actual Clear controller must identify its target before any destructive action.
(() => {
  const checkClearTarget = (peer, name, get, check) => {
    check(
      get('name').textContent === name && get('name').children.length === 0,
      'Clear target name is missing or markup',
    );
    check(
      get('peer').textContent === peer && get('peer').querySelector('wbr'),
      'Clear target ID changed raw copy or lacks chunks',
    );
    check(
      get('peer-label').textContent === appApi.strings['view.peer'],
      'Clear labels the target as our own ID',
    );
    check(
      get('peer').closest('dl')?.querySelector('dt') === get('peer-label'),
      'Clear ID lacks its semantic label',
    );
    const warning = get('dialog').querySelector('[data-i18n="chats.clearWarning"]');
    check(
      warning.textContent === appApi.strings['chats.clearWarning'],
      'Clear warning is not localized',
    );
    check(
      get('name').compareDocumentPosition(get('peer')) & Node.DOCUMENT_POSITION_FOLLOWING,
      'Clear ID precedes target name',
    );
    check(
      get('peer').compareDocumentPosition(warning) & Node.DOCUMENT_POSITION_FOLLOWING,
      'Clear consequence precedes target ID',
    );
  };
  smoke.checkClearDialog = async () => {
    const peer = `12D3KooW${'a'.repeat(44)}`;
    const name = '<target> & contact 😺';
    const get = (id) => document.getElementById(`chat-clear-${id}`);
    const check = (ok, reason) => {
      if (!ok) throw new Error(reason);
    };
    const call = async (command) => {
      if (command === 'contact_names') return [{ peerId: peer, name, verified: null }];
      if (command === 'chat_history')
        return { state: 'open', messages: [], muted: false, activeRecently: false };
      if (command === 'chat_rows') return [];
      return false;
    };
    try {
      await openArea('chats-area');
      await appBindings.openChat(peer, call);
      const opener = document.getElementById('chat-clear');
      opener.focus();
      opener.click();
      await new Promise(requestAnimationFrame);
      check(
        get('dialog').open && document.activeElement === get('cancel'),
        'Clear lost initial Cancel focus',
      );
      checkClearTarget(peer, name, get, check);
      const closed = new Promise((resolve) =>
        get('dialog').addEventListener('close', resolve, { once: true }),
      );
      get('cancel').click();
      await closed;
      check(
        document.activeElement === opener && !get('dialog').open,
        'Clear Cancel lost focus return',
      );
      check(
        !get('name').textContent &&
          !get('peer').textContent &&
          !get('name').dataset.peerId &&
          !get('name').hasAttribute('title'),
        'Closed Clear retains target identity',
      );
    } finally {
      appBindings.closeClear();
      appBindings.closeChat();
      appBindings.peerNames.delete(peer);
    }
  };
})();
