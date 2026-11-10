// Special native smoke routes; every result and rejection returns to the main dispatcher.
(() => {
  const video = async (step) => {
    if (step.startsWith('video-bench:'))
      return smoke.videoBenchmark(step.slice('video-bench:'.length));
    const size = step.slice('video:'.length);
    if (!['360p', '720p'].includes(size)) throw new Error('invalid video smoke size');
    const metrics = await videoFrames(size);
    return invokeTest('ui_test_result', {
      passed: true,
      peerId: '',
      reason: `video:${size}:${JSON.stringify(metrics)}`,
    });
  };
  const recovery = async (state, step) => {
    // A localized reason first; the code and slot row only in collapsed Details, with Copy.
    const reason = step.slice('recovery:'.length);
    const more = document.getElementById('recovery-more');
    const details = document.getElementById('recovery-details');
    if (
      state.status !== 'recovery' ||
      state.reason !== reason ||
      document.getElementById('recovery').hidden ||
      document.getElementById('recovery-reason').textContent !==
        appApi.strings[`recovery.${reason}`]
    )
      throw new Error(`recovery ${reason} not shown`);
    if (more.open || !details.textContent.startsWith(`${reason}\n${state.slot}`)) {
      throw new Error(`recovery details: ${more.open} ${details.textContent}`);
    }
    await smoke.shot('recovery');
    more.open = true;
    await checkCopy('copy-recovery-details', details);
    await smoke.shot('recovery-details');
    return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
  };
  const windowBounds = async (step) => {
    // save: move the window and quit with Ctrl+Q; restore: the saved bounds are back;
    // centre: an off-screen saved window lands in the middle of the primary display.
    const kind = step.slice('window:'.length);
    const [initial, primary, environment] = await invokeTest('ui_test_window');
    const saved = {
      x:
        initial.x +
        Math.floor(Math.max(0, primary.x + primary.width - initial.x - initial.width) / 4),
      y:
        initial.y +
        Math.floor(Math.max(0, primary.y + primary.height - initial.y - initial.height) / 4),
      width: Math.max(environment.minSize[0], Math.round(initial.width * 0.9)),
      height: Math.max(environment.minSize[1], Math.round(initial.height * 0.9)),
    };
    if (kind === 'save') {
      const [at] = await invokeTest('ui_test_window', { set: saved, remember: true });
      if (JSON.stringify(at) !== JSON.stringify(saved))
        throw new Error(`window not moved: ${JSON.stringify(at)}`);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'q', ctrlKey: true }));
      return undefined;
    }
    const at = initial;
    const centred = {
      x: primary.x + Math.floor((primary.width - at.width) / 2),
      y: primary.y + Math.floor((primary.height - at.height) / 2),
      width: at.width,
      height: at.height,
    };
    const expected = kind === 'restore' ? environment.expected : centred;
    if (!expected) throw new Error('missing independent saved-window fixture');
    if (JSON.stringify(at) !== JSON.stringify(expected)) {
      throw new Error(
        `window ${kind}: ${JSON.stringify(at)}, expected ${JSON.stringify(expected)}`,
      );
    }
    return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
  };
  const checkAbsentTray = async (show, close) => {
    if (!show.disabled || !close.disabled || document.getElementById('tray-note').hidden) {
      throw new Error('tray settings usable without a tray host');
    }
    await smoke.shot('tray-none');
    return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
  };
  const checkTrayControls = async (kind, show, close) => {
    if (kind === 'close') {
      if (!show.checked || close.checked || close.disabled) throw new Error('tray defaults wrong');
      close.click();
      await smoke.shot('tray');
    } else if (!close.checked) throw new Error('close-to-tray not restored');
  };
  const tray = async (state, mode, step) => {
    // quit: the tray host clicks Quit; none: no host, the settings are off with a note;
    // close: turn on close-to-tray, close, and the host's Open brings the window back;
    // persisted: the setting survived a relaunch; vanish: the host goes away, the window returns.
    const kind = step.slice('tray:'.length);
    if (kind === 'quit') return undefined;
    const labels = ['tray.open', 'tray.quit', 'notice.newMessage', 'stickers.notice'].map(
      (key) => appApi.strings[key],
    );
    const tray = async (check) => {
      for (let i = 0; i < 30; i += 1) {
        const now = await invokeTest('set_tray', { prefs: null, labels });
        if (check(now)) return;
        await new Promise((resolve) => {
          setTimeout(resolve, 500);
        });
      }
      throw new Error(
        `tray ${kind}: ${JSON.stringify(await invokeTest('set_tray', { prefs: null, labels }))}`,
      );
    };
    if (state.status === 'setup') await chooseStorage(mode);
    if (kind !== 'none') await tray((now) => now.available);
    await openArea('settings', 'appearance');
    await appApi.trayShown;
    const show = document.getElementById('tray-show');
    const close = document.getElementById('tray-close');
    if (kind === 'none') return checkAbsentTray(show, close);
    await checkTrayControls(kind, show, close);
    await tray((now) => now.closeToTray);
    await invokeTest('ui_test_hide', { close: true });
    if (kind === 'vanish') await tray((now) => !now.available);
    return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
  };
  smoke.uiSmokeSystemSteps = { video, recovery, windowBounds, tray };
})();
