// Scoped fixture/assertion helpers and shared smoke scenarios.
{
  const assertChatEditEditDeleteVisibleOutsideOwnLiveText = (editButtons) => {
    if (
      editButtons.length !== 4 ||
      editButtons[0].textContent !== window.strings['chats.edit'] ||
      editButtons[1].textContent !== window.strings['chats.delete'] ||
      editButtons[2].textContent !== window.strings['chats.forward'] ||
      editButtons[3].textContent !== window.strings['chats.forward']
    ) {
      throw new Error('Edit/Delete visible outside own live text or Forward missing');
    }
  };
  const assertChatEditEditDidNotRefreshCurrentTextAnd = (editedArgs, peer, groupId, own) => {
    if (
      !editedArgs ||
      editedArgs.peerId !== peer ||
      editedArgs.text !== 'replacement' ||
      JSON.stringify(editedArgs.groupId) !== JSON.stringify(groupId) ||
      JSON.stringify(editedArgs.messageId) !== JSON.stringify(own.id) ||
      editDialog.open ||
      !chatMessages.textContent.includes(window.strings['chats.edited']) ||
      chatComposeStatus.textContent !== window.strings['chats.editSent']
    ) {
      throw new Error('Edit did not refresh current text and marker');
    }
  };
  const assertChatEditUnstagedEditLostItsDraft = () => {
    if (
      !editDialog.open ||
      editInput.value !== 'keep draft' ||
      !editError.textContent.includes('diskFailed')
    ) {
      throw new Error('unstaged Edit lost its draft');
    }
  };
  const assertChatEditStagedEditOfferedADuplicateRetry = () => {
    if (
      editDialog.open ||
      !chatMessages.textContent.includes('queued text') ||
      chatComposeStatus.textContent !== window.strings['chats.editQueued']
    ) {
      throw new Error('staged Edit offered a duplicate retry');
    }
  };
  const chatEditFixture = () => {
    const fixture = {};
    const peer = 'f'.repeat(64);
    const groupId = [8, 9];
    const own = {
      author: current.peerId,
      groupId,
      id: [1],
      text: 'original',
      deleted: false,
      edited: false,
      sentAtMs: null,
    };
    const theirs = { ...own, author: peer, id: [2], text: 'theirs' };
    const deleted = { ...own, id: [3], text: null, deleted: true };
    fixture.messages = [own, theirs, deleted];
    const row = { peerId: peer, state: 'open', lastMessage: own, unreadCount: 0 };
    const call = (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: fixture.messages });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([row]);
      throw new Error(`unexpected Edit command: ${command}`);
    };
    return Object.assign(fixture, { peer, groupId, own, theirs, deleted, row, call });
  };
  const checkChatEditStale = async (call) => {
    chatMessages.querySelector('button').click();
    editInput.value = 'late';
    editInput.dispatchEvent(new Event('input'));
    const finishEditCall = pendingSmokeCall();
    const oldEdit = editChatText((command) =>
      command === 'edit_chat_text' ? finishEditCall.call() : call(command),
    );
    const readyState = current;
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    finishEditCall.resolve([6]);
    await oldEdit;
    if (editDialog.open || !chatDetail.hidden) throw new Error('old Edit painted new identity');
  };
  const checkChatEdit = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const fixture = chatEditFixture();
    const { peer, groupId, own, theirs, deleted, call } = fixture;
    await openChat(peer, call);
    const editButtons = chatMessages.querySelectorAll('button');
    assertChatEditEditDeleteVisibleOutsideOwnLiveText(editButtons);
    editButtons[0].click();
    if (!editDialog.open || editInput.value !== 'original' || !editSave.disabled) {
      throw new Error('Edit dialog did not load current own text');
    }
    editCancel.click();
    if (editDialog.open) throw new Error('Edit cancel left dialog open');
    editButtons[0].click();
    editInput.value = 'replacement';
    editInput.dispatchEvent(new Event('input'));
    let editedArgs;
    fixture.messages = [{ ...own, text: 'replacement', edited: true }, theirs, deleted];
    await editChatText((command, args) => {
      if (command === 'edit_chat_text') {
        editedArgs = args;
        return Promise.resolve([5]);
      }
      return call(command);
    });
    assertChatEditEditDidNotRefreshCurrentTextAnd(editedArgs, peer, groupId, own);
    chatMessages.querySelector('button').click();
    editInput.value = 'keep draft';
    editInput.dispatchEvent(new Event('input'));
    await editChatText((command) =>
      command === 'edit_chat_text' ? Promise.reject('diskFailed') : call(command),
    );
    assertChatEditUnstagedEditLostItsDraft();
    editInput.value = 'queued text';
    editInput.dispatchEvent(new Event('input'));
    await editChatText((command) => {
      if (command === 'edit_chat_text') {
        fixture.messages = [{ ...own, text: 'queued text', edited: true }, theirs, deleted];
        return Promise.reject('nodeFailed');
      }
      return call(command);
    });
    assertChatEditStagedEditOfferedADuplicateRetry();
    await checkChatEditStale(call);
  };
  const assertChatDeleteBestEffortDeleteWarningMissing = () => {
    if (
      !deleteDialog.open ||
      !deleteDialog.textContent.includes(window.strings['chats.deleteWarning'])
    ) {
      throw new Error('best-effort Delete warning missing');
    }
  };
  const assertChatDeleteDeleteDidNotShowTerminalPlaceholder = (
    deletedArgs,
    peer,
    own,
    deleteButton,
  ) => {
    if (
      !deletedArgs ||
      deletedArgs.peerId !== peer ||
      JSON.stringify(deletedArgs.groupId) !== JSON.stringify(own.groupId) ||
      JSON.stringify(deletedArgs.messageId) !== JSON.stringify(own.id) ||
      deleteDialog.open ||
      chatMessages.textContent.includes('remove me') ||
      !chatMessages.textContent.includes(window.strings['chats.deleted']) ||
      deleteButton() ||
      chatComposeStatus.textContent !== window.strings['chats.deleteSent']
    ) {
      throw new Error('Delete did not show terminal placeholder');
    }
  };
  const assertChatDeleteUnstagedDeleteHidOwnText = () => {
    if (
      !deleteDialog.open ||
      !deleteError.textContent.includes('diskFailed') ||
      !chatMessages.textContent.includes('second')
    ) {
      throw new Error('unstaged Delete hid own text');
    }
  };
  const assertChatDeleteStagedDeleteWasReportedAsFailure = () => {
    if (
      deleteDialog.open ||
      chatMessages.textContent.includes('second') ||
      chatComposeStatus.textContent !== window.strings['chats.deleteQueued']
    ) {
      throw new Error('staged Delete was reported as failure');
    }
  };
  const chatDeleteFixture = () => {
    const fixture = {};
    const peer = '9'.repeat(64);
    const own = {
      author: current.peerId,
      groupId: [6],
      id: [7],
      text: 'remove me',
      edited: false,
      deleted: false,
      sentAtMs: null,
    };
    const theirs = { ...own, author: peer, id: [8], text: 'keep theirs' };
    fixture.messages = [own, theirs];
    const row = { peerId: peer, state: 'open', lastMessage: own, unreadCount: 0 };
    const call = (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: fixture.messages });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([row]);
      throw new Error(`unexpected Delete command: ${command}`);
    };
    return Object.assign(fixture, { peer, own, theirs, row, call });
  };
  const checkChatDeleteStale = async (fixture, peer, own, theirs, call, deleteButton) => {
    fixture.messages = [{ ...own, id: [11], text: 'late' }, theirs];
    await openChat(peer, call);
    deleteButton().click();
    const finishDeleteCall = pendingSmokeCall();
    const oldDelete = deleteChatText((command) =>
      command === 'delete_chat_text' ? finishDeleteCall.call() : call(command),
    );
    const readyState = current;
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    finishDeleteCall.resolve([12]);
    await oldDelete;
    if (deleteDialog.open || !chatDetail.hidden) throw new Error('old Delete painted new identity');
  };
  const checkChatDelete = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const fixture = chatDeleteFixture();
    const { peer, own, theirs, call } = fixture;
    await openChat(peer, call);
    const deleteButton = () =>
      Array.from(chatMessages.querySelectorAll('button')).find(
        (button) => button.textContent === window.strings['chats.delete'],
      );
    if (!deleteButton() || chatMessages.querySelectorAll('button').length !== 4) {
      throw new Error('Delete action visible for peer text or Forward missing');
    }
    deleteButton().click();
    assertChatDeleteBestEffortDeleteWarningMissing();
    deleteCancel.click();
    if (deleteDialog.open || !chatMessages.textContent.includes('remove me')) {
      throw new Error('Cancel deleted own text');
    }
    deleteButton().click();
    let deletedArgs;
    await deleteChatText((command, args) => {
      if (command === 'delete_chat_text') {
        deletedArgs = args;
        fixture.messages = [{ ...own, text: null, deleted: true }, theirs];
        return Promise.resolve([9]);
      }
      return call(command);
    });
    assertChatDeleteDeleteDidNotShowTerminalPlaceholder(deletedArgs, peer, own, deleteButton);
    fixture.messages = [{ ...own, id: [10], text: 'second' }, theirs];
    await openChat(peer, call);
    deleteButton().click();
    await deleteChatText((command) =>
      command === 'delete_chat_text' ? Promise.reject('diskFailed') : call(command),
    );
    assertChatDeleteUnstagedDeleteHidOwnText();
    await deleteChatText((command) => {
      if (command === 'delete_chat_text') {
        fixture.messages = [{ ...fixture.messages[0], text: null, deleted: true }, theirs];
        return Promise.reject('nodeFailed');
      }
      return call(command);
    });
    assertChatDeleteStagedDeleteWasReportedAsFailure();
    await checkChatDeleteStale(fixture, peer, own, theirs, call, deleteButton);
  };
  const readNewDeviceFixture = (fixture) => {
    if (fixture.stop) {
      fixture.stop.reached();
      return fixture.stop.answered;
    }
    return fixture.raised === null
      ? Promise.reject(new Error('locked'))
      : Promise.resolve(fixture.raised);
  };
  const newDeviceFixture = () => {
    const fixture = {};
    const peer = '9'.repeat(64);
    const notice = document.getElementById('chat-new-device-notice');
    const status = document.getElementById('chats-status');
    fixture.raised = true;
    fixture.stop = null;
    fixture.acknowledge = () => Promise.reject(new Error('busy'));
    const call = (command) => {
      if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [] });
      if (command === 'contact_new_device') return readNewDeviceFixture(fixture);
      if (command === 'acknowledge_new_device') return fixture.acknowledge();
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([]);
      throw new Error(`unexpected new-device command: ${command}`);
    };
    return Object.assign(fixture, { peer, notice, status, call });
  };
  const checkNewDeviceStale = async (fixture) => {
    const { peer, notice, call } = fixture;
    const departures = [
      () => document.getElementById('chat-back').click(),
      () => {
        identityGeneration += 1;
      },
    ];
    for (const leave of departures) {
      const held = {};
      held.asked = new Promise((resolve) => {
        held.reached = resolve;
      });
      held.answered = new Promise((resolve) => {
        held.answer = resolve;
      });
      fixture.stop = held;
      const opening = openChat(peer, call);
      await held.asked;
      leave();
      held.answer(true);
      await opening;
      if (!notice.hidden) throw new Error('a stale answer raised the new-device notice');
    }
    fixture.stop = null;
    document.getElementById('chat-back').click();
  };
  const checkNewDevice = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const fixture = newDeviceFixture();
    const { peer, notice, status, call } = fixture;
    await openChat(peer, call);
    if (notice.hidden) throw new Error('a raised new-device notice stayed hidden');
    await shot('chat-new-device');
    for (const flag of [true, null]) {
      fixture.raised = flag;
      await window.mailUpdated(0, call);
      if (notice.hidden) throw new Error('a mail pass hid the new-device notice');
    }
    fixture.raised = true;
    await acknowledgeDevice(call);
    if (notice.hidden || !status.textContent.includes(window.strings['chats.newDevice.failed'])) {
      throw new Error('a failed acknowledgement dismissed the notice silently');
    }
    fixture.acknowledge = () => {
      fixture.raised = false;
      return Promise.resolve();
    };
    await acknowledgeDevice(call);
    if (!notice.hidden || document.activeElement !== chatInput) {
      throw new Error('the acknowledgement left the notice or lost focus');
    }
    fixture.raised = true;
    await openChat(peer, call);
    if (notice.hidden) throw new Error('reopening lost a raised notice');
    document.getElementById('chat-back').click();
    if (!notice.hidden) throw new Error('leaving the chat kept the notice');
    // An answer for a chat left behind, or for an earlier identity, shows nothing.
    await checkNewDeviceStale(fixture);
  };
  const assertChatDeliveryPendingOwnMessageOrPeerDeliveryStatus = () => {
    if (
      !chatMessages.textContent.includes(window.strings['chats.delivery.pending']) ||
      chatMessages.textContent.includes(window.strings['chats.delivery.nodeAccepted'])
    ) {
      throw new Error('pending own message or peer delivery status rendered incorrectly');
    }
  };
  const assertChatDeliveryNodeAcceptanceDidNotRefreshAnOpen = () => {
    if (
      !chatMessages.textContent.includes(window.strings['chats.delivery.nodeAccepted']) ||
      chatMessages.textContent.includes(window.strings['chats.delivery.pending'])
    ) {
      throw new Error('node acceptance did not refresh an open chat');
    }
  };
  const assertChatDeliveryDeviceReceiptStatusDidNotSurviveReopening = () => {
    if (
      !chatMessages.textContent.includes(window.strings['chats.delivery.deviceReceived']) ||
      chatMessages.textContent.includes(window.strings['chats.delivery.nodeAccepted'])
    ) {
      throw new Error('device receipt status did not survive reopening');
    }
  };
  const assertChatDeliveryAFailedPassHidTheCommittedMessage = () => {
    if (
      document.querySelectorAll('#chat-list button').length !== 1 ||
      !chatList.textContent.includes('incoming')
    ) {
      throw new Error('a failed pass hid the committed message');
    }
  };
  const checkChatDelivery = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const peer = '8'.repeat(64);
    const own = {
      author: current.peerId,
      groupId: [1],
      id: [1],
      text: 'outgoing',
      edited: false,
      deleted: false,
      sentAtMs: null,
    };
    const theirs = { ...own, author: peer, id: [2], text: 'incoming', delivery: 'nodeAccepted' };
    let delivery = 'pending';
    const call = (command) => {
      if (command === 'receive_mail') return Promise.resolve(0);
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [{ ...own, delivery }, theirs] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([]);
      throw new Error(`unexpected delivery command: ${command}`);
    };
    await openChat(peer, call);
    assertChatDeliveryPendingOwnMessageOrPeerDeliveryStatus();
    delivery = 'nodeAccepted';
    await window.mailUpdated(0, call);
    assertChatDeliveryNodeAcceptanceDidNotRefreshAnOpen();
    delivery = 'deviceReceived';
    document.getElementById('chat-back').click();
    await openChat(peer, call);
    assertChatDeliveryDeviceReceiptStatusDidNotSurviveReopening();
    document.getElementById('chat-back').click();
    // A lost ACK fails the pass after the commit; the committed row still appears, once.
    const committed = [{ peerId: peer, state: 'open', lastMessage: theirs, unreadCount: 1 }];
    await window.mailUpdated(null, (command) => {
      if (command === 'chat_rows') return Promise.resolve(committed);
      throw new Error(`unexpected refresh command: ${command}`);
    });
    assertChatDeliveryAFailedPassHidTheCommittedMessage();
  };
  const assertChatForwardedForwardedHistoryMarkerOrVerifiedMLSSender = (messages, peer) => {
    if (
      messages.length !== 2 ||
      !messages[0].textContent.includes(window.strings['chats.forwarded']) ||
      messages[1].textContent.includes(window.strings['chats.forwarded']) ||
      !messages[0].textContent.startsWith(shortPeerId(peer, knownPeers))
    ) {
      throw new Error('forwarded history marker or verified MLS sender was lost');
    }
  };
  const checkChatForwarded = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const peer = '7'.repeat(64);
    const forwarded = {
      author: peer,
      groupId: [1],
      id: [1],
      text: 'shared words',
      forwarded: true,
      edited: false,
      deleted: false,
      sentAtMs: null,
    };
    const ordinary = { ...forwarded, id: [2], text: 'own words', forwarded: false };
    renderChatRows([{ peerId: peer, state: 'open', lastMessage: forwarded, unreadCount: 0 }]);
    const preview = document.querySelector('#chat-list button .caption');
    if (!preview?.textContent.startsWith(`${window.strings['chats.forwarded']} · shared words`)) {
      throw new Error('forwarded label missing from chat list preview');
    }
    await openChat(peer, (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [forwarded, ordinary] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([]);
      throw new Error(`unexpected forwarded command: ${command}`);
    });
    const messages = chatMessages.querySelectorAll('p');
    assertChatForwardedForwardedHistoryMarkerOrVerifiedMLSSender(messages, peer);
    document.getElementById('chat-back').click();
  };
  const assertChatForwardActionForwardPickerIncludedARequestOrSource = (targetPeer) => {
    if (
      !forwardDialog.open ||
      forwardSelect.options.length !== 1 ||
      forwardSelect.value !== targetPeer
    )
      throw new Error('Forward picker included a request or source');
  };
  const assertChatForwardActionForwardActionDidNotSendTheSelected = (
    sentArgs,
    sourcePeer,
    source,
    targetPeer,
  ) => {
    if (
      !sentArgs ||
      sentArgs.sourcePeerId !== sourcePeer ||
      sentArgs.sourceAuthor !== sourcePeer ||
      JSON.stringify(sentArgs.sourceGroupId) !== JSON.stringify(source.groupId) ||
      JSON.stringify(sentArgs.sourceMessageId) !== JSON.stringify(source.id) ||
      sentArgs.targetPeerId !== targetPeer ||
      forwardDialog.open ||
      activeChat?.peerId !== targetPeer ||
      chatComposeStatus.textContent !== window.strings['chats.forwardSent']
    ) {
      throw new Error('Forward action did not send the selected source to the open target');
    }
  };
  const assertChatForwardActionStagedForwardWasReportedAsUnstagedFailure = (targetPeer) => {
    if (
      forwardDialog.open ||
      activeChat?.peerId !== targetPeer ||
      chatComposeStatus.textContent !== window.strings['chats.forwardQueued']
    ) {
      throw new Error('staged Forward was reported as unstaged failure');
    }
  };
  const assertChatForwardActionForwardPickerDidNotRejectAnAbsent = () => {
    if (
      !forwardDialog.open ||
      !forwardConfirm.disabled ||
      !forwardError.textContent.includes(window.strings['chats.forwardNoTargets'])
    ) {
      throw new Error('Forward picker did not reject an absent target');
    }
  };
  const stageForwardFixture = (fixture, source, args) => {
    fixture.sentArgs = args;
    if (fixture.sendError !== 'diskFailed') {
      fixture.targetMessages = [
        {
          ...source,
          author: current.peerId,
          groupId: [8],
          id: [9],
          forwarded: true,
          delivery: 'pending',
        },
      ];
    }
    return fixture.sendError ? Promise.reject(fixture.sendError) : Promise.resolve([9]);
  };
  const chatForwardFixture = () => {
    const fixture = {};
    const sourcePeer = '4'.repeat(64);
    const targetPeer = '5'.repeat(64);
    const requestPeer = '6'.repeat(64);
    const source = {
      author: sourcePeer,
      groupId: [3],
      id: [7],
      text: 'share this',
      forwarded: false,
      deleted: false,
      edited: false,
      sentAtMs: null,
    };
    const rows = [
      { peerId: sourcePeer, state: 'open', lastMessage: source, unreadCount: 0 },
      { peerId: targetPeer, state: 'open', lastMessage: null, unreadCount: 0 },
      { peerId: requestPeer, state: 'requestReceived', lastMessage: null, unreadCount: 0 },
    ];
    fixture.targetMessages = [];
    fixture.sendError = null;
    fixture.sentArgs = undefined;
    const call = (command, args) => {
      if (command === 'chat_history')
        return Promise.resolve({
          state: 'open',
          messages: args.peerId === sourcePeer ? [source] : fixture.targetMessages,
        });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve(rows);
      if (command === 'forward_chat_text') return stageForwardFixture(fixture, source, args);
      throw new Error(`unexpected Forward command: ${command}`);
    };
    renderChatRows(rows);
    return Object.assign(fixture, { sourcePeer, targetPeer, source, rows, call });
  };
  const checkChatForwardStale = async (sourcePeer, rows, call, forwardButton) => {
    renderChatRows(rows);
    await openChat(sourcePeer, call);
    forwardButton().click();
    let finishForward;
    const commandStartedCall = pendingSmokeCall();
    const started = commandStartedCall.call();
    const late = forwardChatText((command, args) =>
      command === 'forward_chat_text'
        ? new Promise((resolve) => {
            finishForward = resolve;
            commandStartedCall.resolve();
          })
        : call(command, args),
    );
    await started;
    const readyState = current;
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    finishForward([10]);
    await late;
    if (forwardDialog.open || !chatDetail.hidden)
      throw new Error('old Forward painted new identity');
  };
  const checkChatForwardAction = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const fixture = chatForwardFixture();
    const { sourcePeer, targetPeer, source, rows, call } = fixture;
    await openChat(sourcePeer, call);
    const forwardButton = () =>
      Array.from(chatMessages.querySelectorAll('button')).find(
        (button) => button.textContent === window.strings['chats.forward'],
      );
    forwardButton().click();
    assertChatForwardActionForwardPickerIncludedARequestOrSource(targetPeer);
    forwardCancel.click();
    if (forwardDialog.open || fixture.sentArgs) throw new Error('Forward cancel staged a message');
    forwardButton().click();
    fixture.sendError = 'diskFailed';
    await forwardChatText(call);
    if (!forwardDialog.open || !forwardError.textContent.includes('diskFailed')) {
      throw new Error('unstaged Forward closed the target picker');
    }
    fixture.sendError = null;
    await forwardChatText(call);
    assertChatForwardActionForwardActionDidNotSendTheSelected(
      fixture.sentArgs,
      sourcePeer,
      source,
      targetPeer,
    );
    await openChat(sourcePeer, call);
    fixture.targetMessages = [];
    fixture.sendError = 'nodeFailed';
    forwardButton().click();
    await forwardChatText(call);
    assertChatForwardActionStagedForwardWasReportedAsUnstagedFailure(targetPeer);
    await openChat(sourcePeer, call);
    renderChatRows([rows[0]]);
    forwardButton().click();
    assertChatForwardActionForwardPickerDidNotRejectAnAbsent();
    document.getElementById('chat-back').click();
    if (forwardDialog.open) throw new Error('Forward dialog survived chat navigation');
    await checkChatForwardStale(sourcePeer, rows, call, forwardButton);
  };
  const assertChatLinkHostsIDNOrUserinfoLinkHostWasMisidentified = (hosts) => {
    if (
      hosts.length !== 2 ||
      hosts[0] !== 'xn--bcher-kva.example' ||
      hosts[1] !== 'evil.example' ||
      chatLinkHosts('javascript:alert(1)').length
    ) {
      throw new Error('IDN or userinfo link host was misidentified');
    }
  };
  const assertChatLinkHostsLinkHostMissingOrLinkPreviewWas = (rendered, hosts) => {
    if (
      !rendered?.textContent.includes(`${window.strings['chats.linkHost']}: ${hosts.join(', ')}`) ||
      rendered.querySelector('a, img, iframe')
    ) {
      throw new Error('link host missing or link preview was created');
    }
  };
  const checkChatLinkHosts = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const peer = '3'.repeat(64);
    const text = 'See https://bücher.example/path and https://paypal.com@evil.example/login';
    const hosts = chatLinkHosts(text);
    assertChatLinkHostsIDNOrUserinfoLinkHostWasMisidentified(hosts);
    const message = {
      author: peer,
      groupId: [1],
      id: [1],
      text,
      deleted: false,
      edited: false,
      sentAtMs: null,
    };
    await openChat(peer, (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'requestReceived', messages: [message] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([]);
      throw new Error(`unexpected link host command: ${command}`);
    });
    const rendered = chatMessages.querySelector('p');
    assertChatLinkHostsLinkHostMissingOrLinkPreviewWas(rendered, hosts);
    document.getElementById('chat-back').click();
  };
  const assertChatReadReadMarkIsNotOnTheCovered = (seen, delivered, later) => {
    if (
      !seen.textContent.includes(window.strings['chats.delivery.read']) ||
      seen.textContent.includes(delivered) ||
      !later.textContent.includes(delivered) ||
      later.textContent.includes(window.strings['chats.delivery.read'])
    ) {
      throw new Error('read mark is not on the covered text alone');
    }
  };
  const checkChatRead = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const peer = '5'.repeat(64);
    const own = {
      author: current.peerId,
      groupId: [1],
      text: 'seen',
      deleted: false,
      edited: false,
      delivery: 'nodeAccepted',
      sentAtMs: null,
    };
    const messages = [
      { ...own, id: [1], read: true },
      { ...own, id: [2], text: 'later', read: false },
    ];
    const sent = [];
    const call = (command) => {
      sent.push(command);
      if (command === 'chat_history') return Promise.resolve({ state: 'open', messages });
      return Promise.resolve(command === 'chat_rows' ? [] : 0);
    };
    Object.defineProperty(document, 'hidden', { configurable: true, value: true });
    await openChat(peer, call);
    const [seen, later] = chatMessages.querySelectorAll('.msg');
    const delivered = window.strings['chats.delivery.nodeAccepted'];
    assertChatReadReadMarkIsNotOnTheCovered(seen, delivered, later);
    if (sent.includes('mark_chat_read') || sent.includes('send_read_receipts')) {
      throw new Error('a hidden window counted as reading');
    }
    delete document.hidden;
    const shown = window.mailUpdated;
    let pass;
    window.mailUpdated = (changed) => {
      pass = shown(changed, call);
    };
    document.dispatchEvent(new Event('visibilitychange'));
    window.mailUpdated = shown;
    await pass;
    await window.chatRead;
    if (!sent.includes('mark_chat_read') || !sent.includes('send_read_receipts')) {
      throw new Error('returning to the window did not read the chat');
    }
    document.getElementById('chat-back').click();
  };
  globalThis.uiSmokeChatMessages = Object.freeze({
    checkChatEdit,
    checkChatDelete,
    checkNewDevice,
    checkChatDelivery,
    checkChatForwarded,
    checkChatForwardAction,
    checkChatLinkHosts,
    checkChatRead,
  });
}
