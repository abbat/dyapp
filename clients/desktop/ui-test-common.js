// Runs in the actual Tauri webview after the bundled page loads.
const invokeTest = window.__TAURI__.core.invoke;
const fail = (error) => invokeTest('ui_test_result', {
  passed: false, peerId: '', reason: String(error),
});
const frames = () => new Promise((resolve) => {
  requestAnimationFrame(() => requestAnimationFrame(resolve));
});
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
const shot = async (name) => {
  await audit(name);
  await invokeTest('ui_test_shot', { name }).catch(() => {});
};
