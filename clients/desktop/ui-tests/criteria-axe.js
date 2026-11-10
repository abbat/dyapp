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
// axe 4.14 compares text-line hit stacks below an opaque native dialog.
// Hide only occluded page paint during the audit; keep its layout and restore on every exit.
const runAccessibilityAudit = async () => {
  const options = smoke.accessibilityOptions();
  const modal = document.activeElement?.closest('dialog:modal');
  const hidden = [];
  if (modal) {
    const style = getComputedStyle(modal);
    const opaque = window.axe.commons.color.getOwnBackgroundColor(style).alpha === 1;
    if (opaque && style.opacity === '1' && style.backgroundImage === 'none') {
      for (const child of document.body.children) {
        if (child.contains(modal)) continue;
        const visibility = child.style.getPropertyValue('visibility');
        const priority = child.style.getPropertyPriority('visibility');
        hidden.push([child, visibility, priority, child.hasAttribute('style')]);
        child.style.setProperty('visibility', 'hidden', 'important');
      }
    }
  }
  try {
    await paintReady();
    return await window.axe.run(document, options);
  } finally {
    restoreVisibility(hidden);
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
};
