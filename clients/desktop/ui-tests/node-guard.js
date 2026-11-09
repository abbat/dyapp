// Node Settings waits and save completions stay bound to the identity that started them.
(() => {
  const createNodeFixture = () => {
    const readyState = appBindings.current;
    const node = document.getElementById('node');
    const save = document.getElementById('node-save');
    const saved = { enabled: false, diskMb: 40, maxConnections: 2 };
    const switchAndShow = async () => {
      await appBindings.act(Promise.resolve({ status: 'picker' }));
      await appBindings.act(Promise.resolve(readyState));
      await openArea('settings', 'node');
      await appApi.nodeShown;
      await appBindings.showNode((command) =>
        Promise.resolve(command === 'node_status' ? { status: 'off', peerId: '' } : saved),
      );
    };
    const checkNew = (message) => {
      if (
        node.hidden ||
        node.dataset.status !== 'off' ||
        document.getElementById('node-enabled').checked ||
        document.getElementById('node-disk').value !== '40' ||
        document.getElementById('node-connections').value !== '2'
      )
        throw new Error(message);
    };
    return { node, save, saved, switchAndShow, checkNew };
  };
  const checkOldNodeStatus = async (fixture) => {
    const { switchAndShow, checkNew } = fixture;
    await openArea('settings', 'node');
    let finishStatus;
    const oldStatus = appBindings.showNode(
      () =>
        new Promise((resolve) => {
          finishStatus = resolve;
        }),
    );
    await switchAndShow();
    finishStatus({ status: 'running', peerId: 'old' });
    await oldStatus;
    checkNew('old node status changed new form');
  };
  const checkOldNodeSettings = async (fixture) => {
    const { switchAndShow, checkNew } = fixture;
    let finishSettings;
    const oldSettings = appBindings.showNode((command) =>
      command === 'node_status'
        ? Promise.resolve({ status: 'off', peerId: '' })
        : new Promise((resolve) => {
            finishSettings = resolve;
          }),
    );
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
    if (!finishSettings) throw new Error('old node settings load did not start');
    await switchAndShow();
    finishSettings({ enabled: true, diskMb: 100, maxConnections: 10 });
    await oldSettings;
    checkNew('old node settings changed new form');
  };
  const checkOldNodeSettingsFailure = async (fixture) => {
    const { switchAndShow, checkNew } = fixture;
    let failSettings;
    const oldSettingsFailure = appBindings.showNode((command) =>
      command === 'node_status'
        ? Promise.resolve({ status: 'off', peerId: '' })
        : new Promise((_, reject) => {
            failSettings = reject;
          }),
    );
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
    await switchAndShow();
    failSettings(new Error('old settings'));
    await oldSettingsFailure;
    checkNew('old node settings error changed new form');
  };
  const checkOldNodeSave = async (fixture) => {
    const { saved, switchAndShow, save, node } = fixture;
    let finishOldSave;
    const oldSave = appBindings.saveNodeSettings(
      saved,
      () =>
        new Promise((resolve) => {
          finishOldSave = resolve;
        }),
    );
    await switchAndShow();
    let finishNewSave;
    const newSave = appBindings.saveNodeSettings(
      { ...saved, enabled: true },
      () =>
        new Promise((resolve) => {
          finishNewSave = resolve;
        }),
    );
    finishOldSave({ status: 'running', peerId: 'old' });
    await oldSave;
    if (!save.disabled || node.dataset.status !== 'starting') {
      throw new Error('old node save changed new status or unlocked Save');
    }
    finishNewSave({ status: 'off', peerId: '' });
    await newSave;
    if (save.disabled || node.dataset.status !== 'off')
      throw new Error('new node save did not finish');
  };
  const checkOldNodeSaveFailure = async (fixture) => {
    const { saved, switchAndShow, checkNew } = fixture;
    let failOldSave;
    const oldSaveFailure = appBindings.saveNodeSettings(
      saved,
      () =>
        new Promise((_, reject) => {
          failOldSave = reject;
        }),
    );
    await switchAndShow();
    failOldSave(new Error('old save'));
    await oldSaveFailure;
    checkNew('old node save error changed new status');
    await appBindings.showNode();
  };
  smoke.uiSmokeNodeGuard = async () => {
    const fixture = createNodeFixture();
    await checkOldNodeStatus(fixture);
    await checkOldNodeSettings(fixture);
    await checkOldNodeSettingsFailure(fixture);
    await checkOldNodeSave(fixture);
    await checkOldNodeSaveFailure(fixture);
  };
})();
