// The close event, which the app handles first, comes on a later task in no fixed order with
// timers, so a check waits for the event itself.
const closed = (dialog) =>
  new Promise((resolve, reject) => {
    const finish = () => {
      clearTimeout(deadline);
      resolve();
    };
    const deadline = setTimeout(() => {
      dialog.removeEventListener('close', finish);
      reject(Error(`${dialog.id} close deadline`));
    }, 10000);
    dialog.addEventListener('close', finish, { once: true });
  });
// Wait for paint callbacks after fixture/style changes, with a fresh bounded deadline.
const paintReady = () =>
  new Promise((resolve, reject) => {
    let frame;
    const deadline = setTimeout(() => {
      cancelAnimationFrame(frame);
      reject(Error('accessibility paint deadline'));
    }, 10000);
    frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        clearTimeout(deadline);
        resolve();
      });
    });
  });
// axe's WCAG 2.2 rules include disabled-by-default rules such as target-size.
smoke.accessibilityOptions = () => {
  if (window.axe?.version !== '4.14.0') throw new Error('axe-core 4.14.0 is required');
  const tags = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];
  const rules = Object.fromEntries(
    window.axe.getRules(tags).map((rule) => [rule.ruleId, { enabled: true }]),
  );
  if (!rules['target-size']) throw new Error('axe WCAG 2.2 target-size rule is missing');
  return { runOnly: { type: 'tag', values: tags }, rules };
};
const restoreVisibility = (covered) => {
  for (const [child, visibility, priority, hadStyle] of covered) {
    if (hadStyle || child.style.length > 1)
      child.style.setProperty('visibility', visibility, priority);
    else child.removeAttribute('style');
  }
};
const captureAuditPosition = () => {
  const focus = document.activeElement;
  const saved = [...document.querySelectorAll('*')]
    .filter(
      (element) =>
        element.scrollWidth > element.clientWidth || element.scrollHeight > element.clientHeight,
    )
    .map((element) => [element, element.scrollLeft, element.scrollTop]);
  const position = [scrollX, scrollY];
  return async () => {
    if (focus?.isConnected && document.activeElement !== focus && typeof focus.focus === 'function')
      focus.focus({ preventScroll: true });
    for (const [element, left, top] of saved)
      if (element.scrollLeft !== left || element.scrollTop !== top)
        element.scrollTo({ left, top, behavior: 'instant' });
    if (scrollX !== position[0] || scrollY !== position[1])
      window.scrollTo({ left: position[0], top: position[1], behavior: 'instant' });
    await paintReady();
  };
};
// A clipped scroll position is not a contrast measurement; verify the same node where readable.
const contrastElement = (node) => {
  if (node.target.length !== 1 || typeof node.target[0] !== 'string') return null;
  try {
    const matches = document.querySelectorAll(node.target[0]);
    return matches.length === 1 ? matches[0] : null;
  } catch {
    return null;
  }
};
const contrastAtReadablePosition = async (element, options) => {
  const saved = [];
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    if (parent.scrollWidth > parent.clientWidth || parent.scrollHeight > parent.clientHeight)
      saved.push([parent, parent.scrollLeft, parent.scrollTop]);
  }
  const position = [scrollX, scrollY];
  try {
    element.scrollIntoView({ block: 'start', inline: 'nearest', behavior: 'instant' });
    await paintReady();
    return await window.axe.run(element, options);
  } finally {
    for (const [parent, left, top] of saved)
      if (parent.scrollLeft !== left || parent.scrollTop !== top)
        parent.scrollTo({ left, top, behavior: 'instant' });
    if (scrollX !== position[0] || scrollY !== position[1])
      window.scrollTo({ left: position[0], top: position[1], behavior: 'instant' });
    await paintReady();
  }
};
const contrastTargets = (result) =>
  new Map(
    (result.incomplete.find((rule) => rule.id === 'color-contrast')?.nodes || []).map((node) => [
      node,
      { node, element: contrastElement(node) },
    ]),
  );
const currentContrastTarget = (target) =>
  target?.element?.isConnected && contrastElement(target.node) === target.element;
const confirmContrast = (result, review, target, checked) => {
  if (!currentContrastTarget(target)) return;
  const { node, element } = target;
  const matching = (rule) =>
    rule?.nodes.find((candidate) => contrastElement(candidate) === element);
  const violation = checked.violations.find((rule) => rule.id === 'color-contrast');
  const failed = matching(violation);
  const passing = checked.passes.find((rule) => rule.id === 'color-contrast');
  const passed = matching(passing);
  if (!failed && !passed) return;
  review.nodes = review.nodes.filter((candidate) => candidate !== node);
  const collection = failed ? result.violations : result.passes;
  let confirmed = collection.find((rule) => rule.id === 'color-contrast');
  if (!confirmed) {
    confirmed = { ...(failed ? violation : passing), nodes: [] };
    collection.push(confirmed);
  }
  confirmed.nodes.push({ ...(failed || passed), target: node.target });
};
const resolveScrollableContrast = async (result, options, targets) => {
  const review = result.incomplete.find((rule) => rule.id === 'color-contrast');
  if (!review) return result;
  for (const node of [...review.nodes]) {
    const target = targets.get(node);
    if (currentContrastTarget(target))
      confirmContrast(
        result,
        review,
        target,
        await contrastAtReadablePosition(target.element, options),
      );
  }
  return result;
};
const popupForegroundContrast = async (menu, options) => {
  const hidden = [];
  try {
    for (const child of coveredModalBranches(menu)) {
      hidden.push([
        child,
        child.style.getPropertyValue('visibility'),
        child.style.getPropertyPriority('visibility'),
        child.hasAttribute('style'),
      ]);
      child.style.setProperty('visibility', 'hidden', 'important');
    }
    await paintReady();
    return await window.axe.run(menu, options);
  } finally {
    restoreVisibility(hidden);
    await paintReady();
  }
};
const popupBackgroundContrast = async (menu, options) => {
  const saved = menu.getAttribute('style');
  try {
    menu.style.setProperty('opacity', '0', 'important');
    menu.style.setProperty('pointer-events', 'none', 'important');
    await paintReady();
    return await window.axe.run(document, options);
  } finally {
    if (saved === null) menu.removeAttribute('style');
    else menu.setAttribute('style', saved);
    await paintReady();
  }
};
const resolvePopupContrast = async (result, options, targets) => {
  const review = result.incomplete.find((rule) => rule.id === 'color-contrast');
  if (!review) return result;
  for (const menu of smoke.popupMenus()) {
    if (!smoke.popupPaintEligible(menu)) continue;
    const inside = [...review.nodes]
      .map((node) => targets.get(node))
      .filter((target) => currentContrastTarget(target) && menu.contains(target.element));
    if (inside.length) {
      const checked = await popupForegroundContrast(menu, options);
      for (const target of inside) confirmContrast(result, review, target, checked);
    }
    const outside = [...review.nodes]
      .map((node) => targets.get(node))
      .filter(
        (target) =>
          currentContrastTarget(target) &&
          !menu.contains(target.element) &&
          smoke.popupCoversText(menu, target.element) &&
          target.node.any.some((check) => check.data?.messageKey === 'bgOverlap'),
      );
    if (outside.length) {
      const checked = await popupBackgroundContrast(menu, options);
      for (const target of outside) confirmContrast(result, review, target, checked);
    }
  }
  return result;
};

const coveredModalBranches = (modal) => {
  const covered = [];
  for (let branch = modal; branch.parentElement; branch = branch.parentElement)
    covered.push(...[...branch.parentElement.children].filter((child) => child !== branch));
  return covered;
};
// Keep layout while hiding paint behind opaque native or ARIA modals during the audit.
const runAccessibilityAudit = async () => {
  const options = smoke.accessibilityOptions();
  const restorePosition = captureAuditPosition();
  const modal = document.activeElement?.closest('dialog:modal, [role="dialog"][aria-modal="true"]');
  const hidden = [];
  if (modal) {
    const style = getComputedStyle(modal);
    const opaque = window.axe.commons.color.getOwnBackgroundColor(style).alpha === 1;
    if (opaque && style.opacity === '1' && style.backgroundImage === 'none') {
      for (const child of coveredModalBranches(modal)) {
        const visibility = child.style.getPropertyValue('visibility');
        const priority = child.style.getPropertyPriority('visibility');
        hidden.push([child, visibility, priority, child.hasAttribute('style')]);
        child.style.setProperty('visibility', 'hidden', 'important');
      }
    }
  }
  try {
    await paintReady();
    const result = await window.axe.run(document, options);
    const targets = contrastTargets(result);
    await resolvePopupContrast(result, options, targets);
    return await resolveScrollableContrast(result, options, targets);
  } finally {
    restoreVisibility(hidden);
    await restorePosition();
  }
};
smoke.accessibility = async () => {
  const result = await runAccessibilityAudit();
  const findings = [];
  for (const violation of result.violations) {
    for (const node of violation.nodes) {
      findings.push({
        rule: `axe:${violation.id}`,
        element: JSON.stringify(node.target),
        detail: JSON.stringify({
          summary: node.failureSummary,
          checks: node.any,
          description: violation.description,
        }),
      });
    }
  }
  for (const review of result.incomplete) {
    if (!['target-size', 'color-contrast'].includes(review.id)) continue;
    for (const node of review.nodes) {
      findings.push({
        rule: `axe-review:${review.id}`,
        element: JSON.stringify(node.target),
        detail: JSON.stringify({
          summary: node.failureSummary,
          checks: node.any,
          description: review.description,
        }),
      });
    }
  }
  return findings;
};
// Synthetic calibration controls their background paint independently of the current screen.
const calibrationBackground = () => {
  const covered = [...document.body.children]
    .flatMap((child) => [child, ...child.querySelectorAll('*')])
    .map((child) => [
      child,
      child.style.getPropertyValue('visibility'),
      child.style.getPropertyPriority('visibility'),
      child.hasAttribute('style'),
    ]);
  for (const [child] of covered) child.style.setProperty('visibility', 'hidden', 'important');
  return () => restoreVisibility(covered);
};
const calibrationFailure = (id, fixture, result) => {
  const subject = fixture.querySelector(id === 'color-contrast' ? 'p' : 'button');
  const rect = subject.getBoundingClientRect();
  const style = getComputedStyle(subject);
  const top = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
  const checks = Object.fromEntries(
    ['violations', 'incomplete', 'passes', 'inapplicable'].map((kind) => [
      kind,
      result[kind].filter((finding) => finding.id === id),
    ]),
  );
  return new Error(
    `axe did not reject the ${id} counterexample: ${JSON.stringify({
      rect,
      color: style.color,
      background: style.backgroundColor,
      visibility: style.visibility,
      focused: document.hasFocus(),
      modal: fixture.matches(':modal'),
      onTop: top?.id || top?.tagName,
      scroll: [scrollX, scrollY],
      dialogs: [...document.querySelectorAll('dialog:modal')].map((dialog) => dialog.id),
      active: document.activeElement?.id || document.activeElement?.tagName,
      checks,
    })}`,
  );
};
const nestedContrastHost = (fixture) => {
  const host = document.createElement('section');
  host.style.cssText = 'position:fixed;inset:0';
  host.innerHTML =
    '<div style="position:absolute;inset:0 50% 0 0;background:black"></div><div style="position:absolute;inset:0 0 0 50%;background:white"></div>';
  host.append(fixture);
  document.body.append(host);
  return host;
};
const enlargeContrastText = (fixture, text) => {
  const repeats = Math.ceil(fixture.clientHeight / text.getBoundingClientRect().height) + 1;
  text.textContent = text.textContent.repeat(repeats);
  if (text.getBoundingClientRect().height <= fixture.clientHeight)
    throw Error('oversized contrast fixture fits its scrolling container');
};
const calibrateScrollableContrast = async (oversized = false) => {
  const previous = document.activeElement;
  const fixture = document.createElement('dialog');
  fixture.id = 'criteria-scrollable-contrast';
  fixture.setAttribute('aria-label', 'Scrollable contrast calibration');
  fixture.style.cssText =
    'position:fixed;top:0;left:0;width:240px;height:30vh;max-height:30vh;overflow:auto;background:white';
  fixture.innerHTML =
    '<div aria-hidden="true"></div><p id="criteria-scrollable-text" style="font:18px sans-serif">Contrast remains checkable when this text starts partially outside its scrolling container.</p><div aria-hidden="true"></div><button>Anchor</button>';
  const host = nestedContrastHost(fixture);
  fixture.showModal();
  const text = fixture.querySelector('p');
  if (oversized) enlargeContrastText(fixture, text);
  for (const spacer of fixture.querySelectorAll('div'))
    spacer.style.height = `${fixture.clientHeight}px`;
  const anchor = fixture.querySelector('button');
  try {
    for (const [color, expected] of [
      ['#000', 'passed'],
      ['#eee', 'failed'],
    ]) {
      fixture.style.color = color;
      anchor.focus({ preventScroll: true });
      const textBox = text.getBoundingClientRect();
      const frame = fixture.getBoundingClientRect();
      fixture.scrollTop += textBox.top - frame.top + textBox.height / 2;
      const position = fixture.scrollTop;
      const result = await runAccessibilityAudit();
      const status = ['violations', 'incomplete', 'passes'].find((kind) =>
        result[kind].some(
          (rule) =>
            rule.id === 'color-contrast' &&
            rule.nodes.some((node) => contrastElement(node) === text),
        ),
      );
      const actual = { violations: 'failed', incomplete: 'incomplete', passes: 'passed' }[status];
      if (actual !== expected) throw Error(`scrolled contrast expected ${expected}, got ${actual}`);
      if (fixture.scrollTop !== position || document.activeElement !== anchor)
        throw Error(
          `contrast audit changed scroll or focus: ${JSON.stringify({ before: position, after: fixture.scrollTop, expectedFocus: anchor.tagName, actualFocus: document.activeElement?.tagName })}`,
        );
    }
  } finally {
    const finished = closed(fixture);
    fixture.close();
    await finished;
    host.remove();
    previous?.focus({ preventScroll: true });
  }
};

smoke.accessibilitySelfCheck = async () => {
  const previous = document.activeElement;
  const restore = calibrationBackground();
  // Calibration must remain visible when the first audit starts inside another native modal.
  const fixture = document.createElement('dialog');
  fixture.setAttribute('aria-label', 'Accessibility counterexamples');
  fixture.id = 'criteria-axe-fixture';
  fixture.style.cssText =
    'position:fixed;top:0;left:0;z-index:2147483647;background:white;color:black';
  fixture.innerHTML =
    '<p id="criteria-axe-low-contrast" style="color:#eee;background:white;font:18px sans-serif;max-width:18ch">Unreadable wrapped consent label over an opaque surface.</p><div style="display:flex"><button id="criteria-axe-small-a">a</button><button id="criteria-axe-small-b">b</button></div>';
  for (const button of fixture.querySelectorAll('button')) {
    button.style.cssText =
      'box-sizing:border-box;width:12px;height:12px;min-width:0;min-height:0;padding:0;margin:0;border:0;background:white;color:black';
  }
  document.body.append(fixture);
  fixture.showModal();
  try {
    await paintReady();
    const result = await window.axe.run(fixture, smoke.accessibilityOptions());
    for (const id of ['color-contrast', 'target-size']) {
      if (!result.violations.some((finding) => finding.id === id)) {
        throw calibrationFailure(id, fixture, result);
      }
    }
  } finally {
    try {
      const finished = closed(fixture);
      fixture.close();
      await finished;
    } finally {
      fixture.remove();
      restore();
      previous?.focus({ preventScroll: true });
    }
  }
  for (const oversized of [false, true]) await calibrateScrollableContrast(oversized);
};
