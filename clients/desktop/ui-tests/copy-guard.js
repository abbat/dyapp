// Delayed clipboard outcomes and label reset callbacks retain their original identity.
(() => {
  const createCopyFixture = () => {
    const readyState = current;
    const source = document.getElementById('peer-id');
    const button = document.getElementById('copy-peer-id');
    const clipboard = navigator.clipboard;
    if (!clipboard) throw new Error('no clipboard API for stale copy check');
    const switchAndMark = async () => {
      await act(Promise.resolve({ status: 'picker' }));
      await act(Promise.resolve(readyState));
      source.textContent = 'b'.repeat(64);
      button.textContent = 'new copy';
      window.getSelection().removeAllRanges();
    };
    let closed = false;
    const close = () => {
      if (closed) return;
      closed = true;
      delete clipboard.writeText;
      source.textContent = readyState.peerId;
      button.textContent = window.strings['action.copy'];
    };
    return { source, button, clipboard, switchAndMark, close };
  };
  const checkOldCopyFailure = async (fixture) => {
    try {
      const { source, button, clipboard, switchAndMark } = fixture;
      let rejectOld;
      clipboard.writeText = () =>
        new Promise((_, reject) => {
          rejectOld = reject;
        });
      button.click();
      await new Promise((resolve) => {
        setTimeout(resolve, 0);
      });
      if (!rejectOld) throw new Error('old clipboard write did not start');
      const oldFailure = window.copied;
      await switchAndMark();
      rejectOld(new Error('old clipboard'));
      await oldFailure;
      if (
        source.textContent !== 'b'.repeat(64) ||
        button.textContent !== 'new copy' ||
        window.getSelection().rangeCount
      )
        throw new Error('old copy fallback changed new identity');
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkOldCopySuccess = async (fixture) => {
    try {
      const { button, clipboard, switchAndMark } = fixture;
      let finishOld;
      clipboard.writeText = () =>
        new Promise((resolve) => {
          finishOld = resolve;
        });
      button.click();
      await new Promise((resolve) => {
        setTimeout(resolve, 0);
      });
      if (!finishOld) throw new Error('old clipboard success did not start');
      const oldSuccess = window.copied;
      await switchAndMark();
      finishOld();
      await oldSuccess;
      if (button.textContent !== 'new copy') throw new Error('old copy success changed new label');
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkOldCopyTimer = async (fixture) => {
    try {
      const { button, clipboard, switchAndMark } = fixture;
      const originalTimeout = globalThis.setTimeout;
      let oldReset;
      globalThis.setTimeout = (callback, delay, ...args) => {
        if (delay === 2000) {
          oldReset = callback;
          return 0;
        }
        return originalTimeout(callback, delay, ...args);
      };
      try {
        clipboard.writeText = () => Promise.resolve();
        button.click();
        await window.copied;
      } finally {
        globalThis.setTimeout = originalTimeout;
      }
      if (!oldReset) throw new Error('copy label reset was not scheduled');
      await switchAndMark();
      oldReset();
      if (button.textContent !== 'new copy') throw new Error('old copy timer changed new label');
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  window.uiSmokeCopyGuard = async () => {
    const fixture = createCopyFixture();
    try {
      await checkOldCopyFailure(fixture);
      await checkOldCopySuccess(fixture);
      await checkOldCopyTimer(fixture);
    } finally {
      fixture.close();
    }
  };
})();
