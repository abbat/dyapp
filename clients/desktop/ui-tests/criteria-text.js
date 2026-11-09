smoke.textClipping = () => {
  const found = [];
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
  const root = document.documentElement;
  if (root.scrollWidth > root.clientWidth) {
    found.push({
      rule: 'T1:horizontal-scroll',
      element: 'html',
      detail: `${root.scrollWidth}px content in ${root.clientWidth}px viewport`,
    });
  }
  for (const element of document.querySelectorAll('body *')) {
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
smoke.textClippingSelfCheck = () => {
  const fixture = document.createElement('button');
  fixture.id = 'criteria-clipping-fixture';
  fixture.textContent = 'A deliberately lengthened button label';
  fixture.style.cssText =
    'position:fixed;top:0;left:0;width:20px;height:20px;min-width:0;min-height:0;padding:0;overflow:hidden;white-space:nowrap';
  const details = document.createElement('details');
  details.innerHTML = '<summary>Details fixture</summary>';
  document.body.append(fixture, details);
  try {
    if (!smoke.textClipping().some((finding) => finding.element === '#criteria-clipping-fixture'))
      throw new Error('text clipping did not reject a lengthened button label');
    details.append(fixture);
    if (smoke.textClipping().some((finding) => finding.element === '#criteria-clipping-fixture'))
      throw new Error('closed details content must not be audited as visible');
    details.open = true;
    if (!smoke.textClipping().some((finding) => finding.element === '#criteria-clipping-fixture'))
      throw new Error('opened details content must be audited');
  } finally {
    fixture.remove();
    details.remove();
  }
};
