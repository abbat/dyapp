import { installIdentifiers } from '../../desktop/ui/identifiers.js';
import { installReviewFixture } from './review-fixture.js';
import { installDevicesFixture } from './devices-fixture.js';
import { installViewsFixture } from './views-fixture.js';
import { installUpdateFixture } from './update-fixture.js';
import { installOwnerFixture } from './owner-fixture.js';
import { enhanceChoice } from '../../desktop/ui/profile-choice.js';
import { installCollections } from '../../desktop/prototype/collection-fixture.js';
import { installForwardFixture, forwardScreens } from '../../desktop/prototype/forward-fixture.js';
const screen = document.documentElement.dataset.screen;
const t = STRINGS[document.documentElement.lang];
const contact = document.getElementById('chat-profile');
const contactName = contact?.querySelector('.grow > b');
const targetName = document.getElementById('chat-clear-name');
const targetPeer = document.getElementById('chat-clear-peer');
if (!contactName || !contact.dataset.peerId || !targetName || !targetPeer)
  throw new Error('Clear confirmation target is missing');
targetName.textContent = contactName.textContent;
targetPeer.textContent = contact.dataset.peerId;
if (screen.startsWith('link-review-')) installReviewFixture(t, screen);
installIdentifiers(t);
installCollections(t, ['identities', 'blocked', 'passed', 'devices']);
installDevicesFixture(t, screen.startsWith('owner-') ? 'devices-ready' : screen);
installViewsFixture(t, screen);
installUpdateFixture(t, screen);
installOwnerFixture(t, screen);
if (screen.startsWith('chat') || forwardScreens.includes(screen)) installForwardFixture(t, screen);
enhanceChoice(
  document.querySelector('select[data-choice-picker]'),
  document.querySelector('label[for="profile-goal"]'),
  t['action.cancel'],
  () => 0,
);
const language = document.getElementById('language');
for (const [code, catalogue] of Object.entries(STRINGS)) {
  const name = catalogue['language.name'];
  if (typeof name !== 'string' || !name.trim()) throw new Error('Language catalogue name missing');
  language.append(new Option(name, code));
}
enhanceChoice(
  language,
  document.querySelector('label[for="language"]'),
  t['action.cancel'],
  () => 0,
);
if (screen === 'profile-goal-dialog') document.getElementById('profile-goal-choice').click();
if (screen === 'settings-language-dialog') document.getElementById('language-choice').click();
