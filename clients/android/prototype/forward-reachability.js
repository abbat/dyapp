const reached = (node) => {
  const rect = node.getBoundingClientRect();
  if (
    rect.width <= 0 ||
    rect.height <= 0 ||
    rect.left < 0 ||
    rect.top < 0 ||
    rect.right > innerWidth ||
    rect.bottom > innerHeight
  )
    return false;
  const top = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
  return document.activeElement === node && top != null && node.contains(top);
};

const reveal = (node) =>
  new Promise((resolve, reject) => {
    const lifetime = new AbortController();
    let settled = false;
    let frame = 0;
    const finish = (failed = false) => {
      if (settled) return;
      settled = true;
      lifetime.abort();
      observer.disconnect();
      clearTimeout(deadline);
      cancelAnimationFrame(frame);
      if (failed) reject(new Error(`Prototype control is unreachable: ${node.id}`));
      else resolve(undefined);
    };
    const check = () => {
      if (reached(node)) finish();
    };
    const observer = new IntersectionObserver(check);
    const deadline = setTimeout(() => finish(true), 10000);
    window.addEventListener('scroll', check, { capture: true, signal: lifetime.signal });
    window.addEventListener('resize', check, { signal: lifetime.signal });
    observer.observe(node);
    node.focus({ preventScroll: true });
    node.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' });
    frame = requestAnimationFrame(check);
  });

const calibrate = (node) => {
  const original = node.getAttribute('style');
  const home = node.parentElement;
  const next = node.nextSibling;
  const cover = document.createElement('div');
  cover.style.cssText = 'position:fixed;inset:0;z-index:2147483647;pointer-events:auto';
  try {
    node.style.visibility = 'hidden';
    if (reached(node)) throw new Error('Hidden forward control passed reachability');
    node.style.visibility = 'visible';
    node.focus({ preventScroll: true });
    document.body.append(cover);
    if (reached(node)) throw new Error('Covered forward control passed reachability');
    cover.remove();
    document.body.append(node);
    node.style.position = 'fixed';
    node.style.top = `${innerHeight + node.getBoundingClientRect().height}px`;
    node.focus({ preventScroll: true });
    node.scrollIntoView({ block: 'center', behavior: 'instant' });
    if (reached(node)) throw new Error('Offscreen fixed forward control passed reachability');
  } finally {
    cover.remove();
    home.insertBefore(node, next);
    if (original == null) node.removeAttribute('style');
    else node.setAttribute('style', original);
  }
};

/** Runs after capture, before the current opening is retired by selection calibration. */
export const checkForwardReachability = async () => {
  const screen = document.documentElement.dataset.screen;
  if (
    ![
      'chat-forward-dialog',
      'chat-forward-loading',
      'chat-forward-empty',
      'chat-forward-error',
      'chat-forward-send-error',
    ].includes(screen)
  )
    return;
  const ids = ['chat-forward-cancel'];
  if (['chat-forward-dialog', 'chat-forward-send-error'].includes(screen))
    ids.push('chat-forward-confirm');
  if (screen === 'chat-forward-error') ids.push('chat-forward-retry');
  const controls = ids.map((id) => {
    const node = document.getElementById(id);
    if (!(node instanceof HTMLButtonElement) || node.disabled || node.hidden)
      throw new Error(`Required forward control is unavailable: ${id}`);
    return node;
  });
  await checkControls(controls);
};

/** Shared prototype-only exposure checks, including deliberately unreachable counterexamples. */
export const checkControls = async (controls) => {
  for (const node of controls) await reveal(node);
  if (controls.length) {
    calibrate(controls[0]);
    await reveal(controls[0]);
  }
};
