import * as controller from '../prototype/keyboard.js';

const frames = () =>
  new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
const element = (name) => {
  const found = document.querySelector(name);
  if (!found) throw new Error(`Keyboard control disappeared: ${name}`);
  return found;
};
export const afterKey = async () => {
  await frames();
  return active();
};
export const active = () =>
  document.activeElement === document.body ? '' : controller.selector(document.activeElement);
const checkPhotoQuestion = (problems) => {
  if (document.documentElement.dataset.screen !== 'settings-photos') return;
  const legend = document.querySelector('#photo-mode fieldset > legend');
  if (!legend?.textContent.trim() || !exposed(legend))
    problems.push('photo download choices lack a visible group question');
};
const checkPhotoDisclosure = (problems) => {
  if (!['publish-dialog', 'publish-progress'].includes(document.documentElement.dataset.screen))
    return;
  const tiles = document.querySelectorAll('#profile-photos > .photo-tile');
  const count = document.getElementById('profile-public-photo-count');
  const label = count?.previousElementSibling;
  const expectedLabel = document.querySelector('#profile-photos')?.previousElementSibling;
  if (
    !tiles.length ||
    !count ||
    !exposed(count) ||
    !label ||
    !exposed(label) ||
    !expectedLabel?.textContent.trim() ||
    label.textContent.trim() !== expectedLabel.textContent.trim() ||
    !/^\d+$/.test(count.textContent.trim()) ||
    Number(count.textContent.trim()) !== tiles.length
  )
    problems.push('publication summary does not disclose the fixture photo count');
};
export const plan = () => {
  const enabled = controller.enabledControls();
  const problems = enabled
    .filter(
      (node) =>
        node.tabIndex > 0 ||
        (node.tabIndex < 0 && node.matches('button,input,select,textarea,a[href]')),
    )
    .map((node) => `control is outside DOM tab order: ${controller.selector(node)}`);
  if (document.documentElement.dataset.screen === 'switcher') {
    for (const id of ['identity-switcher', 'switch-to-2', 'add-identity-menu']) {
      const control = document.getElementById(id);
      if (!control || !enabled.includes(control) || !exposed(control))
        problems.push(`switcher state is missing visible #${id}`);
    }
  }
  checkPhotoQuestion(problems);
  checkPhotoDisclosure(problems);
  problems.push(...forwardFailureProblems());
  const dialog = controller.activeDialog();
  const expected = dialog?.dataset.initialFocus;
  if (expected) {
    const target = document.getElementById(expected);
    if (!target || !dialog.contains(target) || document.activeElement !== target)
      problems.push(`initial focus does not match #${expected}`);
  }
  const groups = new Map();
  for (const radio of enabled.filter((node) => node.type === 'radio' && node.name)) {
    const members = enabled.filter(
      (node) => node.type === 'radio' && node.name === radio.name && node.form === radio.form,
    );
    const names = members.map(controller.selector);
    groups.set(names.join('|'), { names, checked: members.findIndex((node) => node.checked) });
  }
  return {
    forward: controller.tabStops().map(controller.selector),
    reverse: controller.tabStops(true).map(controller.selector),
    active: active(),
    dialog: controller.dialogContract(),
    boundary: controller.activeDialog()?.matches('dialog')
      ? controller.selector(controller.activeDialog())
      : '',
    radios: [...groups.values()],
    problems,
  };
};
const bounds = (node) => {
  const rect = node.getBoundingClientRect();
  return {
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
    scale: devicePixelRatio,
  };
};
export const unfocused = async (name) => {
  const node = element(name);
  node.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  node.blur();
  await frames();
  return bounds(node);
};
const paintPadding = (style) => {
  let padding = Math.max(
    0,
    Number.parseFloat(style.outlineWidth) + Number.parseFloat(style.outlineOffset),
  );
  const values = [...style.boxShadow.matchAll(/(-?[0-9.]+)px/g)].map((match) => Number(match[1]));
  for (let i = 0; i < values.length; i += 4)
    padding = Math.max(
      padding,
      Math.abs(values[i]) +
        Math.abs(values[i + 1]) +
        2 * Math.max(0, values[i + 2]) +
        values[i + 3],
    );
  return padding;
};
const exposed = (node) => {
  const rect = node.getBoundingClientRect();
  const left = Math.max(0, rect.left),
    right = Math.min(innerWidth, rect.right);
  const top = Math.max(0, rect.top),
    bottom = Math.min(innerHeight, rect.bottom);
  if (right <= left || bottom <= top) return false;
  const hit = (x, y) => node.contains(document.elementFromPoint(x, y));
  if (hit((left + right) / 2, (top + bottom) / 2)) return true;
  for (let y = top + 0.5 / devicePixelRatio; y < bottom; y += 1 / devicePixelRatio)
    for (let x = left + 0.5 / devicePixelRatio; x < right; x += 1 / devicePixelRatio)
      if (hit(x, y)) return true;
  return false;
};
export const focused = async (name) => {
  const node = element(name);
  node.focus({ preventScroll: true });
  await frames();
  return {
    ...bounds(node),
    padding: paintPadding(getComputedStyle(node)),
    exposed: exposed(node),
    active: active(),
  };
};
const forwardFailureProblems = () => {
  if (document.documentElement.dataset.screen !== 'chat-forward-send-error') return [];
  const visible = (node) => {
    const rect = node.getBoundingClientRect();
    return (
      rect.width > 0 &&
      rect.height > 0 &&
      rect.left >= 0 &&
      rect.top >= 0 &&
      rect.right <= innerWidth &&
      rect.bottom <= innerHeight &&
      exposed(node)
    );
  };
  const feedback = element('#forward-send-failure');
  const nodes = [feedback, element('#chat-forward-cancel'), element('#chat-forward-confirm')];
  return feedback.textContent.trim() && nodes.every(visible)
    ? []
    : ['Forward failure is not visible beside its recovery actions'];
};
let escapeClosed;
export const prepareAdmittedPhase = async () => {
  if (document.documentElement.dataset.screen !== 'chat-forward-pending') return null;
  const unavailable = () =>
    element('#chat-forward-cancel').disabled && element('#chat-forward-confirm').disabled;
  if (!unavailable()) throw new Error('Admitted forwarding still has enabled actions');
  document.dispatchEvent(
    new KeyboardEvent('keydown', {
      key: 'Escape',
      bubbles: true,
      cancelable: true,
    }),
  );
  await frames();
  if (document.documentElement.dataset.screen !== 'chat-forward-pending' || !unavailable())
    throw new Error('Escape dismissed an admitted forward');
  const fixture = await import('../prototype/forward-state.js');
  await fixture.completePendingForKeyboard();
  if (element('#chat-forward-confirm').disabled)
    throw new Error('Controlled forward completion did not enable retry');
  return null;
};

export const prepareEscape = (contract) => {
  const dialog = element(contract.dialog);
  escapeClosed =
    dialog instanceof HTMLDialogElement && dialog.open
      ? new Promise((resolve) => dialog.addEventListener('close', resolve, { once: true }))
      : null;
  return null;
};
export const restored = async (contract) => {
  const closedEvent = escapeClosed;
  if (closedEvent) await closedEvent;
  else await frames();
  if (escapeClosed === closedEvent) escapeClosed = null;
  const dialog = element(contract.dialog);
  const closed =
    !dialog.getClientRects().length || getComputedStyle(dialog).visibility !== 'visible';
  return closed && document.activeElement === element(contract.opener);
};
export const focus = async (name) => {
  element(name).focus();
  await frames();
  return active();
};
let fixtureState;
let fixtureFocus;
let fixtureDialog;
let resumeUrlUpdates;
export const beginFixture = async (mode) => {
  if (resumeUrlUpdates) throw new Error('Keyboard fixture is already active');
  resumeUrlUpdates = controller.suspendUrlUpdates();
  try {
    await mountFixture(mode);
  } catch (error) {
    endFixture();
    throw error;
  }
};
const mountFixture = async (mode) => {
  fixtureState = document.documentElement.dataset.screen;
  fixtureFocus = document.activeElement;
  fixtureDialog = document.querySelector('dialog[open]');
  if (fixtureDialog) {
    const closed = new Promise((resolve) =>
      fixtureDialog.addEventListener('close', resolve, { once: true }),
    );
    fixtureDialog.close();
    await closed;
  }
  controller.showState(mode === 'escape' ? 'revoked-dialog' : 'wizard-identity');
  const fixture = document.createElement('div');
  fixture.id = 'keyboard-fixture';
  fixture.style.cssText =
    'position:fixed;top:0;left:0;z-index:2147483647;background:var(--bg);color:var(--text);padding:16px';
  if (mode === 'escape') fixture.setAttribute('role', 'dialog');
  for (const id of ['first', 'second']) {
    const button = document.createElement('button');
    button.id = `keyboard-fixture-${id}`;
    button.className = 'btn';
    button.textContent = 'Fixture';
    fixture.append(button);
  }
  if (mode === 'focus') fixture.firstElementChild.style.cssText = 'outline:none;box-shadow:none';
  fixture.addEventListener('keydown', (event) => {
    if ((mode === 'trap' && event.key === 'Tab') || (mode === 'escape' && event.key === 'Escape')) {
      event.preventDefault();
      event.stopPropagation();
    }
  });
  document.body.append(fixture);
  fixture.firstElementChild.focus();
  await frames();
};
export const endFixture = () => {
  try {
    document.getElementById('keyboard-fixture')?.remove();
    controller.showState(fixtureState);
    if (fixtureDialog?.isConnected) fixtureDialog.showModal();
    fixtureDialog = null;
    if (fixtureFocus?.isConnected) fixtureFocus.focus();
  } finally {
    resumeUrlUpdates?.();
    resumeUrlUpdates = null;
  }
};
export const checkRenameInitialFocus = async () => {
  await beginFixture('initial-focus');
  try {
    controller.showState('chat-rename-dialog');
    const input = element('#chat-rename-input');
    if (document.activeElement !== input) throw new Error('Rename initial focus is missing');
    input.blur();
    if (!plan().problems.includes('initial focus does not match #chat-rename-input'))
      throw new Error('Missing rename focus escaped the pre-walk audit');
    controller.showState('chat');
    controller.showState('chat-rename-dialog');
    if (document.activeElement !== input) throw new Error('Reopened rename lost initial focus');
  } finally {
    endFixture();
  }
};
export const checkSwitcherVisibilityGuard = async () => {
  await beginFixture('switcher-visibility');
  const sidebar = element('.sidebar');
  const visibility = sidebar.style.visibility;
  try {
    controller.showState('switcher');
    await frames();
    const missing = () =>
      plan().problems.filter((problem) => problem.startsWith('switcher state is missing visible'));
    if (missing().length) throw new Error('Visible switcher actions were rejected');
    // Reproduce the old narrow selector omission without a guessed viewport offset.
    sidebar.style.visibility = 'hidden';
    await frames();
    if (missing().length !== 3) throw new Error('Hidden requested switcher escaped the audit');
  } finally {
    sidebar.style.visibility = visibility;
    endFixture();
  }
};

const checkForwardSourceRestoration = async () => {
  controller.showState('chat');
  await frames();
  const profile = element('#chat-profile');
  const presence = element('#chat-presence');
  const source = [profile.dataset.peerId, profile.textContent, presence.hidden];
  for (const outcome of ['chat-forward-sent', 'chat-forward-queued']) {
    controller.showState(outcome);
    await frames();
    if (!presence.hidden) throw new Error('Forward destination retained source presence');
    controller.showState('chat');
    await frames();
    if (
      profile.dataset.peerId !== source[0] ||
      profile.textContent !== source[1] ||
      presence.hidden !== source[2]
    )
      throw new Error('Forward outcome navigation did not restore the source header');
  }
};

export const checkForwardReopening = async () => {
  await beginFixture('forward-opening');
  try {
    const fixture = await import('../prototype/forward-state.js');
    await checkForwardSourceRestoration();
    controller.showState('chat-forward-dialog');
    await frames();
    const old = fixture.forwardOpening;
    controller.showState('chat');
    controller.showState('chat-forward-dialog');
    await frames();
    const current = fixture.forwardOpening;
    if (old === current || old.current() || !current.current())
      throw new Error('Forward opening was reused across navigation');
    const peer = '2'.padStart(64, '0');
    current.choose(peer, 'Robin 2');
    old.choose('3'.padStart(64, '0'), 'Retired choice');
    if (element('#chat-forward-selected-peer').textContent !== peer)
      throw new Error('Retired forward choice changed the new opening');
    current.state.next.click();
    if (current.state.page !== 1 || old.state.page !== 0)
      throw new Error('Retired forward pagination still receives events');
  } finally {
    endFixture();
  }
};

export const checkForwardFailure = async () => {
  await beginFixture('forward-failure');
  let feedback;
  let next;
  try {
    controller.showState('chat-forward-pending');
    await frames();
    await prepareAdmittedPhase();
    if (forwardFailureProblems().length) {
      const geometry = [
        '#forward-send-failure',
        '#chat-forward-cancel',
        '#chat-forward-confirm',
      ].map((selector) => {
        const node = element(selector);
        const rect = node.getBoundingClientRect();
        const hit = document.elementFromPoint(
          Math.max(0, Math.min(innerWidth - 1, (rect.left + rect.right) / 2)),
          Math.max(0, Math.min(innerHeight - 1, (rect.top + rect.bottom) / 2)),
        );
        return {
          selector,
          rect: rect.toJSON(),
          exposed: exposed(node),
          hit: hit?.id || hit?.className,
        };
      });
      const dialog = element('#chat-forward-dialog');
      throw new Error(
        `Forward completion hid its failure: ${JSON.stringify({ geometry, scrollTop: dialog.scrollTop, scrollHeight: dialog.scrollHeight, clientHeight: dialog.clientHeight, overflow: getComputedStyle(dialog).overflow, dialog: dialog.getBoundingClientRect().toJSON() })}`,
      );
    }
    feedback = element('#forward-send-failure');
    next = feedback.nextSibling;
    if (!next) throw new Error('Forward failure lost its action region');
    const title = element('#chat-forward-title');
    const cancel = element('#chat-forward-cancel');
    title.after(feedback);
    cancel.scrollIntoView({ block: 'nearest', behavior: 'instant' });
    await frames();
    if (!forwardFailureProblems().length)
      throw new Error('Old offscreen forward failure escaped the audit');
    next.before(feedback);
    cancel.scrollIntoView({ block: 'nearest', behavior: 'instant' });
    await frames();
    if (forwardFailureProblems().length) throw new Error('Restored forward failure stayed hidden');
  } finally {
    if (feedback && next) next.before(feedback);
    endFixture();
  }
};

export const restoreRadios = (group) => {
  group.names.forEach((name, index) => {
    element(name).checked = index === group.checked;
  });
};
