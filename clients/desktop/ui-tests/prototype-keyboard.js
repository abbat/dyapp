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
export const plan = () => {
  const enabled = controller.enabledControls();
  const problems = enabled
    .filter(
      (node) =>
        node.tabIndex > 0 ||
        (node.tabIndex < 0 && node.matches('button,input,select,textarea,a[href]')),
    )
    .map((node) => `control is outside DOM tab order: ${controller.selector(node)}`);
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
export const restored = (contract) => {
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
export const beginFixture = async (mode) => {
  fixtureState = document.documentElement.dataset.screen;
  fixtureFocus = document.activeElement;
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
  document.getElementById('keyboard-fixture')?.remove();
  controller.showState(fixtureState);
  if (fixtureFocus?.isConnected) fixtureFocus.focus();
};
export const restoreRadios = (group) => {
  group.names.forEach((name, index) => {
    element(name).checked = index === group.checked;
  });
};
