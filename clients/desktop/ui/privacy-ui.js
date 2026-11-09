import { byId as domById } from './dom.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
export const controller = {};

// Settings → Privacy: stored per identity on this device, saved on every change.
const privacyInputs = {
  sendViews: 'send-views',
  sendReadReceipts: 'send-read-receipts',
  sendTyping: 'send-typing',
  presenceMatches: 'presence-matches',
  notifications: 'message-notifications',
};
const privacyError = domById('privacy-error', HTMLParagraphElement);
const noticeContent = domById('notice-content', HTMLSelectElement);
const privacyFailure = (failed) => {
  privacyError.hidden = !failed;
  domById('notifications-error', HTMLParagraphElement).hidden = !failed;
};
const showNoticeChoice = () => {
  noticeContent.disabled = !domById('message-notifications', HTMLInputElement).checked;
};
controller.showPrivacy = async (call = invoke) => {
  const generation = identityState.generation;
  try {
    const privacy = await call('privacy_settings');
    if (!identityCurrent(generation)) return;
    for (const [key, id] of Object.entries(privacyInputs)) {
      domById(id, HTMLInputElement).checked = privacy[key];
    }
    domById('presence-nobody', HTMLInputElement).checked = !privacy.presenceMatches;
    noticeContent.value = privacy.noticeContent;
    showNoticeChoice();
    privacyFailure(false);
  } catch (_) {
    if (identityCurrent(generation)) privacyFailure(true);
  }
};
const savePrivacy = (call = invoke) => {
  const generation = identityState.generation;
  const privacy = {
    ...Object.fromEntries(
      Object.entries(privacyInputs).map(([key, id]) => [
        key,
        domById(id, HTMLInputElement).checked,
      ]),
    ),
    noticeContent: noticeContent.value,
  };
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
  domById(id, HTMLElement).addEventListener('change', () => {
    showNoticeChoice();
    controller.privacySaved = savePrivacy();
  });
}

export { privacyError, savePrivacy };
