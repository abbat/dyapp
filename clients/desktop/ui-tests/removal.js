(() => {
  // Removal withdraws a published profile first. With the node stopped no node takes the tombstone:
  // Retry fails again and Cancel keeps the identity.
  const removeOfflineIdentity = async () => {
    await openArea('settings', 'identities');
    byId('settings-remove').click();
    await waitFor('remove');
    if (!shown('remove-text') || !shown('remove-ask') || shown('remove-offline')) {
      throw new Error('removal does not say the profile is withdrawn first');
    }
    byId('remove-confirm').click();
    byId('remove-action').click();
    await appApi.removeDone;
    if (
      !shown('remove-offline') ||
      shown('remove-ask') ||
      document.activeElement.id !== 'remove-cancel-offline'
    ) {
      throw new Error('no offline choice when the tombstone failed');
    }
    await smoke.shot('remove-offline');
    byId('remove-retry').click();
    await appApi.removeDone;
    if (!shown('remove-offline') || byId('remove-retry').disabled) {
      throw new Error('Retry did not offer the offline choice again');
    }
    byId('remove-cancel-offline').click();
    if (byId('remove').open || !shown('settings')) throw new Error('Cancel did not keep Settings');
  };
  // With the node back the tombstone goes first, then the key; a new identity opens for the checks.
  const removeWithTombstone = async (mode) => {
    await openArea('settings', 'identities');
    byId('settings-remove').click();
    await waitFor('remove');
    byId('remove-confirm').click();
    byId('remove-action').click();
    const state = await appApi.removeDone;
    if (shown('remove-offline')) throw new Error('removal stopped although a node answered');
    if (state?.status !== 'setup') {
      const remaining = await invokeTest('list_identities');
      throw new Error(
        `removal returned ${state?.status ?? 'none'} (${remaining.length} identities remain)`,
      );
    }
    await waitFor('setup');
    await chooseStorage(mode);
  };
  smoke.identityRemovalChecks = { offline: removeOfflineIdentity, tombstone: removeWithTombstone };
})();
