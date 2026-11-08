// Special native smoke routes; every result and rejection returns to the main dispatcher.
(() => {
  const video = async (step) => {
    if (step.startsWith('video-bench:'))
      return globalThis.videoBenchmark(step.slice('video-bench:'.length));
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
        window.strings[`recovery.${reason}`]
    )
      throw new Error(`recovery ${reason} not shown`);
    if (more.open || !details.textContent.startsWith(`${reason}\n${state.slot}`)) {
      throw new Error(`recovery details: ${more.open} ${details.textContent}`);
    }
    await shot('recovery');
    more.open = true;
    await checkCopy('copy-recovery-details', details);
    await shot('recovery-details');
    return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
  };
  const windowBounds = async (step) => {
    // save: move the window and quit with Ctrl+Q; restore: the saved bounds are back;
    // centre: an off-screen saved window lands in the middle of the primary display.
    const saved = { x: 120, y: 80, width: 900, height: 640 };
    const kind = step.slice('window:'.length);
    if (kind === 'save') {
      const [at] = await invokeTest('ui_test_window', { set: saved });
      if (JSON.stringify(at) !== JSON.stringify(saved))
        throw new Error(`window not moved: ${JSON.stringify(at)}`);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'q', ctrlKey: true }));
      return undefined;
    }
    const [at, primary] = await invokeTest('ui_test_window');
    const centred = {
      x: primary.x + Math.floor((primary.width - at.width) / 2),
      y: primary.y + Math.floor((primary.height - at.height) / 2),
      width: at.width,
      height: at.height,
    };
    const expected = kind === 'restore' ? saved : centred;
    if (JSON.stringify(at) !== JSON.stringify(expected)) {
      throw new Error(
        `window ${kind}: ${JSON.stringify(at)}, expected ${JSON.stringify(expected)}`,
      );
    }
    return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
  };
  window.uiSmokeSystemSteps = { video, recovery, windowBounds };
})();
