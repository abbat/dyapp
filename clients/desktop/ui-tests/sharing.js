// The loopback pair shares the sender's public profile, so no third published identity is needed.
Object.assign(peerSteps, {
  'share-profile-send': async (peer) => {
    await openArea('chats-area');
    await appApi.chatsDone;
    await appBindings.openChat(peer);
    await appBindings.openShare(appBindings.current.peerId);
    if (
      !appBindings.forwardDialog.open ||
      ![...appBindings.forwardSelect.options].some((item) => item.value === peer)
    ) {
      throw Error('Share dialog omitted the established conversation');
    }
    appBindings.forwardSelect.value = peer;
    document.getElementById('chat-forward-form').requestSubmit();
    await appApi.profileShared;
    const history = await appBindings.invoke('chat_history', { peerId: peer });
    const link = `dyapp-profile:1:${appBindings.current.peerId}`;
    const shared = history.messages.find((message) => message.text === link);
    if (
      appBindings.forwardDialog.open ||
      !shared ||
      shared.sharedPeer !== appBindings.current.peerId ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.shareSent']
    ) {
      throw Error('Share did not persist exactly the public profile link');
    }
  },
  'share-profile-open': async (peer) => {
    const deadline = performance.now() + 30000;
    let shared;
    do {
      await appBindings.invoke('receive_mail');
      const history = await appBindings.invoke('chat_history', { peerId: peer });
      shared = history.messages.find(
        (message) => message.author === peer && message.sharedPeer === peer,
      );
      if (shared) break;
    } while (performance.now() < deadline);
    if (!shared || shared.text !== `dyapp-profile:1:${peer}`)
      throw Error('Shared public link did not arrive');
    await openArea('chats-area');
    await appApi.chatsDone;
    await appBindings.openChat(peer);
    const card = appBindings.chatMessages.querySelector(`button[data-peer-id="${peer}"]`);
    if (!card) throw Error('Received link has no profile card');
    card.click();
    await appApi.sharedOpened;
    if (
      appBindings.viewed?.found.peerId !== peer ||
      document.getElementById('view-message').disabled
    ) {
      throw Error('Card did not open the other verified profile');
    }
    await smoke.shot('shared-profile-peer');
    document.getElementById('view-message').click();
    await appApi.chatOpened;
    if (
      appBindings.activeChat?.peerId !== peer ||
      appBindings.activeChat.state !== 'open' ||
      appBindings.chatInput.disabled
    ) {
      throw Error('Message on the shared profile did not return to the writable conversation');
    }
  },
});
