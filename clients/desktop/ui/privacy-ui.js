// Settings → Privacy: stored per identity on this device, saved on every change.
const privacyInputs = {
  sendViews: 'send-views',
  sendReadReceipts: 'send-read-receipts',
  sendTyping: 'send-typing',
  presenceMatches: 'presence-matches',
  notifications: 'message-notifications',
};
const privacyError = document.getElementById('privacy-error');
const noticeContent = document.getElementById('notice-content');
const showNoticeChoice = () => {
  noticeContent.disabled = !document.getElementById('message-notifications').checked;
};
window.showPrivacy = async (call = invoke) => {
  const generation = identityGeneration;
  try {
    const privacy = await call('privacy_settings');
    if (!identityCurrent(generation)) return;
    for (const [key, id] of Object.entries(privacyInputs)) {
      document.getElementById(id).checked = privacy[key];
    }
    document.getElementById('presence-nobody').checked = !privacy.presenceMatches;
    noticeContent.value = privacy.noticeContent;
    showNoticeChoice();
    privacyError.hidden = true;
  } catch (_) {
    if (identityCurrent(generation)) privacyError.hidden = false;
  }
};
const savePrivacy = (call = invoke) => {
  const generation = identityGeneration;
  const privacy = Object.fromEntries(Object.entries(privacyInputs)
    .map(([key, id]) => [key, document.getElementById(id).checked]));
  privacy.noticeContent = noticeContent.value;
  return call('set_privacy', { privacy }).then(
    () => { if (identityCurrent(generation)) privacyError.hidden = true; },
    () => { if (identityCurrent(generation)) privacyError.hidden = false; },
  );
};
document.getElementById('privacy').addEventListener('change', () => {
  showNoticeChoice();
  window.privacySaved = savePrivacy();
});
