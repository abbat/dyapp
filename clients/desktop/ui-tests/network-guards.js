// Network generation smoke guards; delayed replies belong to their original identity.
(() => {
  const createFormFixture = () => {
    const readyState = current;
    const switchAndShow = async () => {
      await act(Promise.resolve({ status: 'picker' }));
      await act(Promise.resolve(readyState));
      await showNetwork((_, args) =>
        Promise.resolve(
          args.file === 'active'
            ? { seeds: ['new-active'], name: 'new' }
            : { seeds: ['new-draft'], name: 'new draft' },
        ),
      );
    };
    const checkNew = (message) => {
      if (
        seeds.value !== 'new-draft' ||
        networkName.value !== 'new draft' ||
        document.getElementById('network-status').textContent !== window.strings['network.active']
      )
        throw new Error(message);
    };
    return { switchAndShow, checkNew };
  };
  const checkOldActive = async (fixture) => {
    const { switchAndShow, checkNew } = fixture;
    let finishActive;
    let oldDraftCalls = 0;
    const oldActive = showNetwork((_, args) => {
      if (args.file === 'active')
        return new Promise((resolve) => {
          finishActive = resolve;
        });
      oldDraftCalls += 1;
      return Promise.resolve(null);
    });
    await switchAndShow();
    finishActive({ seeds: ['old'], name: 'old' });
    await oldActive;
    checkNew('old active load changed new network form');
    if (oldDraftCalls) throw new Error('old active load continued to draft');
  };
  const checkOldDraft = async (fixture) => {
    const { switchAndShow, checkNew } = fixture;
    let finishDraft;
    const oldDraft = showNetwork((_, args) =>
      args.file === 'active'
        ? Promise.resolve({ seeds: ['old'], name: 'old' })
        : new Promise((resolve) => {
            finishDraft = resolve;
          }),
    );
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
    if (!finishDraft) throw new Error('old draft load did not start');
    await switchAndShow();
    finishDraft({ seeds: ['old-draft'], name: 'old draft' });
    await oldDraft;
    checkNew('old draft load changed new network form');
  };
  const checkOldDraftSave = async (fixture) => {
    const { switchAndShow, checkNew } = fixture;
    let failSave;
    const oldSave = saveNetworkDraft(
      () =>
        new Promise((_, reject) => {
          failSave = reject;
        }),
    );
    await switchAndShow();
    failSave(new Error('old draft save'));
    await oldSave;
    checkNew('old draft-save error changed new network status');
  };
  const checkOldAndNewRequests = async (readyState) => {
    let finishOldCheck;
    let oldActivations = 0;
    const oldCheck = check((command) => {
      if (command === 'check_connection') {
        return new Promise((resolve) => {
          finishOldCheck = resolve;
        });
      }
      oldActivations += 1;
      return Promise.resolve();
    });
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    if (checkButton.disabled || !cancelButton.hidden) {
      throw new Error('new network form inherited old busy controls');
    }
    let finishNewCheck;
    const newCheck = check((command) =>
      command === 'check_connection'
        ? new Promise((resolve) => {
            finishNewCheck = resolve;
          })
        : Promise.resolve(),
    );
    finishOldCheck([{ status: 'connected' }]);
    await oldCheck;
    if (
      oldActivations ||
      !checkButton.disabled ||
      cancelButton.hidden ||
      document.getElementById('network-status').textContent !== window.strings['network.checking']
    ) {
      throw new Error('old network check changed new busy form or activated');
    }
    finishNewCheck([{ status: 'unreachable' }]);
    await newCheck;
    if (checkButton.disabled || !cancelButton.hidden) {
      throw new Error('current network check did not release controls');
    }
  };
  const checkOldActivation = async (readyState) => {
    let finishActivation;
    let oldApply = 0;
    const oldActivation = check((command) => {
      if (command === 'check_connection') return Promise.resolve([{ status: 'connected' }]);
      if (command === 'activate_connection') {
        return new Promise((resolve) => {
          finishActivation = resolve;
        });
      }
      oldApply += 1;
      return Promise.resolve();
    });
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
    if (!finishActivation) throw new Error('old network activation did not start');
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const status = document.getElementById('network-status').textContent;
    finishActivation();
    await oldActivation;
    if (
      oldApply ||
      document.getElementById('network-status').textContent !== status ||
      checkButton.disabled ||
      !cancelButton.hidden
    ) {
      throw new Error('old activation changed new network or applied node');
    }
  };
  const checkOldFailure = async (readyState) => {
    let failOldCheck;
    const oldFailure = check(
      () =>
        new Promise((_, reject) => {
          failOldCheck = reject;
        }),
    );
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const newerStatus = document.getElementById('network-status').textContent;
    failOldCheck(new Error('old check'));
    await oldFailure;
    if (
      document.getElementById('network-status').textContent !== newerStatus ||
      checkButton.disabled ||
      !cancelButton.hidden
    ) {
      throw new Error('old network check error changed new form');
    }
  };
  const form = async () => {
    const fixture = createFormFixture();
    await checkOldActive(fixture);
    await checkOldDraft(fixture);
    await checkOldDraftSave(fixture);
  };
  const requests = async () => {
    const readyState = current;
    await checkOldAndNewRequests(readyState);
    await checkOldActivation(readyState);
    await checkOldFailure(readyState);
  };
  window.uiSmokeNetworkGuards = { form, requests };
})();
