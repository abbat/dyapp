// Scoped fixture/assertion helpers and shared smoke scenarios.
{
  const assertChatEditEditDeleteVisibleOutsideOwnLiveText = (editButtons) => {
    if (
      editButtons.length !== 4 ||
      editButtons[0].textContent !== appApi.strings['chats.edit'] ||
      editButtons[1].textContent !== appApi.strings['chats.delete'] ||
      editButtons[2].textContent !== appApi.strings['chats.forward'] ||
      editButtons[3].textContent !== appApi.strings['chats.forward']
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
      appBindings.editDialog.open ||
      !appBindings.chatMessages.textContent.includes(appApi.strings['chats.edited']) ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.editSent']
    ) {
      throw new Error('Edit did not refresh current text and marker');
    }
  };
  const assertChatEditUnstagedEditLostItsDraft = () => {
    if (
      !appBindings.editDialog.open ||
      appBindings.editInput.value !== 'keep draft' ||
      !appBindings.editError.textContent.includes('diskFailed')
    ) {
      throw new Error('unstaged Edit lost its draft');
    }
  };
  const assertChatEditStagedEditOfferedADuplicateRetry = () => {
    if (
      appBindings.editDialog.open ||
      !appBindings.chatMessages.textContent.includes('queued text') ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.editQueued']
    ) {
      throw new Error('staged Edit offered a duplicate retry');
    }
  };
  const chatEditFixture = () => {
    const fixture = {};
    const peer = 'f'.repeat(64);
    const groupId = [8, 9];
    const own = {
      author: appBindings.current.peerId,
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
    appBindings.chatMessages.querySelector('button').click();
    appBindings.editInput.value = 'late';
    appBindings.editInput.dispatchEvent(new Event('input'));
    await appApi.editShown;
    const finishEditCall = smoke.pendingSmokeCall();
    const oldEdit = appBindings.editChatText((command) =>
      command === 'edit_chat_text' ? finishEditCall.call() : call(command),
    );
    const readyState = appBindings.current;
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    finishEditCall.resolve([6]);
    await oldEdit;
    if (appBindings.editDialog.open || !appBindings.chatDetail.hidden)
      throw new Error('old Edit painted new identity');
  };
  const checkChatEdit = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const fixture = chatEditFixture();
    const { peer, groupId, own, theirs, deleted, call } = fixture;
    await appBindings.openChat(peer, call);
    const editButtons = appBindings.chatMessages.querySelectorAll('button');
    assertChatEditEditDeleteVisibleOutsideOwnLiveText(editButtons);
    editButtons[0].click();
    if (
      !appBindings.editDialog.open ||
      appBindings.editInput.value !== 'original' ||
      !appBindings.editSave.disabled
    ) {
      throw new Error('Edit dialog did not load current own text');
    }
    appBindings.editCancel.click();
    if (appBindings.editDialog.open) throw new Error('Edit cancel left dialog open');
    editButtons[0].click();
    appBindings.editInput.value = 'replacement';
    appBindings.editInput.dispatchEvent(new Event('input'));
    await appApi.editShown;
    let editedArgs;
    fixture.messages = [{ ...own, text: 'replacement', edited: true }, theirs, deleted];
    await appBindings.editChatText((command, args) => {
      if (command === 'edit_chat_text') {
        editedArgs = args;
        return Promise.resolve([5]);
      }
      return call(command);
    });
    assertChatEditEditDidNotRefreshCurrentTextAnd(editedArgs, peer, groupId, own);
    appBindings.chatMessages.querySelector('button').click();
    appBindings.editInput.value = 'keep draft';
    appBindings.editInput.dispatchEvent(new Event('input'));
    await appApi.editShown;
    await appBindings.editChatText((command) =>
      command === 'edit_chat_text' ? Promise.reject('diskFailed') : call(command),
    );
    assertChatEditUnstagedEditLostItsDraft();
    appBindings.editInput.value = 'queued text';
    appBindings.editInput.dispatchEvent(new Event('input'));
    await appApi.editShown;
    await appBindings.editChatText((command) => {
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
      !appBindings.deleteDialog.open ||
      !appBindings.deleteDialog.textContent.includes(appApi.strings['chats.deleteWarning'])
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
      appBindings.deleteDialog.open ||
      appBindings.chatMessages.textContent.includes('remove me') ||
      !appBindings.chatMessages.textContent.includes(appApi.strings['chats.deleted']) ||
      deleteButton() ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.deleteSent']
    ) {
      throw new Error('Delete did not show terminal placeholder');
    }
  };
  const assertChatDeleteUnstagedDeleteHidOwnText = () => {
    if (
      !appBindings.deleteDialog.open ||
      !appBindings.deleteError.textContent.includes('diskFailed') ||
      !appBindings.chatMessages.textContent.includes('second')
    ) {
      throw new Error('unstaged Delete hid own text');
    }
  };
  const assertChatDeleteStagedDeleteWasReportedAsFailure = () => {
    if (
      appBindings.deleteDialog.open ||
      appBindings.chatMessages.textContent.includes('second') ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.deleteQueued']
    ) {
      throw new Error('staged Delete was reported as failure');
    }
  };
  const chatDeleteFixture = () => {
    const fixture = {};
    const peer = '9'.repeat(64);
    const own = {
      author: appBindings.current.peerId,
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
    await appBindings.openChat(peer, call);
    deleteButton().click();
    const finishDeleteCall = smoke.pendingSmokeCall();
    const oldDelete = appBindings.deleteChatText((command) =>
      command === 'delete_chat_text' ? finishDeleteCall.call() : call(command),
    );
    const readyState = appBindings.current;
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    finishDeleteCall.resolve([12]);
    await oldDelete;
    if (appBindings.deleteDialog.open || !appBindings.chatDetail.hidden)
      throw new Error('old Delete painted new identity');
  };
  const checkChatDelete = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const fixture = chatDeleteFixture();
    const { peer, own, theirs, call } = fixture;
    await appBindings.openChat(peer, call);
    const deleteButton = () =>
      Array.from(appBindings.chatMessages.querySelectorAll('button')).find(
        (button) => button.textContent === appApi.strings['chats.delete'],
      );
    if (!deleteButton() || appBindings.chatMessages.querySelectorAll('button').length !== 4) {
      throw new Error('Delete action visible for peer text or Forward missing');
    }
    deleteButton().click();
    assertChatDeleteBestEffortDeleteWarningMissing();
    appBindings.deleteCancel.click();
    if (
      appBindings.deleteDialog.open ||
      !appBindings.chatMessages.textContent.includes('remove me')
    ) {
      throw new Error('Cancel deleted own text');
    }
    deleteButton().click();
    let deletedArgs;
    await appBindings.deleteChatText((command, args) => {
      if (command === 'delete_chat_text') {
        deletedArgs = args;
        fixture.messages = [{ ...own, text: null, deleted: true }, theirs];
        return Promise.resolve([9]);
      }
      return call(command);
    });
    assertChatDeleteDeleteDidNotShowTerminalPlaceholder(deletedArgs, peer, own, deleteButton);
    fixture.messages = [{ ...own, id: [10], text: 'second' }, theirs];
    await appBindings.openChat(peer, call);
    deleteButton().click();
    await appBindings.deleteChatText((command) =>
      command === 'delete_chat_text' ? Promise.reject('diskFailed') : call(command),
    );
    assertChatDeleteUnstagedDeleteHidOwnText();
    await appBindings.deleteChatText((command) => {
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
        appBindings.identityGeneration += 1;
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
      const opening = appBindings.openChat(peer, call);
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
    await appApi.chatsDone;
    const fixture = newDeviceFixture();
    const { peer, notice, status, call } = fixture;
    await appBindings.openChat(peer, call);
    if (notice.hidden) throw new Error('a raised new-device notice stayed hidden');
    await smoke.shot('chat-new-device');
    for (const flag of [true, null]) {
      fixture.raised = flag;
      await appApi.mailUpdated(0, call);
      if (notice.hidden) throw new Error('a mail pass hid the new-device notice');
    }
    fixture.raised = true;
    await appBindings.acknowledgeDevice(call);
    if (notice.hidden || !status.textContent.includes(appApi.strings['chats.newDevice.failed'])) {
      throw new Error('a failed acknowledgement dismissed the notice silently');
    }
    fixture.acknowledge = () => {
      fixture.raised = false;
      return Promise.resolve();
    };
    await appBindings.acknowledgeDevice(call);
    if (!notice.hidden || document.activeElement !== appBindings.chatInput) {
      throw new Error('the acknowledgement left the notice or lost focus');
    }
    fixture.raised = true;
    await appBindings.openChat(peer, call);
    if (notice.hidden) throw new Error('reopening lost a raised notice');
    document.getElementById('chat-back').click();
    if (!notice.hidden) throw new Error('leaving the chat kept the notice');
    // An answer for a chat left behind, or for an earlier identity, shows nothing.
    await checkNewDeviceStale(fixture);
  };
  const assertChatDeliveryPendingOwnMessageOrPeerDeliveryStatus = () => {
    if (
      !appBindings.chatMessages.textContent.includes(appApi.strings['chats.delivery.pending']) ||
      appBindings.chatMessages.textContent.includes(appApi.strings['chats.delivery.nodeAccepted'])
    ) {
      throw new Error('pending own message or peer delivery status rendered incorrectly');
    }
  };
  const assertChatDeliveryNodeAcceptanceDidNotRefreshAnOpen = () => {
    if (
      !appBindings.chatMessages.textContent.includes(
        appApi.strings['chats.delivery.nodeAccepted'],
      ) ||
      appBindings.chatMessages.textContent.includes(appApi.strings['chats.delivery.pending'])
    ) {
      throw new Error('node acceptance did not refresh an open chat');
    }
  };
  const assertChatDeliveryDeviceReceiptStatusDidNotSurviveReopening = () => {
    if (
      !appBindings.chatMessages.textContent.includes(
        appApi.strings['chats.delivery.deviceReceived'],
      ) ||
      appBindings.chatMessages.textContent.includes(appApi.strings['chats.delivery.nodeAccepted'])
    ) {
      throw new Error('device receipt status did not survive reopening');
    }
  };
  const assertChatDeliveryAFailedPassHidTheCommittedMessage = () => {
    if (
      document.querySelectorAll('#chat-list button').length !== 1 ||
      !appBindings.chatList.textContent.includes('incoming')
    ) {
      throw new Error('a failed pass hid the committed message');
    }
  };
  const checkChatDelivery = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const peer = '8'.repeat(64);
    const own = {
      author: appBindings.current.peerId,
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
    await appBindings.openChat(peer, call);
    assertChatDeliveryPendingOwnMessageOrPeerDeliveryStatus();
    delivery = 'nodeAccepted';
    await appApi.mailUpdated(0, call);
    assertChatDeliveryNodeAcceptanceDidNotRefreshAnOpen();
    delivery = 'deviceReceived';
    document.getElementById('chat-back').click();
    await appBindings.openChat(peer, call);
    assertChatDeliveryDeviceReceiptStatusDidNotSurviveReopening();
    document.getElementById('chat-back').click();
    // A lost ACK fails the pass after the commit; the committed row still appears, once.
    const committed = [{ peerId: peer, state: 'open', lastMessage: theirs, unreadCount: 1 }];
    await appApi.mailUpdated(null, (command) => {
      if (command === 'chat_rows') return Promise.resolve(committed);
      throw new Error(`unexpected refresh command: ${command}`);
    });
    assertChatDeliveryAFailedPassHidTheCommittedMessage();
  };
  const assertChatForwardedForwardedHistoryMarkerOrVerifiedMLSSender = (messages, peer) => {
    if (
      messages.length !== 2 ||
      !messages[0].textContent.includes(appApi.strings['chats.forwarded']) ||
      messages[1].textContent.includes(appApi.strings['chats.forwarded']) ||
      !messages[0].textContent.startsWith(appBindings.shortPeerId(peer))
    ) {
      throw new Error('forwarded history marker or verified MLS sender was lost');
    }
  };
  const checkChatForwarded = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
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
    appBindings.renderChatRows([
      { peerId: peer, state: 'open', lastMessage: forwarded, unreadCount: 0 },
    ]);
    const preview = document.querySelector('#chat-list button .caption');
    if (!preview?.textContent.startsWith(`${appApi.strings['chats.forwarded']} · shared words`)) {
      throw new Error('forwarded label missing from chat list preview');
    }
    await appBindings.openChat(peer, (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [forwarded, ordinary] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([]);
      throw new Error(`unexpected forwarded command: ${command}`);
    });
    const messages = appBindings.chatMessages.querySelectorAll('p');
    assertChatForwardedForwardedHistoryMarkerOrVerifiedMLSSender(messages, peer);
    document.getElementById('chat-back').click();
  };
  const assertChatForwardActionForwardPickerIncludedARequestOrSource = (targetPeer) => {
    if (
      !appBindings.forwardDialog.open ||
      appBindings.forwardSelect.options.length !== 1 ||
      appBindings.forwardSelect.value !== targetPeer
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
      appBindings.forwardDialog.open ||
      appBindings.activeChat?.peerId !== targetPeer ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.forwardSent']
    ) {
      throw new Error('Forward action did not send the selected source to the open target');
    }
  };
  const assertChatForwardActionStagedForwardWasReportedAsUnstagedFailure = (targetPeer) => {
    if (
      appBindings.forwardDialog.open ||
      appBindings.activeChat?.peerId !== targetPeer ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.forwardQueued']
    ) {
      throw new Error('staged Forward was reported as unstaged failure');
    }
  };
  const assertChatForwardActionForwardPickerDidNotRejectAnAbsent = () => {
    if (
      !appBindings.forwardDialog.open ||
      !appBindings.forwardConfirm.disabled ||
      !appBindings.forwardError.textContent.includes(appApi.strings['chats.forwardNoTargets'])
    ) {
      throw new Error('Forward picker did not reject an absent target');
    }
  };
  // Refusals reject; committed forwards resolve with their ID and any queued delivery error.
  const stageForwardFixture = (fixture, source, args) => {
    fixture.sentArgs = args;
    fixture.sentCount += 1;
    if (fixture.sendError === 'diskFailed') return Promise.reject(fixture.sendError);
    fixture.targetMessages = [
      {
        ...source,
        text: fixture.sentText ?? source.text,
        author: appBindings.current.peerId,
        groupId: [8],
        id: [9],
        forwarded: true,
        delivery: 'pending',
      },
    ];
    return Promise.resolve({ id: [9], queued: fixture.sendError });
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
    fixture.sentCount = 0;
    fixture.sentText = null;
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
    appBindings.renderChatRows(rows);
    return Object.assign(fixture, { sourcePeer, targetPeer, source, rows, call });
  };
  const checkChatForwardStale = async (sourcePeer, rows, call, forwardButton) => {
    appBindings.renderChatRows(rows);
    await appBindings.openChat(sourcePeer, call);
    forwardButton().click();
    let finishForward;
    const commandStartedCall = smoke.pendingSmokeCall();
    const started = commandStartedCall.call();
    const late = appBindings.forwardChatText((command, args) =>
      command === 'forward_chat_text'
        ? new Promise((resolve) => {
            finishForward = resolve;
            commandStartedCall.resolve();
          })
        : call(command, args),
    );
    await started;
    const readyState = appBindings.current;
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    finishForward({ id: [10], queued: null });
    await late;
    if (appBindings.forwardDialog.open || !appBindings.chatDetail.hidden)
      throw new Error('old Forward painted new identity');
  };
  const checkChatForwardQueued = async (fixture, forwardButton) => {
    // The core forwards the current text, so the UI must not match the selected text to find it.
    fixture.targetMessages = [];
    fixture.sendError = 'nodeFailed';
    fixture.sentText = 'edited after selection';
    forwardButton().click();
    const sentBefore = fixture.sentCount;
    await appBindings.forwardChatText(fixture.call);
    await appBindings.forwardChatText(fixture.call);
    if (fixture.sentCount !== sentBefore + 1)
      throw new Error('queued Forward allowed a fresh resubmission');
    fixture.sentText = null;
    assertChatForwardActionStagedForwardWasReportedAsUnstagedFailure(fixture.targetPeer);
  };
  const checkChatForwardAction = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const fixture = chatForwardFixture();
    const { sourcePeer, targetPeer, source, rows, call } = fixture;
    await appBindings.openChat(sourcePeer, call);
    const forwardButton = () =>
      Array.from(appBindings.chatMessages.querySelectorAll('button')).find(
        (button) => button.textContent === appApi.strings['chats.forward'],
      );
    forwardButton().click();
    assertChatForwardActionForwardPickerIncludedARequestOrSource(targetPeer);
    appBindings.forwardCancel.click();
    if (appBindings.forwardDialog.open || fixture.sentArgs)
      throw new Error('Forward cancel staged a message');
    forwardButton().click();
    fixture.sendError = 'diskFailed';
    await appBindings.forwardChatText(call);
    if (
      !appBindings.forwardDialog.open ||
      !appBindings.forwardError.textContent.includes('diskFailed')
    ) {
      throw new Error('unstaged Forward closed the target picker');
    }
    fixture.sendError = null;
    await appBindings.forwardChatText(call);
    assertChatForwardActionForwardActionDidNotSendTheSelected(
      fixture.sentArgs,
      sourcePeer,
      source,
      targetPeer,
    );
    await appBindings.openChat(sourcePeer, call);
    await checkChatForwardQueued(fixture, forwardButton);
    await appBindings.openChat(sourcePeer, call);
    appBindings.renderChatRows([rows[0]]);
    forwardButton().click();
    assertChatForwardActionForwardPickerDidNotRejectAnAbsent();
    document.getElementById('chat-back').click();
    if (appBindings.forwardDialog.open) throw new Error('Forward dialog survived chat navigation');
    await checkChatForwardStale(sourcePeer, rows, call, forwardButton);
  };
  const assertChatLinkHostsIDNOrUserinfoLinkHostWasMisidentified = (hosts) => {
    if (
      hosts.length !== 2 ||
      hosts[0] !== 'xn--bcher-kva.example' ||
      hosts[1] !== 'evil.example' ||
      appBindings.chatLinkHosts('javascript:alert(1)').length
    ) {
      throw new Error('IDN or userinfo link host was misidentified');
    }
  };
  const assertChatLinkHostsLinkHostMissingOrLinkPreviewWas = (rendered, hosts) => {
    if (
      !rendered?.textContent.includes(`${appApi.strings['chats.linkHost']}: ${hosts.join(', ')}`) ||
      rendered.querySelector('a, img, iframe')
    ) {
      throw new Error('link host missing or link preview was created');
    }
  };
  const checkChatLinkHosts = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const peer = '3'.repeat(64);
    const text = 'See https://bücher.example/path and https://paypal.com@evil.example/login';
    const hosts = appBindings.chatLinkHosts(text);
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
    await appBindings.openChat(peer, (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'requestReceived', messages: [message] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([]);
      throw new Error(`unexpected link host command: ${command}`);
    });
    const rendered = appBindings.chatMessages.querySelector('p');
    assertChatLinkHostsLinkHostMissingOrLinkPreviewWas(rendered, hosts);
    document.getElementById('chat-back').click();
  };
  const assertChatReadReadMarkIsNotOnTheCovered = (seen, delivered, later) => {
    if (
      !seen.textContent.includes(appApi.strings['chats.delivery.read']) ||
      seen.textContent.includes(delivered) ||
      !later.textContent.includes(delivered) ||
      later.textContent.includes(appApi.strings['chats.delivery.read'])
    ) {
      throw new Error('read mark is not on the covered text alone');
    }
  };
  const checkChatRead = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const peer = '5'.repeat(64);
    const own = {
      author: appBindings.current.peerId,
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
    await appBindings.openChat(peer, call);
    const [seen, later] = appBindings.chatMessages.querySelectorAll('.msg');
    const delivered = appApi.strings['chats.delivery.nodeAccepted'];
    assertChatReadReadMarkIsNotOnTheCovered(seen, delivered, later);
    if (sent.includes('mark_chat_read') || sent.includes('send_read_receipts')) {
      throw new Error('a hidden window counted as reading');
    }
    delete document.hidden;
    const shown = appApi.mailUpdated;
    let pass;
    appApi.mailUpdated = (changed) => {
      pass = shown(changed, call);
    };
    document.dispatchEvent(new Event('visibilitychange'));
    appApi.mailUpdated = shown;
    await pass;
    await appApi.chatRead;
    if (!sent.includes('mark_chat_read') || !sent.includes('send_read_receipts')) {
      throw new Error('returning to the window did not read the chat');
    }
    document.getElementById('chat-back').click();
  };
  smoke.uiSmokeChatMessages = Object.freeze({
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
