/** Named core outcomes only: no code parsing, expiry calculation, authorization or native writes. */
export const installOwnerFixture = (strings, screen) => {
  if (!screen.startsWith('owner-')) return;
  const code = document.getElementById('link-receive-code').textContent.trim();
  document.getElementById('owner-code-input').value =
    screen === 'owner-code' ? '' : screen === 'owner-malformed' ? 'not-a-link-code' : code;
  document.getElementById('owner-identity').textContent = document
    .getElementById('peer-id')
    .textContent.trim();
  document.getElementById('owner-recipient').textContent = code;
  if (screen === 'owner-sent')
    document.getElementById('owner-confirm-title').textContent = strings['devices.sent'];
};
