// Keep keyboard submission on the same validated send path as the button.
chatInput.addEventListener('keydown', (event) => {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  if (!chatSend.disabled) document.getElementById('chat-composer').requestSubmit();
});
document.getElementById('chat-stickers').addEventListener('click', () => {
  if (chatInput.disabled || activeChat?.state !== 'open') return;
  const peerId = activeChat.peerId;
  const generation = identityGeneration;
  window.stickersShown = window.stickerPicker.open({
    peerId,
    onSent: async () => {
      if (!identityCurrent(generation) || activeChat?.peerId !== peerId) return;
      await openChat(peerId);
      if (identityCurrent(generation) && activeChat?.peerId === peerId)
        chatComposeStatus.textContent = window.strings['stickerPicker.queued'];
    },
  });
});
