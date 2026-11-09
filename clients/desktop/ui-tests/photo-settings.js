// Photo settings smoke keeps save, cancellation and cache-budget assertions together.
(() => {
  const checkCacheBudget = async () => {
    await appApi.mediaCacheShown;
    const limit = document.getElementById('media-cache-limit');
    const dialog = document.getElementById('photo-keep-dialog');
    const savedCache = () =>
      appBindings.invoke('media_cache', { identityId: appBindings.current.peerId });
    const editLimit = (value) => {
      limit.value = value;
      limit.dispatchEvent(new Event('change', { bubbles: true }));
    };
    if (limit.value !== '256') throw new Error('cache did not default to 256 MiB');
    editLimit('512');
    await appApi.mediaCacheSaved;
    if (dialog.open || (await savedCache()).limitBytes !== 512 * 1048576) {
      throw new Error('custom cache budget was not saved at once');
    }
    editLimit('256');
    if (!dialog.open) throw new Error('lower cache budget did not ask');
    document.getElementById('photo-keep-cancel').click();
    if (limit.value !== '512' || (await savedCache()).limitBytes !== 512 * 1048576) {
      throw new Error('Cancel changed cache budget');
    }
    editLimit('256');
    document.getElementById('photo-keep-confirm').click();
    await appApi.mediaCacheSaved;
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
