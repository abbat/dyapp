import { byId } from '../ui/dom.js';
import { forwardScreens, installForwardFixture } from './forward-fixture.js';
import { showState } from './keyboard.js';

const profile = byId('chat-profile', HTMLElement);
const heading = profile.querySelector('.grow > b');
const avatar = profile.querySelector('.avatar.p2');
const menu = byId('msg-menu', HTMLElement);
const source = {
  peer: profile.dataset.peerId,
  name: heading.textContent,
  avatarClass: avatar.className,
  avatarText: avatar.textContent,
  presenceHidden: byId('chat-presence', HTMLElement).hidden,
  home: menu.parentElement,
};
export let forwardOpening;
let completed;
const restoreSource = () => {
  profile.dataset.peerId = source.peer;
  heading.textContent = source.name;
  avatar.className = source.avatarClass;
  avatar.textContent = source.avatarText;
  byId('chat-presence', HTMLElement).hidden = source.presenceHidden;
  source.home.append(menu);
  byId('msg-edit', HTMLButtonElement).hidden = false;
  byId('msg-delete', HTMLButtonElement).hidden = false;
};
const mount = () => {
  const focused = document.activeElement;
  forwardOpening?.retire();
  restoreSource();
  const screen = document.documentElement.dataset.screen;
  if (screen.startsWith('chat') || forwardScreens.includes(screen))
    forwardOpening = installForwardFixture(window.strings, screen);
  if (menu.contains(focused)) focused.focus();
  if (screen === 'chat-forward-send-error') {
    byId('chat-forward-cancel', HTMLButtonElement).focus();
    // Reveal the final recovery region in its owning scroll container after rendering.
    const dialog = byId('chat-forward-dialog', HTMLElement);
    const opening = forwardOpening;
    requestAnimationFrame(() => {
      if (!opening.current()) return;
      dialog.scrollTop = dialog.scrollHeight - dialog.clientHeight;
      if (completed) {
        const resolve = completed;
        completed = null;
        resolve();
      }
    });
  }
};
const navigation = new MutationObserver(mount);
navigation.observe(document.documentElement, {
  attributes: true,
  attributeFilter: ['data-screen'],
});
byId('received-actions', HTMLButtonElement).addEventListener('click', () =>
  showState('chat-forward-received-menu'),
);
mount();

// The audit acknowledges a controlled failed completion; it never cancels admitted work.
export const completePendingForKeyboard = () =>
  new Promise((resolve, reject) => {
    if (
      document.documentElement.dataset.screen !== 'chat-forward-pending' ||
      !byId('chat-forward-cancel', HTMLButtonElement).disabled
    ) {
      reject(new Error('Forwarding is not awaiting an admitted completion'));
      return;
    }
    const url = new URL(location.href);
    url.searchParams.set('forwardTo', byId('chat-forward-selected-peer', HTMLElement).textContent);
    history.replaceState(null, '', url);
    completed = resolve;
    showState('chat-forward-send-error');
  });
