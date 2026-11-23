import { searchRecoveryCard } from '../ui/search-recovery.js';

const failures = {
  'read-error': 'failed',
  'review-required': 'reviewRequired',
  'link-pending': 'linkPending',
  'version-exhausted': 'versionExhausted',
  'wrong-owner': 'wrongOwner',
};
const saved = { version: 8, hidden: true, searchPending: true };
const prepare = (scene) =>
  failures[scene] ? Promise.reject({ kind: failures[scene] }) : Promise.resolve('original-review');
const confirm = (scene) => {
  if (scene === 'busy') return new Promise(() => {});
  if (scene === 'stale') return Promise.reject({ kind: 'changed' });
  if (scene === 'confirm-error') return Promise.reject({ kind: 'failed' });
  return Promise.resolve(scene === 'sent' ? { ...saved, searchPending: false } : saved);
};
const callFor = (scene) => async (command) => {
  if (command === 'dismiss_search_recovery') return;
  if (command === 'prepare_search_recovery') return prepare(scene);
  if (command === 'confirm_search_recovery') return confirm(scene);
  if (command === 'retry_search_recovery') return Promise.reject({ kind: 'failed' });
  throw new Error(`Unexpected search recovery command: ${command}`);
};
const showScene = async (scene) => {
  const root = document.getElementById('profile-search-recovery');
  const card = searchRecoveryCard(
    root,
    '0123456789abcdef'.repeat(4),
    callFor(scene),
    () => true,
    window.strings,
  );
  await card.prepare();
  if (failures[scene] || scene === 'review') return;
  const confirmed = card.model.confirm(card.model.state.opening);
  if (scene === 'busy') {
    if (!card.model.state.busy) throw new Error('Search recovery confirmation was not admitted');
    return;
  }
  await confirmed;
  if (scene === 'retry-error') await card.model.retry(card.model.state.opening);
};
export const showSearchRecoveryFixture = () => {
  const screen = document.documentElement.dataset.screen;
  if (!screen.startsWith('profile-recovery-')) return;
  const scene = screen.slice('profile-recovery-'.length);
  const scenes =
    'review busy pending sent read-error confirm-error retry-error stale review-required link-pending version-exhausted wrong-owner'.split(
      ' ',
    );
  if (!scenes.includes(scene)) throw new Error(`Unknown search recovery scene: ${scene}`);
  return showScene(scene);
};
