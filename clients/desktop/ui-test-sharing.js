// The loopback pair shares the sender's public profile, so no third published identity is needed.
Object.assign(peerSteps, {
  'share-profile-send': async (peer) => {
    await openArea('chats-area'); await window.chatsDone;
    await openChat(peer);
    await openShare(current.peerId);
    if (!forwardDialog.open || ![...forwardSelect.options].some((item) => item.value === peer)) {
      throw Error('Share dialog omitted the established conversation');
    }
    forwardSelect.value = peer;
    document.getElementById('chat-forward-form').requestSubmit();
    await window.profileShared;
    const history = await invoke('chat_history', { peerId: peer });
    const link = `dyapp-profile:1:${current.peerId}`;
    const shared = history.messages.find((message) => message.text === link);
    if (forwardDialog.open || !shared || shared.sharedPeer !== current.peerId
      || chatComposeStatus.textContent !== window.strings['chats.shareSent']) {
      throw Error('Share did not persist exactly the public profile link');
    }
  },
  'share-profile-open': async (peer) => {
    const deadline = performance.now() + 30000;
    let shared;
    do {
      await invoke('receive_mail');
      const history = await invoke('chat_history', { peerId: peer });
      shared = history.messages.find((message) => message.author === peer && message.sharedPeer === peer);
      if (shared) break;
    } while (performance.now() < deadline);
    if (!shared || shared.text !== `dyapp-profile:1:${peer}`) throw Error('Shared public link did not arrive');
    await openArea('chats-area'); await window.chatsDone;
    await openChat(peer);
    const card = chatMessages.querySelector(`button[data-peer-id="${peer}"]`);
    if (!card) throw Error('Received link has no profile card');
    card.click(); await window.sharedOpened;
    if (viewed?.found.peerId !== peer || document.getElementById('view-message').disabled) {
      throw Error('Card did not open the other verified profile');
    }
    await shot('shared-profile-peer');
    document.getElementById('view-message').click();
    await window.chatOpened;
    if (activeChat?.peerId !== peer || activeChat.state !== 'open' || chatInput.disabled) {
      throw Error('Message on the shared profile did not return to the writable conversation');
    }
  },
});
