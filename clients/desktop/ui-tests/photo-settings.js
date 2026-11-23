// Photo settings smoke keeps save, cancellation and cache-budget assertions together.
(() => {
  const checkFractionalBudget = async (limit, dialog, editLimit, savedCache) => {
    const fractional = '512.00000000000000000001';
    await editLimit(fractional);
    const current = await savedCache();
    const invalid = appApi.strings['photos.cache.invalid']
      .replace('{min}', current.minMib)
      .replace('{max}', current.maxMib);
    if (
      dialog.open ||
      limit.value !== fractional ||
      document.getElementById('media-cache-status').textContent !== invalid ||
      current.limitBytes !== 512 * 1048576
    )
      throw new Error('Fractional-byte input was rounded or changed the saved budget');
  };
  const checkCacheBudget = async () => {
    await appApi.mediaCacheShown;
    const limit = document.getElementById('media-cache-limit');
    const dialog = document.getElementById('photo-keep-dialog');
    const savedCache = () =>
      appBindings.invoke('media_cache', { identityId: appBindings.current.peerId });
    const editLimit = (value) => {
      limit.value = value;
      if (limit.value !== value)
        throw new Error('Native number input changed the raw MiB test value');
      limit.dispatchEvent(new Event('change', { bubbles: true }));
      return smoke.waitForSignal('native cache budget admission', appApi.mediaCacheSaved);
    };
    if (limit.value !== '256') throw new Error('cache did not default to 256 MiB');
    await editLimit('512');
    await appApi.mediaCacheSaved;
    if (dialog.open || (await savedCache()).limitBytes !== 512 * 1048576) {
      throw new Error('custom cache budget was not saved at once');
    }
    await checkFractionalBudget(limit, dialog, editLimit, savedCache);
    await editLimit('256');
    if (!dialog.open) throw new Error('lower cache budget did not ask');
    const canceled = closed(dialog);
    document.getElementById('photo-keep-cancel').click();
    await smoke.waitForSignal('native cache cancellation close', canceled);
    if (limit.value !== '512' || (await savedCache()).limitBytes !== 512 * 1048576) {
      throw new Error('Cancel changed cache budget');
    }
    await editLimit('256');
    const confirmed = closed(dialog);
    document.getElementById('photo-keep-confirm').click();
    await smoke.waitForSignal('native cache confirmation close', confirmed);
    await smoke.waitForSignal('native confirmed cache budget', appApi.mediaCacheSaved);
    if (limit.value !== '256' || (await savedCache()).limitBytes !== 256 * 1048576) {
      throw new Error('lower cache budget was not saved');
    }
  };
  smoke.uiSmokePhotoSettings = async () => {
    await openArea('settings', 'photos');
    await appApi.photoModeShown;
    const mode = (value) => document.querySelector(`input[name="photo-mode"][value="${value}"]`);
    if (!mode('byNetwork').checked) throw new Error('the photo mode did not default to by network');
    mode('onTap').click();
    await appApi.photoModeSaved;
    if ((await appBindings.invoke('auto_download_mode')) !== 'onTap')
      throw new Error('the photo mode was not saved');
    await smoke.shot('settings-photos');
    mode('byNetwork').click();
    await appApi.photoModeSaved;

    return checkCacheBudget();
  };
})();
