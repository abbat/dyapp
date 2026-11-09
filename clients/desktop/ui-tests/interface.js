const appModule = await import(new URL('app.js', document.baseURI).href);
const chatMessageModule = await import(new URL('chat-message.js', document.baseURI).href);
const chatStateModule = await import(new URL('chat-state.js', document.baseURI).href);
const chatsPhotoSendModule = await import(new URL('chats-photo-send.js', document.baseURI).href);
const chatsShareModule = await import(new URL('chats-share.js', document.baseURI).href);
const chatsUiModule = await import(new URL('chats-ui.js', document.baseURI).href);
const clipboardModule = await import(new URL('clipboard.js', document.baseURI).href);
const contactNotesModule = await import(new URL('contact-notes.js', document.baseURI).href);
const devicesModule = await import(new URL('devices.js', document.baseURI).href);
const elementsModule = await import(new URL('elements.js', document.baseURI).href);
const favoritesModule = await import(new URL('favorites.js', document.baseURI).href);
const formattingModule = await import(new URL('formatting.js', document.baseURI).href);
const identityStateModule = await import(new URL('identity-state.js', document.baseURI).href);
const ipcModule = await import(new URL('ipc.js', document.baseURI).href);
const linkModule = await import(new URL('link.js', document.baseURI).href);
const localeModule = await import(new URL('locale.js', document.baseURI).href);
const mediaCacheModule = await import(new URL('media-cache.js', document.baseURI).href);
const networkModule = await import(new URL('network.js', document.baseURI).href);
const peerLabelsModule = await import(new URL('peer-labels.js', document.baseURI).href);
const peerNamesModule = await import(new URL('peer-names.js', document.baseURI).href);
const peerPhotosModule = await import(new URL('peer-photos.js', document.baseURI).href);
const photoSettingsModule = await import(new URL('photo-settings.js', document.baseURI).href);
const placesModule = await import(new URL('places.js', document.baseURI).href);
const privacyUiModule = await import(new URL('privacy-ui.js', document.baseURI).href);
const profileFieldsModule = await import(new URL('profile-fields.js', document.baseURI).href);
const profileFormModule = await import(new URL('profile-form.js', document.baseURI).href);
const profileInterestsModule = await import(new URL('profile-interests.js', document.baseURI).href);
const profilePhotosModule = await import(new URL('profile-photos.js', document.baseURI).href);
const profileSeekingModule = await import(new URL('profile-seeking.js', document.baseURI).href);
const removeModule = await import(new URL('remove.js', document.baseURI).href);
const savedProfilesModule = await import(new URL('saved-profiles.js', document.baseURI).href);
const searchModule = await import(new URL('search.js', document.baseURI).href);
const settingsCollectionsModule = await import(
  new URL('settings-collections.js', document.baseURI).href
);
const settingsNavModule = await import(new URL('settings-nav.js', document.baseURI).href);
const settingsModule = await import(new URL('settings.js', document.baseURI).href);
const stickerMessagesModule = await import(new URL('sticker-messages.js', document.baseURI).href);
const stickerPickerModule = await import(new URL('sticker-picker.js', document.baseURI).href);
const stickerPixelsModule = await import(new URL('sticker-pixels.js', document.baseURI).href);
const stickerRequestsModule = await import(new URL('sticker-requests.js', document.baseURI).href);
const stickersModule = await import(new URL('stickers.js', document.baseURI).href);
const progressModule = await import(new URL('ui-progress.js', document.baseURI).href);
// biome-ignore lint/correctness/noUnusedVariables: shared by the concatenated native smoke scripts
const smoke = {};
// biome-ignore lint/correctness/noUnusedVariables: shared by the concatenated native smoke scripts
const appBindings = {
  get acknowledgeDevice() {
    return chatsUiModule.acknowledgeDevice;
  },
  get act() {
    return appModule.act;
  },
  get activeChat() {
    return chatStateModule.chatState.activeChat;
  },
  set activeChat(value) {
    chatStateModule.chatState.activeChat = value;
  },
  get addProfilePhoto() {
    return profilePhotosModule.addProfilePhoto;
  },
  get blockChat() {
    return chatsUiModule.blockChat;
  },
  get blockViewed() {
    return searchModule.blockViewed;
  },
  get cameraPending() {
    return searchModule.cameraPending;
  },
  get cameraStream() {
    return searchModule.cameraState.stream;
  },
  set cameraStream(value) {
    searchModule.cameraState.stream = value;
  },
  get cancelButton() {
    return networkModule.cancelButton;
  },
  get chatComposeStatus() {
    return elementsModule.chatComposeStatus;
  },
  get chatDetail() {
    return elementsModule.chatDetail;
  },
  get chatInput() {
    return elementsModule.chatInput;
  },
  get chatLinkHosts() {
    return chatsUiModule.chatLinkHosts;
  },
  get chatList() {
    return elementsModule.chatList;
  },
  get chatMessages() {
    return elementsModule.chatMessages;
  },
  get chatOverview() {
    return elementsModule.chatOverview;
  },
  get chatRequests() {
    return elementsModule.chatRequests;
  },
  get chatSend() {
    return elementsModule.chatSend;
  },
  get check() {
    return networkModule.check;
  },
  get checkButton() {
    return networkModule.checkButton;
  },
  get checkCode() {
    return devicesModule.checkCode;
  },
  get clearCancel() {
    return elementsModule.clearCancel;
  },
  get clearChat() {
    return chatsUiModule.clearChat;
  },
  get clearDialog() {
    return elementsModule.clearDialog;
  },
  get clearError() {
    return elementsModule.clearError;
  },
  get clearSearch() {
    return searchModule.clearSearch;
  },
  get closeChat() {
    return chatsUiModule.closeChat;
  },
  get closeClear() {
    return chatsUiModule.closeClear;
  },
  get closeDelete() {
    return chatsUiModule.closeDelete;
  },
  get closeFound() {
    return searchModule.closeFound;
  },
  get codeField() {
    return devicesModule.codeField;
  },
  get confirmContact() {
    return searchModule.confirmContact;
  },
  get confirmPublish() {
    return appModule.confirmPublish;
  },
  get connectAtLaunch() {
    return networkModule.connectAtLaunch;
  },
  get connectionStatus() {
    return networkModule.connectionStatus;
  },
  get copySafety() {
    return searchModule.copySafety;
  },
  get current() {
    return identityStateModule.identityState.current;
  },
  get deleteCancel() {
    return elementsModule.deleteCancel;
  },
  get deleteChatText() {
    return chatsUiModule.deleteChatText;
  },
  get deleteDialog() {
    return elementsModule.deleteDialog;
  },
  get deleteError() {
    return elementsModule.deleteError;
  },
  get disconnectNow() {
    return devicesModule.disconnectNow;
  },
  get editCancel() {
    return elementsModule.editCancel;
  },
  get editChatText() {
    return chatsUiModule.editChatText;
  },
  get editDialog() {
    return elementsModule.editDialog;
  },
  get editError() {
    return elementsModule.editError;
  },
  get editInput() {
    return elementsModule.editInput;
  },
  get editSave() {
    return elementsModule.editSave;
  },
  get elapsed() {
    return appModule.elapsed;
  },
  get forwardCancel() {
    return elementsModule.forwardCancel;
  },
  get forwardChatText() {
    return chatsUiModule.forwardChatText;
  },
  get forwardConfirm() {
    return elementsModule.forwardConfirm;
  },
  get forwardDialog() {
    return elementsModule.forwardDialog;
  },
  get forwardError() {
    return elementsModule.forwardError;
  },
  get forwardSelect() {
    return elementsModule.forwardSelect;
  },
  get hideProfile() {
    return appModule.hideProfile;
  },
  get identity() {
    return appModule.identity;
  },
  get identityGeneration() {
    return identityStateModule.identityState.generation;
  },
  set identityGeneration(value) {
    identityStateModule.identityState.generation = value;
  },
  get invoke() {
    return ipcModule.invoke;
  },
  get knownPeers() {
    return peerLabelsModule.knownPeers;
  },
  get linkJoined() {
    return linkModule.linkJoined;
  },
  get linkRun() {
    return linkModule.linkRun;
  },
  get linkTimer() {
    return linkModule.linkTimer;
  },
  get loadSafety() {
    return searchModule.loadSafety;
  },
  get muteChat() {
    return chatsUiModule.muteChat;
  },
  get networkName() {
    return networkModule.networkName;
  },
  get openChat() {
    return chatsUiModule.openChat;
  },
  get openDelete() {
    return chatsUiModule.openDelete;
  },
  get openFound() {
    return searchModule.openFound;
  },
  get openPublishDialog() {
    return appModule.openPublishDialog;
  },
  get openShare() {
    return chatsShareModule.openShare;
  },
  get peerNames() {
    return peerNamesModule.peerNames;
  },
  get placeInput() {
    return appModule.placeInput;
  },
  get placePicker() {
    return placesModule.placePicker;
  },
  get pollLink() {
    return linkModule.pollLink;
  },
  get previewAndPublish() {
    return appModule.previewAndPublish;
  },
  get privacyError() {
    return privacyUiModule.privacyError;
  },
  get profileAttested() {
    return appModule.profileAttested;
  },
  get profileBusy() {
    return appModule.profileBusy;
  },
  get profileDisclosure() {
    return appModule.profileDisclosure;
  },
  get profileDraft() {
    return appModule.profileDraft;
  },
  get profileFailure() {
    return appModule.profileFailure;
  },
  get profileHidden() {
    return appModule.profileHidden;
  },
  get profileStatus() {
    return appModule.profileStatus;
  },
  get profileView() {
    return searchModule.profileView;
  },
  get publicFields() {
    return appModule.publicFields;
  },
  get publishConfirm() {
    return appModule.publishConfirm;
  },
  get publishDialog() {
    return appModule.publishDialog;
  },
  get removeAnyway() {
    return removeModule.removeAnyway;
  },
  get removeAsk() {
    return removeModule.removeAsk;
  },
  get removeDialog() {
    return elementsModule.removeDialog;
  },
  get removeMark() {
    return searchModule.removeMark;
  },
  get removeOffline() {
    return removeModule.removeOffline;
  },
  get removePublished() {
    return removeModule.removalState.published;
  },
  set removePublished(value) {
    removeModule.removalState.published = value;
  },
  get removeRetry() {
    return removeModule.removeRetry;
  },
  get renderChatRows() {
    return chatsUiModule.renderChatRows;
  },
  get resultCard() {
    return searchModule.resultCard;
  },
  get retryRevocation() {
    return settingsModule.retryRevocation;
  },
  get revokedDialog() {
    return removeModule.revokedDialog;
  },
  get runSearch() {
    return searchModule.runSearch;
  },
  get saveNetworkDraft() {
    return networkModule.saveNetworkDraft;
  },
  get saveNodeSettings() {
    return appModule.saveNodeSettings;
  },
  get savePrivacy() {
    return privacyUiModule.savePrivacy;
  },
  get saveProfile() {
    return appModule.saveProfile;
  },
  get searchBusy() {
    return searchModule.searchBusy;
  },
  get searchQuery() {
    return searchModule.searchQuery;
  },
  get searchResults() {
    return searchModule.searchResults;
  },
  get seeds() {
    return networkModule.seeds;
  },
  get sendChatPhoto() {
    return chatsPhotoSendModule.sendChatPhoto;
  },
  get sendChatText() {
    return chatsUiModule.sendChatText;
  },
  get sendLike() {
    return searchModule.sendLike;
  },
  get sendLink() {
    return devicesModule.sendLink;
  },
  get sendPass() {
    return searchModule.sendPass;
  },
  get sendSearchProfile() {
    return appModule.sendSearchProfile;
  },
  get setProfileBusy() {
    return appModule.setProfileBusy;
  },
  get setTray() {
    return appModule.setTray;
  },
  get shareProfile() {
    return chatsShareModule.controller.shareProfile;
  },
  get shortPeerId() {
    return peerLabelsModule.shortPeerId;
  },
  get show() {
    return appModule.show;
  },
  get showArea() {
    return appModule.showArea;
  },
  get showBlocked() {
    return settingsModule.showBlocked;
  },
  get showDevices() {
    return devicesModule.showDevices;
  },
  get showNetwork() {
    return networkModule.showNetwork;
  },
  get showNode() {
    return appModule.controller.showNode;
  },
  get showPrivacy() {
    return privacyUiModule.controller.showPrivacy;
  },
  get showProfile() {
    return appModule.showProfile;
  },
  get showRemove() {
    return removeModule.showRemove;
  },
  get showSearchFilters() {
    return searchModule.showSearchFilters;
  },
  get showSearchWait() {
    return appModule.showSearchWait;
  },
  get showSettingsIdentities() {
    return settingsModule.showSettingsIdentities;
  },
  get showStatus() {
    return appModule.showStatus;
  },
  get showUsage() {
    return appModule.controller.showUsage;
  },
  get sizeText() {
    return formattingModule.sizeText;
  },
  get startElapsed() {
    return appModule.startElapsed;
  },
  get startLink() {
    return linkModule.startLink;
  },
  get stopLink() {
    return linkModule.stopLink;
  },
  get submitProfile() {
    return appModule.submitProfile;
  },
  get unblockPeer() {
    return settingsModule.unblockPeer;
  },
  get updateComposer() {
    return chatsUiModule.updateComposer;
  },
  get viewed() {
    return searchModule.viewed;
  },
  get viewStatus() {
    return searchModule.viewStatus;
  },
  get withdrawThenRemove() {
    return removeModule.withdrawThenRemove;
  },
  get wizardNetwork() {
    return appModule.wizardNetwork;
  },
  get wizardStep() {
    return networkModule.wizardStep;
  },
};
// biome-ignore lint/correctness/noUnusedVariables: shared by the concatenated native smoke scripts
const appApi = {
  get addKnownPeers() {
    return peerLabelsModule.addKnownPeers;
  },
  get announcePresence() {
    return chatsUiModule.controller.announcePresence;
  },
  set announcePresence(value) {
    chatsUiModule.controller.announcePresence = value;
  },
  get blockedShown() {
    return settingsModule.controller.blockedShown;
  },
  get cameraDone() {
    return searchModule.controller.cameraDone;
  },
  get chatBlocked() {
    return chatsUiModule.controller.chatBlocked;
  },
  get chatCleared() {
    return chatsUiModule.controller.chatCleared;
  },
  get chatDeleted() {
    return chatsUiModule.controller.chatDeleted;
  },
  get chatEdited() {
    return chatsUiModule.controller.chatEdited;
  },
  get chatMessage() {
    return chatMessageModule.controller.chatMessage;
  },
  get chatOpened() {
    return chatsUiModule.controller.chatOpened;
  },
  get chatPhotosShown() {
    return chatsUiModule.controller.chatPhotosShown;
  },
  get chatRead() {
    return chatsUiModule.controller.chatRead;
  },
  get chatsDone() {
    return progressModule.progress.chatsDone;
  },
  set chatsDone(value) {
    progressModule.progress.chatsDone = value;
  },
  get chatSent() {
    return chatsUiModule.controller.chatSent;
  },
  get codeChecked() {
    return devicesModule.controller.codeChecked;
  },
  get confirmDone() {
    return searchModule.controller.confirmDone;
  },
  get connectionChecked() {
    return networkModule.controller.connectionChecked;
  },
  get connectionShown() {
    return progressModule.progress.connectionShown;
  },
  get copied() {
    return clipboardModule.controller.copied;
  },
  get deviceRevoked() {
    return removeModule.controller.deviceRevoked;
  },
  get devicesShown() {
    return progressModule.progress.devicesShown;
  },
  get disconnectDone() {
    return devicesModule.controller.disconnectDone;
  },
  get favoriteShown() {
    return searchModule.controller.favoriteShown;
  },
  get identityShown() {
    return appModule.controller.identityShown;
  },
  get knowPeers() {
    return peerLabelsModule.knowPeers;
  },
  get likeDone() {
    return searchModule.controller.likeDone;
  },
  get linkSent() {
    return devicesModule.controller.linkSent;
  },
  get locale() {
    return localeModule.locale;
  },
  get mailUpdated() {
    return chatsUiModule.controller.mailUpdated;
  },
  set mailUpdated(value) {
    chatsUiModule.controller.mailUpdated = value;
  },
  get mediaCacheSaved() {
    return mediaCacheModule.controller.mediaCacheSaved;
  },
  get mediaCacheShown() {
    return mediaCacheModule.controller.mediaCacheShown;
  },
  get nodeApplied() {
    return appModule.controller.nodeApplied;
  },
  set nodeApplied(value) {
    appModule.controller.nodeApplied = value;
  },
  get nodeShown() {
    return settingsModule.controller.nodeShown;
  },
  get noteShown() {
    return progressModule.progress.noteShown;
  },
  get openSettingsPane() {
    return settingsNavModule.controller.openSettingsPane;
  },
  get ownAvatarsShown() {
    return profilePhotosModule.controller.ownAvatarsShown;
  },
  get passDone() {
    return searchModule.controller.passDone;
  },
  get passedShown() {
    return progressModule.progress.passedShown;
  },
  get peerName() {
    return peerNamesModule.controller.peerName;
  },
  get peerPhotoChosen() {
    return peerPhotosModule.controller.peerPhotoChosen;
  },
  get peerPhotoLoaded() {
    return peerPhotosModule.controller.peerPhotoLoaded;
  },
  get peerPhotosShown() {
    return searchModule.controller.peerPhotosShown;
  },
  get photoModeSaved() {
    return photoSettingsModule.controller.photoModeSaved;
  },
  get photoModeShown() {
    return photoSettingsModule.controller.photoModeShown;
  },
  get placesShown() {
    return placesModule.controller.placesShown;
  },
  get privacySaved() {
    return privacyUiModule.controller.privacySaved;
  },
  get privacyShown() {
    return settingsModule.controller.privacyShown;
  },
  get profileFields() {
    return profileFieldsModule;
  },
  get profileForm() {
    return profileFormModule.profileForm;
  },
  get profileFormValid() {
    return profileSeekingModule.profileFormValid;
  },
  get profileInterestTags() {
    return profileInterestsModule.profileInterestTags;
  },
  get profilePhotoChanged() {
    return profilePhotosModule.controller.profilePhotoChanged;
  },
  get profileShared() {
    return chatsUiModule.controller.profileShared;
  },
  get pseudoLocale() {
    return localeModule.pseudoLocale;
  },
  get removeDone() {
    return removeModule.controller.removeDone;
  },
  get renderSettingsRows() {
    return settingsCollectionsModule.controller.renderSettingsRows;
  },
  get revocationRetried() {
    return settingsModule.controller.revocationRetried;
  },
  get safetyClearDone() {
    return searchModule.controller.safetyClearDone;
  },
  get safetyCopied() {
    return searchModule.controller.safetyCopied;
  },
  get safetyDone() {
    return searchModule.controller.safetyDone;
  },
  get saveContactNote() {
    return contactNotesModule.controller.saveContactNote;
  },
  get savedOpened() {
    return savedProfilesModule.controller.savedOpened;
  },
  get savedRemoved() {
    return savedProfilesModule.controller.savedRemoved;
  },
  get savedShown() {
    return savedProfilesModule.controller.savedShown;
  },
  get scanQrFile() {
    return searchModule.controller.scanQrFile;
  },
  get searchDone() {
    return searchModule.controller.searchDone;
  },
  get seekingDraft() {
    return profileSeekingModule.seekingDraft;
  },
  get seekingFacts() {
    return profileSeekingModule.seekingFacts;
  },
  get settingsCollectionRefreshed() {
    return settingsCollectionsModule.controller.settingsCollectionRefreshed;
  },
  get settingsShown() {
    return appModule.controller.settingsShown;
  },
  get sharedOpened() {
    return chatsShareModule.controller.sharedOpened;
  },
  get shortPeer() {
    return peerLabelsModule.shortPeer;
  },
  get shortPeerId() {
    return peerLabelsModule.shortPeerId;
  },
  get showCardPhotos() {
    return peerPhotosModule.controller.showCardPhotos;
  },
  get showChats() {
    return chatsUiModule.controller.showChats;
  },
  get showContactNote() {
    return contactNotesModule.controller.showContactNote;
  },
  get showFoundPhotos() {
    return peerPhotosModule.controller.showFoundPhotos;
  },
  get showMediaCache() {
    return mediaCacheModule.controller.showMediaCache;
  },
  get showPrivacy() {
    return privacyUiModule.controller.showPrivacy;
  },
  get showProfileInterests() {
    return profileInterestsModule.showProfileInterests;
  },
  get showProfilePhotos() {
    return profilePhotosModule.controller.showProfilePhotos;
  },
  get showSaved() {
    return savedProfilesModule.controller.showSaved;
  },
  get showSeeking() {
    return profileSeekingModule.showSeeking;
  },
  get statusShown() {
    return appModule.controller.statusShown;
  },
  set statusShown(value) {
    appModule.controller.statusShown = value;
  },
  get stickerBinaryPixels() {
    return stickerPixelsModule.stickerBinaryPixels;
  },
  get stickerCollection() {
    return stickersModule.controller.stickerCollection;
  },
  get stickerImageLoaded() {
    return stickerMessagesModule.controller.stickerImageLoaded;
  },
  get stickerImagesLoaded() {
    return stickerMessagesModule.controller.stickerImagesLoaded;
  },
  get stickerImported() {
    return stickersModule.controller.stickerImported;
  },
  get stickerMessages() {
    return stickerMessagesModule.controller.stickerMessages;
  },
  get stickerMessageText() {
    return stickersModule.controller.stickerMessageText;
  },
  get stickerPicker() {
    return stickerPickerModule.controller.stickerPicker;
  },
  get stickerPixels() {
    return stickerPixelsModule.stickerPixels;
  },
  get stickerRemoved() {
    return stickersModule.controller.stickerRemoved;
  },
  get stickerRequestChanged() {
    return stickerRequestsModule.controller.stickerRequestChanged;
  },
  get stickerRequestDelivered() {
    return stickerRequestsModule.controller.stickerRequestDelivered;
  },
  get stickerRequests() {
    return stickerRequestsModule.controller.stickerRequests;
  },
  get stickerRequestsUpdated() {
    return stickerRequestsModule.controller.stickerRequestsUpdated;
  },
  get stickerSent() {
    return stickerPickerModule.controller.stickerSent;
  },
  get stickersShown() {
    return progressModule.progress.stickersShown;
  },
  get strings() {
    return localeModule.strings;
  },
  get toggleFavorite() {
    return favoritesModule.controller.toggleFavorite;
  },
  get trayShown() {
    return progressModule.progress.trayShown;
  },
  get uiSmokeRename() {
    return peerNamesModule.controller.uiSmokeRename;
  },
  get usageShown() {
    return settingsModule.controller.usageShown;
  },
};
