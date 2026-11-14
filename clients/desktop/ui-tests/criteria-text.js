const clippingContext = () => {
  const modal = document.activeElement?.closest('dialog:modal, [role="dialog"][aria-modal="true"]');
  return modal?.getClientRects().length && getComputedStyle(modal).visibility !== 'hidden'
    ? modal
    : null;
};
const horizontalClipping = (modal, name) => {
  const root = document.documentElement;
  const bounds = modal?.getBoundingClientRect();
  const overflow = modal
    ? bounds.left < 0 || bounds.right > root.clientWidth
    : root.scrollWidth > root.clientWidth;
  return overflow
    ? {
        rule: 'T1:horizontal-scroll',
        element: modal ? name(modal) : 'html',
        detail: modal
          ? `${bounds.left}..${bounds.right}px modal in ${root.clientWidth}px viewport`
          : `${root.scrollWidth}px content in ${root.clientWidth}px viewport`,
      }
    : null;
};
smoke.textClipping = () => {
  const found = [];
  const modal = clippingContext();
  const visible = (element) => {
    if (!element.getClientRects().length || getComputedStyle(element).visibility !== 'visible')
      return false;
    for (let parent = element.parentElement; parent; parent = parent.parentElement) {
      if (
        parent.matches('details:not([open])') &&
        !parent.querySelector(':scope > summary')?.contains(element)
      )
        return false;
    }
    return true;
  };
  const name = (element) => {
    if (element.id) return `#${element.id}`;
    const parent = element.parentElement;
    if (!parent) return element.tagName.toLowerCase();
    return `${name(parent)}>${element.tagName.toLowerCase()}:nth-child(${[...parent.children].indexOf(element) + 1})`;
  };
  const horizontal = horizontalClipping(modal, name);
  if (horizontal) found.push(horizontal);
  const elements = modal
    ? [modal, ...modal.querySelectorAll('*')]
    : document.querySelectorAll('body *');
  for (const element of elements) {
    if (
      !visible(element) ||
      !element.textContent.trim() ||
      !element.clientWidth ||
      !element.clientHeight
    )
      continue;
    const style = getComputedStyle(element);
    const x = element.scrollWidth > element.clientWidth && !/^(auto|scroll)$/.test(style.overflowX);
    const y =
      element.scrollHeight > element.clientHeight && !/^(auto|scroll)$/.test(style.overflowY);
    if (x || y)
      found.push({
        rule: 'T1:clipping',
        element: name(element),
        detail: `${element.scrollWidth}x${element.scrollHeight}px content in ${element.clientWidth}x${element.clientHeight}px box`,
      });
  }
  return found;
};
const clippingModalSelfCheck = (background, host, kind) => {
  const previous = document.activeElement;
  const modal = document.createElement(kind);
  modal.setAttribute('role', 'dialog');
  modal.setAttribute('aria-modal', 'true');
  modal.setAttribute('aria-label', 'Clipping context calibration');
  modal.style.cssText =
    'position:fixed;top:0;left:0;width:240px;z-index:2147483647;background:white';
  const foreground = background.cloneNode(true);
  foreground.id = 'criteria-clipping-modal-foreground';
  const anchor = document.createElement('button');
  anchor.textContent = 'Modal anchor';
  modal.append(foreground, anchor);
  host.append(background, modal);
  if (kind === 'dialog') modal.showModal();
  try {
    anchor.focus({ preventScroll: true });
    const findings = smoke.textClipping();
    if (findings.some((row) => row.element === `#${background.id}`))
      throw Error('inactive background was audited as modal content');
    if (!findings.some((row) => row.element === `#${foreground.id}`))
      throw Error('modal clipping escaped the audit');
    modal.hidden = true;
    if (!smoke.textClipping().some((row) => row.element === `#${background.id}`))
      throw Error('hidden modal suppressed the page audit');
    modal.hidden = false;
    anchor.focus({ preventScroll: true });
    const left = modal.style.left;
    modal.style.left = `${document.documentElement.clientWidth}px`;
    if (!smoke.textClipping().some((row) => row.rule === 'T1:horizontal-scroll'))
      throw Error('out-of-viewport modal escaped the audit');
    modal.style.left = left;
    if (kind === 'dialog') modal.close();
    modal.remove();
    previous?.focus({ preventScroll: true });
    if (!smoke.textClipping().some((row) => row.element === `#${background.id}`))
      throw Error('closing the modal lost background clipping coverage');
  } finally {
    if (modal.matches(':modal')) modal.close();
    modal.remove();
    previous?.focus({ preventScroll: true });
  }
};

smoke.textClippingSelfCheck = () => {
  const fixture = document.createElement('button');
  fixture.id = 'criteria-clipping-fixture';
  fixture.textContent = 'A deliberately lengthened button label';
  fixture.style.cssText =
    'position:fixed;top:0;left:0;width:20px;height:20px;min-width:0;min-height:0;padding:0;overflow:hidden;white-space:nowrap';
  const details = document.createElement('details');
  details.innerHTML = '<summary>Details fixture</summary>';
  const host = clippingContext() || document.body;
  host.append(fixture, details);
  try {
    if (!smoke.textClipping().some((finding) => finding.element === '#criteria-clipping-fixture'))
      throw new Error('text clipping did not reject a lengthened button label');
    details.append(fixture);
    if (smoke.textClipping().some((finding) => finding.element === '#criteria-clipping-fixture'))
      throw new Error('closed details content must not be audited as visible');
    details.open = true;
    if (!smoke.textClipping().some((finding) => finding.element === '#criteria-clipping-fixture'))
      throw new Error('opened details content must be audited');
    for (const kind of ['div', 'dialog']) clippingModalSelfCheck(fixture, host, kind);
  } finally {
    fixture.remove();
    details.remove();
  }
};
