// Contact names in chats: the rule for each kind of name, the rename dialog and identity isolation.
const checkContactNames = smoke.uiSmokeContactNames;
const { checkPublicName, checkChatMute, checkProfileShare, checkInterests } =
  smoke.uiSmokeNamesPublic;
const {
  checkFavoriteAction,
  checkSavedProfiles,
  checkContactNotes,
  checkPassActions,
  checkProfileActionGuards,
} = smoke.uiSmokeNamesActions;
smoke.uiSmokeNames = Object.freeze({
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
