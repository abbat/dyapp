// Runs in the actual Tauri webview after the bundled page loads.
const invokeTest = window.__TAURI__.core.invoke;
window.fail = (error) => invokeTest('ui_test_result', {
  passed: false, peerId: '', reason: String(error),
});
const frames = () => new Promise((resolve) => {
  requestAnimationFrame(() => requestAnimationFrame(resolve));
});
// axe-core 4.14 marks target-size disabled by default, so every checkpoint enables it.
const targetSize = { runOnly: { type: 'rule', values: ['target-size'] },
  rules: { 'target-size': { enabled: true } } };
const checkTargetSizeFixture = async () => {
  const fixture = document.createElement('div');
  fixture.style.cssText = 'position:fixed;top:0;left:0;z-index:2147483647;display:flex';
  for (const id of ['a', 'b']) {
    const button = document.createElement('button');
    button.id = `target-size-fixture-${id}`;
    button.textContent = id;
    button.style.cssText = 'box-sizing:border-box;width:12px;height:12px;min-width:0;min-height:0;padding:0;margin:0;border:0';
    fixture.append(button);
  }
  document.body.append(fixture);
  try {
    await frames();
    const result = await window.axe.run(document, targetSize);
    if (!result.violations.some((violation) => violation.id === 'target-size'
      && violation.nodes.some((node) => node.target.some((target) => target.includes('target-size-fixture-'))))) {
      throw new Error('axe target-size did not report the undersized fixture');
    }
  } finally {
    fixture.remove();
  }
};
// With axe-core loaded (DYAPP_AXE), checks the current screen: WCAG 2.x A/AA (labels and
// contrast included) in the light and dark themes, tab order following the DOM, a visible focus
// ring on every focusable control and no horizontal overflow at 440 px.
const audit = async (name) => {
  if (!window.axe) return;
  const problems = [];
  const root = document.documentElement;
  const theme = root.dataset.theme;
  const tags = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];
  try {
    for (const mode of ['light', 'dark']) {
      root.dataset.theme = mode;
      await frames();
      const result = await window.axe.run(document, { runOnly: { type: 'tag', values: tags } });
      for (const v of result.violations) problems.push(`${mode} ${v.id} ${v.nodes[0].target}`);
    }
  } finally {
    if (theme) root.dataset.theme = theme; else delete root.dataset.theme;
  }
  if (name === 'search-results') await checkTargetSizeFixture();
  const sized = await window.axe.run(document, targetSize);
  for (const violation of sized.violations) {
    problems.push(`target-size ${violation.nodes[0].target}`);
  }
  for (const review of sized.incomplete) {
    problems.push(`target-size needs review ${review.nodes[0].target}`);
  }
  const visible = (element) => element.getClientRects().length > 0
    && getComputedStyle(element).visibility === 'visible';
  for (const element of document.querySelectorAll('[tabindex]')) {
    if (element.tabIndex > 0 && visible(element)) {
      problems.push(`tabindex>0 ${element.id || element.tagName}`);
    }
  }
  const before = document.activeElement;
  const [scrollLeft, scrollTop] = [window.scrollX, window.scrollY];
  const focusable = 'a[href], button, input, select, textarea, [tabindex]:not([tabindex="-1"])';
  for (const element of document.querySelectorAll(focusable)) {
    if (element.disabled || !visible(element)) continue;
    element.focus();
    const style = getComputedStyle(element);
    if (document.activeElement === element && style.boxShadow === 'none'
      && style.outlineStyle === 'none') {
      problems.push(`no focus ring ${element.id || element.tagName}`);
    }
  }
  if (before?.focus) before.focus(); else document.activeElement.blur();
  const [window0] = await invokeTest('ui_test_window', { set: null });
  await invokeTest('ui_test_window', {
    set: { ...window0, width: Math.round(440 * devicePixelRatio) },
  });
  await frames();
  const overflow = root.scrollWidth - root.clientWidth;
  const wide = [...document.querySelectorAll('body *')]
    .filter((element) => element.scrollWidth > element.clientWidth + 1 && visible(element))
    .slice(-3).map((element) => element.id || element.className || element.tagName);
  await invokeTest('ui_test_window', { set: window0 });
  await frames();
  // Focusing and the narrow window scroll the page; the checks after the audit expect it as it was.
  window.scrollTo(scrollLeft, scrollTop);
  if (overflow > 0) problems.push(`overflow ${overflow}px at 440: ${wide}`);
  if (problems.length) throw new Error(`accessibility ${name}: ${problems.join('; ')}`);
};
// Audits the current screen, then saves its screenshots when the run asks for them.
window.shot = async (name) => {
  await audit(name);
  await invokeTest('ui_test_shot', { name });
};

globalThis.checkSafetyNumber = async () => {
  document.getElementById('view-safety').click();
  await window.safetyDone;
  const number = document.getElementById('view-safety-number').textContent;
  const qr = document.getElementById('view-safety-qr');
  const pixels = qr.getContext('2d');
  const quiet = pixels.getImageData(0, 0, 1, 1).data;
  const finder = pixels.getImageData(24, 24, 1, 1).data;
  if (qr.width < 174 || quiet[0] !== 255 || quiet[1] !== 255 || quiet[2] !== 255
    || finder[0] !== 0 || finder[1] !== 0 || finder[2] !== 0) {
    throw new Error('the contact QR lacks a quiet zone or finder pattern');
  }
  if (document.getElementById('view-safety-panel').hidden
    || !/^(\d{5} ){11}\d{5}$/.test(number)
    || document.getElementById('view-safety').getAttribute('aria-expanded') !== 'true'
    || !document.querySelector('#view-safety-panel [data-i18n="view.safety.unverified"]').textContent) {
    throw new Error('the unverified safety number was not shown as 12 groups of five digits');
  }
  const scan = document.getElementById('view-safety-scan-status');
  await globalThis.scanQrFile(new File(['not an image'], 'invalid.png', { type: 'image/png' }));
  if (scan.textContent !== window.strings['view.safety.scan.loadFailed']) {
    throw new Error('malformed PNG was not reported');
  }
  const png = await new Promise((resolve) => qr.toBlob(resolve, 'image/png'));
  if (!png) throw new Error('could not make the QR fixture PNG');
  // This search fixture opens our own profile, so both QR directions are identical.
  // Direction rejection for two distinct peers is covered by the Rust safety tests.
  await globalThis.scanQrFile(new File([png], 'loopback.png', { type: 'image/png' }));
  if (scan.hidden || scan.textContent !== window.strings['view.safety.scan.matched']) {
    throw new Error('loopback QR image did not match');
  }
  // Opening the panel marks nothing, Cancel sends nothing, and the real command refuses this
  // own-profile fixture; ui-test-identity.js answers the command for the other outcomes.
  const confirm = document.getElementById('view-confirm');
  const dialog = document.getElementById('verify-dialog');
  const mark = document.getElementById('view-verified');
  if (confirm.hidden || !mark.hidden) throw new Error('opening the safety panel changed the mark');
  confirm.click();
  if (!dialog.open || document.activeElement.id !== 'verify-cancel') {
    throw new Error('verification did not ask first with Cancel focused');
  }
  await shot('search-safety-confirm');
  document.getElementById('verify-cancel').click();
  if (dialog.open || confirm.disabled || scan.textContent !== window.strings['view.safety.scan.matched']) {
    throw new Error('Cancel did not leave the comparison as it was');
  }
  confirm.click();
  document.getElementById('verify-confirm').click();
  await window.confirmDone;
  if (scan.textContent !== window.strings['view.safety.confirm.ownPeer'] || !mark.hidden
    || confirm.disabled || document.activeElement !== confirm) {
    throw new Error('own profile was not refused');
  }
  const camera = document.getElementById('view-safety-camera');
  const mediaDescriptor = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');
  const fakeMedia = (getUserMedia) => Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true, value: { getUserMedia },
  });
  try {
    for (const [name, expected] of [['NotAllowedError', 'denied'], ['NotFoundError', 'unavailable']]) {
      fakeMedia(async () => { const error = new Error(name); error.name = name; throw error; });
      camera.click();
      await window.cameraDone;
      if (scan.textContent !== window.strings[`view.safety.camera.${expected}`]) {
        throw new Error(`camera ${name} was not reported`);
      }
    }
    let finishCapture;
    let stopped = false;
    fakeMedia(() => new Promise((resolve) => { finishCapture = resolve; }));
    camera.click();
    camera.click();
    finishCapture({ getTracks: () => [{ stop: () => { stopped = true; } }] });
    await window.cameraDone;
    if (!stopped || scan.textContent !== window.strings['view.safety.camera.stopped']) {
      throw new Error('cancelled camera capture was not stopped');
    }
  } finally {
    if (mediaDescriptor) Object.defineProperty(navigator, 'mediaDevices', mediaDescriptor);
    else delete navigator.mediaDevices;
  }
  const originalTimeout = globalThis.setTimeout;
  const expiries = [];
  globalThis.setTimeout = (callback, delay, ...args) => {
    if (delay === 60000) { expiries.push(callback); return 0; }
    return originalTimeout(callback, delay, ...args);
  };
  try {
    const copy = document.getElementById('copy-safety-number');
    copy.click();
    await window.safetyCopied;
    if (copy.textContent !== window.strings['action.copied'] || expiries.length !== 1) {
      throw new Error('safety copy was not scheduled');
    }
    expiries.shift()();
    if (await window.safetyClearDone !== true) throw new Error('safety copy did not expire');
    copy.click();
    await window.safetyCopied;
    const other = await invoke('copy_safety_number', { peerId: '00'.repeat(32) });
    if (other === number) throw new Error('second safety number was not different');
    expiries.shift()();
    if (await window.safetyClearDone !== false) throw new Error('expiry erased a later clipboard value');
    copy.click();
    await window.safetyCopied;
    document.getElementById('copy-view-peer').click();
    await window.copied;
    expiries.shift()();
    if (await window.safetyClearDone !== false) throw new Error('safety expiry erased an ordinary Peer ID copy');
  } finally {
    globalThis.setTimeout = originalTimeout;
  }
};
