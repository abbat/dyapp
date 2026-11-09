// Contact names in chats: the rule for each kind of name, the rename dialog and identity isolation.
const checkContactNames = window.uiSmokeContactNames;
const { checkPublicName, checkChatMute, checkProfileShare, checkInterests } =
  globalThis.uiSmokeNamesPublic;
const {
  checkFavoriteAction,
  checkSavedProfiles,
  checkContactNotes,
  checkPassActions,
  checkProfileActionGuards,
} = globalThis.uiSmokeNamesActions;
globalThis.uiSmokeNames = Object.freeze({
  checkContactNames,
  checkPublicName,
  checkChatMute,
  checkProfileShare,
  checkInterests,
  checkFavoriteAction,
  checkSavedProfiles,
  checkContactNotes,
  checkPassActions,
  checkProfileActionGuards,
});
