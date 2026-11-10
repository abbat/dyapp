// Counterexamples for axe's opaque-modal background isolation.
(() => {
  const fixture = () => {
    // Keep actual page content from masking the controlled normal-flow underlay.
    const restore = calibrationBackground();
    const modal = document.createElement('dialog');
    modal.className = 'modal';
    modal.setAttribute('aria-label', 'Accessibility calibration');
    modal.innerHTML =
      '<span id="criteria-modal-label" style="display:block;max-width:18ch">A wrapped consent label must keep its readable foreground over the modal surface.</span><button class="btn" type="button">Cancel</button>';
    const underlay = document.createElement('div');
    document.body.prepend(underlay);
    document.body.append(modal);
    modal.showModal();
    modal.querySelector('button').focus();
    const label = modal.querySelector('span');
    const at = label.getBoundingClientRect();
    underlay.style.cssText = `height:${at.top + at.height / 2}px;background:var(--accent)`;
    return { modal, underlay, label, restore };
  };
  const contrastFinding = (findings) =>
    findings.some(
      (finding) =>
        finding.rule.endsWith('color-contrast') &&
        finding.element.includes('#criteria-modal-label'),
    );
  const checkForeground = async (modal, label) => {
    label.style.color = 'var(--surface)';
    if (!contrastFinding(await smoke.accessibility()))
      throw new Error('modal isolation accepted unreadable text');
    label.style.removeProperty('color');
    const cover = document.createElement('span');
    cover.setAttribute('aria-hidden', 'true');
    cover.style.cssText = `position:absolute;left:${label.offsetLeft}px;top:${label.offsetTop}px;width:${label.offsetWidth}px;height:${label.offsetHeight / 2}px;background:var(--surface);z-index:1`;
    modal.append(cover);
    try {
      if (!contrastFinding(await smoke.accessibility()))
        throw new Error('modal isolation hid a real foreground overlap');
    } finally {
      cover.remove();
    }
  };
  const checkRestoration = async () => {
    const before = [...document.body.children].map((child) => [
      child,
      child.style.getPropertyValue('visibility'),
      child.style.getPropertyPriority('visibility'),
    ]);
    const run = window.axe.run;
    let rejected = false;
    window.axe.run = async (..._arguments) => {
      throw new Error('expected audit failure');
    };
    try {
      await smoke.accessibility();
    } catch (error) {
      rejected = error.message === 'expected audit failure';
    } finally {
      window.axe.run = run;
    }
    if (!rejected) throw new Error('audit failure was swallowed');
    for (const [child, visibility, priority] of before) {
      if (
        child.style.getPropertyValue('visibility') !== visibility ||
        child.style.getPropertyPriority('visibility') !== priority
      )
        throw new Error(`audit restoration changed ${child.tagName}#${child.id} visibility`);
    }
  };
  smoke.modalAccessibilitySelfCheck = async () => {
    const previous = document.activeElement;
    const { modal, underlay, label, restore } = fixture();
    try {
      await paintReady();
      const raw = await window.axe.run(document, smoke.accessibilityOptions());
      const reproduced = raw.incomplete.some(
        (finding) =>
          finding.id === 'color-contrast' &&
          finding.nodes.some((node) => node.target.includes('#criteria-modal-label')),
      );
      if (!reproduced) throw new Error('modal underlay counterexample did not reproduce');
      if (contrastFinding(await smoke.accessibility()))
        throw new Error('opaque modal still depends on covered page paint');
      await checkForeground(modal, label);
      await checkRestoration();
    } finally {
      try {
        if (modal.open) {
          const finished = closed(modal);
          modal.close();
          await finished;
        }
      } finally {
        modal.remove();
        underlay.remove();
        restore();
        previous?.focus({ preventScroll: true });
      }
    }
  };
})();
