import { collections } from '../../desktop/prototype/collection-fixture.js';
import { renderCollectionView } from '../../desktop/ui/settings-collection-view.js';

const readStates = {
  'devices-loading': 'loading',
  'devices-saved': 'saved',
  'devices-ready': 'verified',
  'devices-verified-saved': 'verifiedSaved',
  'devices-offline': 'unavailable',
  'devices-no-profile': 'noProfile',
  'devices-empty': 'empty',
  'devices-error': 'error',
  'devices-confirm': 'verified',
  'devices-revoking': 'verified',
  'devices-uncertain': 'saved',
  'devices-gone': 'saved',
};

/** Static read outcomes only; native admission and profile ordering stay in Rust. */
export const installDevicesFixture = (strings, screen) => {
  if (!screen.startsWith('devices-')) return;
  const read = readStates[screen];
  if (!read) throw new Error('Unknown own-device fixture state');
  const state = collections.get('devices');
  let items = state.items;
  if (['devices-loading', 'devices-empty'].includes(screen)) items = [];
  if (screen === 'devices-error') items = null;
  if (screen === 'devices-gone') items = items.slice(0, 1);
  state.items = items;
  state.reload = async () => items;
  const words = Object.fromEntries(
    ['search', 'previous', 'next', 'refresh', 'failed', 'empty', 'page'].map((key) => [
      key,
      strings[`settings.collection.${key}`],
    ]),
  );
  renderCollectionView(state, words);
  document.getElementById('device-read-status').textContent = strings[`devices.read.${read}`];
  const busy = ['devices-loading', 'devices-revoking'].includes(screen);
  state.refresh.disabled = busy;
  state.search.disabled = busy;
  for (const button of state.list.querySelectorAll('button')) button.disabled = busy;
  if (screen === 'devices-loading') state.status.hidden = true;
};
