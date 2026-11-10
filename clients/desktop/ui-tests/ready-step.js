// Final ready-screen assertions and layout diagnostics for native desktop smoke.
(() => {
  const peerLinesAt = (peer, width) => {
    document.body.style.width = `${width}px`;
    const range = document.createRange();
    range.selectNodeContents(peer);
    // Chunk spans and WBRs create multiple rectangles on the same visual line.
    const bands = [];
    const boxes = [...range.getClientRects()]
      .filter((box) => box.width > 0 && box.height > 0)
      .sort((left, right) => left.top - right.top);
    for (const box of boxes) {
      const line = bands.at(-1);
      if (line && box.top < line.bottom) line.bottom = Math.max(line.bottom, box.bottom);
      else bands.push({ bottom: box.bottom });
    }
    if (peer.scrollWidth > peer.clientWidth) throw Error('Peer ID overflows its container');
    return bands.length;
  };
  const peerReflow = () => {
    const peer = document.getElementById('peer-id');
    const originalBodyWidth = document.body.style.width;
    const narrowLines = peerLinesAt(peer, 350);
    const wideLines = peerLinesAt(peer, 960);
    document.body.style.width = originalBodyWidth;
    return { narrowLines, wideLines };
  };
  const checkBackup = (mode, strings) => {
    const backup = document.getElementById('file-backup');
    const keyPath = document.getElementById('key-path').textContent;
    return mode === 'file'
      ? !backup.hidden &&
          /[\\/]identity\.key$/.test(keyPath) &&
          backup.querySelector('p').textContent === strings['storage.file.backup']
      : backup.hidden && keyPath === '';
  };
  const readyChecks = (mode, context) => {
    const { ready, area, rect, style, strings, peerId, narrowLines, wideLines } = context;
    return {
      backupMatches: checkBackup(mode, strings),
      title: document.title === strings['app.name'],
      heading:
        document.querySelector('.topbar .title').textContent === strings['app.name'] &&
        area.textContent === strings['nav.profile'],
      readyText: ready.textContent === strings['status.ready'],
      locale:
        strings['status.ready'] ===
        (appApi.pseudoLocale ? '[Réádý··]' : appApi.locale === 'ru' ? 'Готово' : 'Ready'),
      dimensions: rect.width > 0 && rect.height > 0,
      visibility: style.visibility === 'visible' && style.opacity !== '0',
      onTop: document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) === area,
      peerId: /^[0-9a-f]{64}$/.test(peerId),
      shortPeerId: shortPeerIdRule(),
      peerIdReflows: narrowLines > 1 && wideLines === 1,
      hiddenNotRendered: hiddenRendered().length === 0,
      noIdentityActions:
        document.getElementById('switch-identity').hidden &&
        document.getElementById('remove-identity').hidden,
    };
  };
  const reportReady = (checks, context) => {
    const { rect, peerId, narrowLines, wideLines } = context;
    const failed = Object.entries(checks)
      .filter(([, ok]) => !ok)
      .map(([name]) => name);
    if (!checks.peerIdReflows) failed.push(`visual lines narrow=${narrowLines} wide=${wideLines}`);
    if (!checks.onTop) {
      const top = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
      failed.push(
        `covered by ${top?.id || top?.className || top?.tagName} at ${Math.round(rect.x)}` +
          ` ${Math.round(rect.y)} in ${innerWidth}x${innerHeight} scroll ${scrollY}`,
      );
    }
    return invokeTest('ui_test_result', {
      passed: failed.length === 0,
      peerId,
      reason: failed.join(','),
    });
  };
  smoke.uiSmokeReadyStep = async (mode, existing, step) => {
    if (!document.getElementById('identity').hidden) await openArea('profile-area');
    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    if (existing && !step) await smoke.shot('ready');
    const ready = document.getElementById('app-ready');
    const area = document.querySelector('#profile-area h1');
    const { strings } = appApi;
    const peerId = document.getElementById('peer-id').textContent;
    const { narrowLines, wideLines } = peerReflow();
    // Keyboard and viewport audits may leave the page scrolled; use the final live geometry.
    area.scrollIntoView({ block: 'center', behavior: 'instant' });
    await frames();
    const rect = area.getBoundingClientRect();
    const style = getComputedStyle(area);
    const context = { ready, area, rect, style, strings, peerId, narrowLines, wideLines };
    return reportReady(readyChecks(mode, context), context);
  };
})();
