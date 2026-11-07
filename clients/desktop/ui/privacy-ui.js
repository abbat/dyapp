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
const privacyFailure = (failed) => {
  privacyError.hidden = !failed;
  document.getElementById('notifications-error').hidden = !failed;
};
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
    privacyFailure(false);
  } catch (_) {
    if (identityCurrent(generation)) privacyFailure(true);
  }
};
const savePrivacy = (call = invoke) => {
  const generation = identityGeneration;
  const privacy = Object.fromEntries(
    Object.entries(privacyInputs).map(([key, id]) => [key, document.getElementById(id).checked]),
  );
  privacy.noticeContent = noticeContent.value;
  return call('set_privacy', { privacy }).then(
    () => {
      if (identityCurrent(generation)) privacyFailure(false);
    },
    () => {
      if (identityCurrent(generation)) privacyFailure(true);
    },
  );
};
for (const id of ['privacy', 'notifications']) {
  document.getElementById(id).addEventListener('change', () => {
    showNoticeChoice();
    window.privacySaved = savePrivacy();
  });
}
