import { showState } from '../prototype/keyboard.js';
import { beginFixture, endFixture } from './prototype-keyboard.js';

const check = (ok, message) => {
  if (!ok) throw new Error(message);
};
const frames = () =>
  new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
const restore = (node, style) => {
  if (style === null) node.removeAttribute('style');
  else node.setAttribute('style', style);
};
const declaredStyle = (node) => {
  const parser = document.createElement('span');
  parser.setAttribute('style', node.getAttribute('style') || '');
  return [...parser.style]
    .sort()
    .map((name) => [
      name,
      parser.style.getPropertyValue(name),
      parser.style.getPropertyPriority(name),
    ]);
};
const snapshot = () =>
  JSON.stringify(
    [...document.querySelectorAll('*')].map((node) => [
      declaredStyle(node),
      node.scrollLeft,
      node.scrollTop,
    ]),
  );
const boxes = (menu) =>
  JSON.stringify(
    [menu, ...menu.querySelectorAll('*')].map((node) => {
      const box = node.getBoundingClientRect();
      return [box.left, box.top, box.width, box.height];
    }),
  );
const colorFinding = (rows, target) =>
  rows.some((row) => row.rule.includes('color-contrast') && row.element.includes(target));
const baseline = async ({ audit, menu, head }) => {
  check(audit.popupPaintEligible(menu), 'Plain popup is ineligible');
  const before = boxes(menu);
  const measured = audit.withPopupGeometry(() => [head.clientHeight, head.scrollHeight]);
  check(measured[0] === measured[1], 'Visible overlay inflated its header');
  check(boxes(menu) === before, 'Popup normalization moved content');
  check(
    !(await audit.accessibility()).some((row) => row.rule.includes('color-contrast')),
    'Readable popup or covered background remained unmeasured',
  );
};
const foreground = async ({ audit, menu, label }) => {
  const saved = label.getAttribute('style');
  try {
    label.style.setProperty('color', getComputedStyle(menu).backgroundColor, 'important');
    check(colorFinding(await audit.accessibility(), label.id), 'Poor popup foreground was passed');
  } finally {
    restore(label, saved);
  }
};
const background = async ({ audit }) => {
  const day = document.querySelector('.day'),
    saved = day.getAttribute('style');
  try {
    day.style.cssText += ';background:white!important;color:white!important';
    check(colorFinding(await audit.accessibility(), '.day'), 'Poor covered background was passed');
  } finally {
    restore(day, saved);
  }
};
const cover = (menu, layer) => {
  const box = menu.getBoundingClientRect(),
    node = document.createElement('div');
  node.style.cssText = 'position:fixed;background:white';
  node.style.zIndex = String(layer);
  for (const key of ['left', 'top', 'width', 'height'])
    node.style.setProperty(key, `${box[key]}px`);
  document.body.append(node);
  return node;
};
const higher = async ({ audit, menu }, pointer) => {
  const node = cover(menu, Number(getComputedStyle(menu).zIndex) + 1);
  const run = window.axe.run;
  let isolated = false;
  try {
    node.style.pointerEvents = pointer;
    check(!audit.popupPaintEligible(menu), 'Higher paint was hidden by popup isolation');
    window.axe.run = (context, options) => {
      if (context === menu || menu.style.opacity === '0') isolated = true;
      return run.call(window.axe, context, options);
    };
    await audit.accessibility();
    check(!isolated, 'Ineligible popup entered paint isolation');
  } finally {
    window.axe.run = run;
    node.remove();
  }
};
const layer = ({ audit, menu, label }) => {
  const bar = document.querySelector('.topbar');
  if (!bar.getClientRects().length) return;
  const barLayer = Number(getComputedStyle(bar).zIndex);
  check(Number.isFinite(barLayer), 'Layer calibration needs the visible application bar');
  const node = cover(menu, barLayer),
    saved = menu.getAttribute('style');
  const box = label.getBoundingClientRect();
  const point = [box.left + box.width / 2, box.top + box.height / 2];
  try {
    check(menu.contains(document.elementFromPoint(...point)), 'Popup is below the application bar');
    menu.style.zIndex = String(barLayer - 1);
    check(document.elementFromPoint(...point) === node, 'Old lower layer did not expose occlusion');
    check(!audit.popupPaintEligible(menu), 'Old lower popup layer escaped the guard');
  } finally {
    restore(menu, saved);
    node.remove();
  }
};
const translucent = ({ audit, menu }) => {
  const saved = menu.getAttribute('style');
  try {
    menu.style.opacity = '0.5';
    check(!audit.popupPaintEligible(menu), 'Translucent popup entered opaque paint isolation');
  } finally {
    restore(menu, saved);
  }
};
const clipped = ({ audit, menu, head }) => {
  const saved = head.getAttribute('style');
  try {
    head.style.overflow = 'hidden';
    check(!audit.popupPaintEligible(menu), 'Clipping ancestor escaped the paint guard');
    check(
      audit.withPopupGeometry(() => getComputedStyle(menu).position) === 'absolute',
      'Clipped popup was lifted out of its ancestor',
    );
    check(
      audit
        .textClipping()
        .some((row) => row.rule === 'T1:clipping' && document.querySelector(row.element) === head),
      'Clipped popup escaped text measurement',
    );
  } finally {
    restore(head, saved);
  }
};
const outside = ({ audit, menu }) => {
  const saved = menu.getAttribute('style');
  try {
    menu.style.left = `${document.documentElement.clientWidth}px`;
    menu.style.right = 'auto';
    check(!audit.popupPaintEligible(menu), 'Outside popup entered isolation');
    check(
      audit.textClipping().some((row) => row.rule === 'T1:horizontal-scroll'),
      'Outside popup escaped viewport measurement',
    );
  } finally {
    restore(menu, saved);
  }
};
const labelClipping = ({ audit, label }) => {
  const saved = label.getAttribute('style');
  try {
    label.style.whiteSpace = 'nowrap';
    label.style.width = '2ch';
    label.style.overflow = 'hidden';
    check(
      audit.textClipping().some((row) => document.querySelector(row.element) === label),
      'Real foreground clipping escaped the audit',
    );
  } finally {
    restore(label, saved);
  }
};
const measurementError = ({ audit, menu }) => {
  const saved = menu.getAttribute('style');
  let caught = false;
  try {
    audit.withPopupGeometry(() => {
      throw new Error('Forced measurement');
    });
  } catch (error) {
    caught = error.message === 'Forced measurement';
  }
  check(caught && menu.getAttribute('style') === saved, 'Measurement error changed popup styles');
};
const pinError = ({ audit, menu }) => {
  const descriptor = Object.getOwnPropertyDescriptor(menu, 'getBoundingClientRect');
  const original = menu.getBoundingClientRect,
    saved = menu.getAttribute('style');
  try {
    menu.getBoundingClientRect = () => {
      if (menu.style.position === 'fixed') throw new Error('Forced pin');
      return original.call(menu);
    };
    let caught = false;
    try {
      audit.withPopupGeometry(() => false);
    } catch (error) {
      caught = error.message === 'Forced pin';
    }
    check(caught && menu.getAttribute('style') === saved, 'Pin error changed popup styles');
  } finally {
    if (descriptor) Object.defineProperty(menu, 'getBoundingClientRect', descriptor);
    else delete menu.getBoundingClientRect;
  }
};
const priority = async ({ audit, menu }) => {
  const saved = menu.getAttribute('style');
  try {
    menu.style.setProperty('opacity', '1', 'important');
    check(
      menu.style.getPropertyPriority('opacity') === 'important',
      'Priority fixture was not set',
    );
    const before = snapshot();
    await audit.accessibility();
    check(snapshot() === before, 'Paint audit changed an important declaration');
    menu.style.removeProperty('opacity');
    menu.style.setProperty('opacity', '1');
    check(menu.style.getPropertyPriority('opacity') === '', 'Priority fixture was not cleared');
    check(snapshot() !== before, 'Style comparison ignored a changed declaration priority');
  } finally {
    restore(menu, saved);
  }
};
const paintError = async ({ audit, menu }) => {
  const run = window.axe.run;
  let caught = false;
  try {
    window.axe.run = (context, options) =>
      context === menu || menu.style.opacity === '0'
        ? Promise.reject(new Error('Forced paint'))
        : run.call(window.axe, context, options);
    try {
      await audit.accessibility();
    } catch (error) {
      caught = error.message === 'Forced paint';
    }
    check(caught, 'Paint error calibration never reached the recheck');
  } finally {
    window.axe.run = run;
  }
};
const targetReport = (target, kind) => ({
  violations: [],
  passes: [],
  incomplete: [],
  [kind]: [
    {
      id: 'color-contrast',
      nodes: [
        {
          target: [`#${target.id}`],
          any: kind === 'incomplete' ? [{ data: { messageKey: 'bgOverlap' } }] : [],
        },
      ],
    },
  ],
});
const backgroundTarget = (menu, id, top) => {
  const node = document.createElement('span'),
    box = menu.getBoundingClientRect();
  node.id = id;
  node.textContent = 'Calibration';
  node.style.cssText = 'position:fixed';
  node.style.left = `${box.left + box.width / 4}px`;
  node.style.top = `${top}px`;
  document.body.append(node);
  return node;
};
const uncoveredTarget = async ({ audit, menu, label }) => {
  const node = backgroundTarget(
    menu,
    'popup-uncovered-target',
    menu.getBoundingClientRect().bottom + label.getBoundingClientRect().height / 2,
  );
  const run = window.axe.run;
  let initial = true,
    rechecked = false;
  try {
    check(!audit.popupCoversText(menu, node), 'Outside-text fixture overlaps the popup body');
    window.axe.run = (context) => {
      if (context === document) {
        if (!initial) {
          rechecked = true;
          return Promise.resolve(targetReport(node, 'passes'));
        }
        initial = false;
      }
      return Promise.resolve(targetReport(node, 'incomplete'));
    };
    check(
      !rechecked && colorFinding(await audit.accessibility(), node.id),
      'An uncovered target was cleared through popup paint isolation',
    );
    check(!rechecked, 'An uncovered target entered popup isolation');
  } finally {
    window.axe.run = run;
    node.remove();
  }
};
const replacedTarget = async ({ audit, menu, label }) => {
  const node = backgroundTarget(
    menu,
    'popup-replaced-target',
    menu.getBoundingClientRect().top + label.getBoundingClientRect().height / 2,
  );
  const replacement = node.cloneNode(true),
    run = window.axe.run;
  let initial = true,
    replaced = false;
  try {
    check(audit.popupCoversText(menu, node), 'Replacement fixture is not covered');
    window.axe.run = () => {
      if (initial) {
        initial = false;
        return Promise.resolve(targetReport(node, 'incomplete'));
      }
      if (!replaced) {
        node.replaceWith(replacement);
        replaced = true;
      }
      return Promise.resolve(targetReport(replacement, 'passes'));
    };
    const result = await audit.accessibility();
    check(
      replaced && colorFinding(result, node.id),
      'A replacement node cleared an earlier node contrast result',
    );
  } finally {
    window.axe.run = run;
    node.remove();
    replacement.remove();
  }
};
const cases = {
  baseline,
  foreground,
  background,
  layer,
  translucent,
  clipped,
  outside,
  'higher-auto': (fixture) => higher(fixture, 'auto'),
  'higher-none': (fixture) => higher(fixture, 'none'),
  'label-clipping': labelClipping,
  'measurement-error': measurementError,
  'pin-error': pinError,
  priority,
  'paint-error': paintError,
  'uncovered-target': uncoveredTarget,
  'replaced-target': replacedTarget,
};
export const runPopupCase = async (audit, name) => {
  const position = [scrollX, scrollY];
  await beginFixture('popup');
  try {
    document.getElementById('keyboard-fixture').remove();
    showState('chat-menu');
    await document.fonts.ready;
    await frames();
    const menu = document.getElementById('chat-menu'),
      before = snapshot();
    const focus = document.activeElement,
      scroll = [scrollX, scrollY];
    await cases[name]({
      audit,
      menu,
      head: menu.parentElement,
      label: menu.querySelector('button'),
    });
    check(snapshot() === before, 'Popup calibration did not restore declared styles and scrolling');
    check(
      document.activeElement === focus && scrollX === scroll[0] && scrollY === scroll[1],
      'Popup calibration did not restore focus or page scrolling',
    );
  } finally {
    endFixture();
    window.scrollTo({ left: position[0], top: position[1], behavior: 'instant' });
    await frames();
  }
};
