// Keep keyboard submission on the same validated send path as the button.
chatInput.addEventListener('keydown', (event) => {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  if (!chatSend.disabled) document.getElementById('chat-composer').requestSubmit();
});
document.getElementById('chat-stickers').addEventListener('click', () => {
  if (chatInput.disabled || activeChat?.state !== 'open') return;
  window.stickersShown = window.stickerCollection.open();
});
