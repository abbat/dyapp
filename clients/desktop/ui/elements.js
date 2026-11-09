import { byId as domById } from './dom.js';
const removeIdentity = domById('remove-identity', HTMLButtonElement);
const removeDialog = domById('remove', HTMLDialogElement);
const settingsRemove = domById('settings-remove', HTMLButtonElement);
const likedButton = domById('chats-liked', HTMLButtonElement);
const likedList = domById('chats-liked-list', HTMLDivElement);
const chatOverview = domById('chats-overview', HTMLDivElement);
const chatDetail = domById('chat-detail', HTMLDivElement);
const chatRequests = domById('chats-requests', HTMLDivElement);
const chatList = domById('chat-list', HTMLDivElement);
const chatMessages = domById('chat-messages', HTMLDivElement);
const chatInput = domById('chat-input', HTMLTextAreaElement);
const chatSend = domById('chat-send', HTMLButtonElement);
const chatComposeStatus = domById('chat-compose-status', HTMLParagraphElement);
const newDeviceNotice = domById('chat-new-device-notice', HTMLDivElement);
const clearDialog = domById('chat-clear-dialog', HTMLDialogElement);
const clearError = domById('chat-clear-error', HTMLParagraphElement);
const clearCancel = domById('chat-clear-cancel', HTMLButtonElement);
const clearConfirm = domById('chat-clear-confirm', HTMLButtonElement);
const editDialog = domById('chat-edit-dialog', HTMLDialogElement);
const editInput = domById('chat-edit-input', HTMLTextAreaElement);
const editError = domById('chat-edit-error', HTMLParagraphElement);
const editCancel = domById('chat-edit-cancel', HTMLButtonElement);
const editSave = domById('chat-edit-save', HTMLButtonElement);
const deleteDialog = domById('chat-delete-dialog', HTMLDialogElement);
const deleteError = domById('chat-delete-error', HTMLParagraphElement);
const deleteCancel = domById('chat-delete-cancel', HTMLButtonElement);
const deleteConfirm = domById('chat-delete-confirm', HTMLButtonElement);
const forwardDialog = domById('chat-forward-dialog', HTMLDialogElement);
const forwardSelect = domById('chat-forward-peer', HTMLSelectElement);
const forwardError = domById('chat-forward-error', HTMLParagraphElement);
const forwardCancel = domById('chat-forward-cancel', HTMLButtonElement);
const forwardConfirm = domById('chat-forward-confirm', HTMLButtonElement);

export {
  chatComposeStatus,
  chatDetail,
  chatInput,
  chatList,
  chatMessages,
  chatOverview,
  chatRequests,
  chatSend,
  clearCancel,
  clearConfirm,
  clearDialog,
  clearError,
  deleteCancel,
  deleteConfirm,
  deleteDialog,
  deleteError,
  editCancel,
  editDialog,
  editError,
  editInput,
  editSave,
  forwardCancel,
  forwardConfirm,
  forwardDialog,
  forwardError,
  forwardSelect,
  likedButton,
  likedList,
  newDeviceNotice,
  removeDialog,
  removeIdentity,
  settingsRemove,
};
