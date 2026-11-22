import { checkControls, isVisible } from './forward-reachability.js';

/** Static warning/action reachability; withdrawal and key deletion remain native/core tests. */
export const checkRemoveFixture = async () => {
  const warning = document.querySelector('[data-t="remove.offline"]');
  if (!isVisible(warning) || !warning.textContent.trim() || warning.textContent.startsWith('?'))
    throw new Error('Unconfirmed withdrawal warning is missing');
  const controls = ['remove-cancel-offline', 'remove-retry', 'remove-anyway'].map((id) => {
    const node = document.getElementById(id);
    if (!(node instanceof HTMLButtonElement) || !isVisible(node) || node.disabled)
      throw new Error(`Required removal action is unavailable: ${id}`);
    return node;
  });
  // Focus only during the audit so the shared geometry/hit-test controls can inspect the notice.
  const original = warning.getAttribute('tabindex');
  try {
    warning.setAttribute('tabindex', '-1');
    await checkControls([warning, ...controls]);
  } finally {
    if (original == null) warning.removeAttribute('tabindex');
    else warning.setAttribute('tabindex', original);
  }
};
