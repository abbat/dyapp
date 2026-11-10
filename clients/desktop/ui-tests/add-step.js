// Add-identity scenario phases; preserve the sequential timed smoke checks.
(() => {
  const checkSharedUi = async (state) => {
    if (state.status !== 'ready') throw new Error('identity not reopened');
    await checkSettingsInventory(state.peerId, state.mode);
    await smoke.checkStickerCollection();
    await smoke.checkStickerRemoval();
    await smoke.checkChatComposer();
    await smoke.checkStickerPicker();
    await smoke.checkStickerMessages();
    await smoke.checkChatRedraw();
    await smoke.checkStickerRequests();
    await smoke.checkClearDialog();
    await smoke.checkIdentifiers();
    await smoke.checkSettingsNavigation();
    await smoke.checkMediaCache();
    await smoke.checkSettingsLists();
  };
  const createAdditionalIdentity = async (state, mode, checkNode = async () => {}) => {
    await openArea('settings', 'identities');
    document.getElementById('settings-create').click();
    await waitFor('setup');
    await checkNode();
    await chooseStorage(mode);
    await checkNode();
    const addedPeer = document.getElementById('peer-id').textContent;
    await checkSettingsInventory(addedPeer, mode);
    await expectPrivacy('false,false,true,true,true');
    document.getElementById('send-views').click();
    await appApi.privacySaved;
    await switchFromSettings(state.mode);
    await checkNode();
    await checkSettingsInventory(state.peerId, state.mode);
    await expectPrivacy('false,false,true,true,true');
    await switchFromSettings(mode);
    await checkNode();
    await checkSettingsInventory(addedPeer, mode);
    await expectPrivacy('true,false,true,true,true');
    document.getElementById('send-views').click();
    await appApi.privacySaved;
    await openArea('profile-area');
  };
  const checkFunctionalSmoke = async (checks) => {
    const { identityChecks, chatChecks, presenceChecks, nameChecks, photoChecks } = checks;
    await identityChecks.checkStaleRender();
    await identityChecks.checkStaleSearchAndChats();
    await chatChecks.checkChatProjection();
    await chatChecks.checkFirstMessage();
    await chatChecks.checkClearChat();
    await chatChecks.checkChatBlock();
    await chatChecks.checkChatEdit();
    await chatChecks.checkChatDelete();
    await chatChecks.checkChatDelivery();
    await chatChecks.checkNewDevice();
    await chatChecks.checkChatRead();
    await presenceChecks.checkChatPresence();
    await presenceChecks.checkLikePending();
    await nameChecks.checkContactNames();
    await nameChecks.checkPublicName();
    await nameChecks.checkInterests();
    await nameChecks.checkFavoriteAction();
    await nameChecks.checkContactNotes();
    await nameChecks.checkPassActions();
    await nameChecks.checkProfileActionGuards();
    await nameChecks.checkSavedProfiles();
    await nameChecks.checkChatMute();
    await photoChecks.checkChatPhoto();
    await photoChecks.checkProfilePhotos();
    await photoChecks.checkPeerPhotos();
    await chatChecks.checkChatForwarded();
    await chatChecks.checkChatForwardAction();
    await chatChecks.checkChatLinkHosts();
    await nameChecks.checkProfileShare();
  };
  const checkStaleSmoke = async (checks) => {
    const { identityChecks, guardChecks } = checks;
    await identityChecks.checkStalePeerActions();
    await identityChecks.checkStaleSafety();
    await identityChecks.checkStaleProfileLoad();
    await identityChecks.checkStaleProfileHandlers();
    await identityChecks.checkStalePublishAndHide();
    await identityChecks.checkStaleProfileTimers();
    await identityChecks.checkStaleUsageStatus();
    await identityChecks.checkStalePrivacy();
    await identityChecks.checkStaleBlocked();
    await guardChecks.checkStaleSettingsInventory();
    await guardChecks.checkStaleNode();
    await guardChecks.checkStaleTray();
    await guardChecks.checkStaleRemovePreparation();
    await guardChecks.checkStaleWithdrawal();
    await guardChecks.checkDeviceRevoked();
    await guardChecks.checkDevices();
    await guardChecks.checkLinkCode();
    await guardChecks.checkLinkScreen();
    await guardChecks.checkLinkedNotice();
    await guardChecks.checkStaleLaunchConnection();
    await guardChecks.checkStaleNetworkForm();
    await guardChecks.checkStaleNetworkCheck();
    await guardChecks.checkStaleCopy();
    await guardChecks.checkStalePlaceSuggestions();
  };
  const checkNodeUsage = async (settings) => {
    await openArea('settings', 'node');
    await appApi.usageShown;
    const bar = document.getElementById('usage-node-bar');
    if (bar.hidden || bar.max !== settings.diskMb * 1024 ** 2 || !(bar.value > 0)) {
      throw new Error('running installation node has no usage bar');
    }
  };
  smoke.uiSmokeAddStep = async (state, mode, checks) => {
    await checkSharedUi(state);
    {
      const settings = await appBindings.invoke('load_node_settings');
      const running = await appBindings.invoke('save_node_settings', {
        settings: { ...settings, enabled: true },
      });
      if (running.status !== 'running') throw new Error('installation node did not start');
      try {
        await checkNodeUsage(settings);
        await createAdditionalIdentity(state, mode, async () => {
          const current = await appBindings.invoke('node_status');
          if (current.status !== 'running' || current.peerId !== running.peerId) {
            throw new Error('profile transition interrupted the installation node');
          }
        });
      } finally {
        await appBindings.invoke('save_node_settings', { settings });
      }
    }
    await checkFunctionalSmoke(checks);
    await checkStaleSmoke(checks);
  };
})();
