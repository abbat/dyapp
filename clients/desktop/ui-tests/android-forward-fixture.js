const check = (value, reason) => {
  if (!value) throw new Error(reason);
};

// Runs after the static snapshot; retiring the opening cannot alter the captured state.
export const checkForwardFixture = () => {
  const get = (id) => {
    const node = document.getElementById(id);
    if (!node) throw new Error('Missing forward fixture: ' + id);
    return node;
  };
  const list = get('chat-forward-list');
  const selected = get('chat-forward-selected-peer');
  const query = get('chat-forward-query');
  const next = get('chat-forward-next');
  const previous = get('chat-forward-previous');
  const radios = () => [...list.querySelectorAll('input[type=radio]')];
  check(radios().length === 32, 'Forward first page must be bounded');
  next.click();
  check(radios().length === 8, 'Forward second page must contain the remaining targets');
  const chosen = radios()[0];
  chosen.click();
  check(selected.textContent === chosen.value, 'Selected full Peer ID must follow the radio');
  previous.click();
  check(!radios().some((radio) => radio.checked), 'Another page retained an old selection');
  next.click();
  check(
    radios().filter((radio) => radio.checked).length === 1 &&
      radios().find((radio) => radio.checked).value === chosen.value,
    'Selection did not survive paging',
  );
  query.value = chosen.value;
  query.dispatchEvent(new Event('input', { bubbles: true }));
  check(radios().length === 1 && radios()[0].checked, 'Full Peer ID search lost the selection');
  query.value = 'not-a-target';
  query.dispatchEvent(new Event('input', { bubbles: true }));
  check(
    radios().length === 0 && selected.textContent === chosen.value,
    'Search changed the chosen target',
  );
  query.value = '';
  query.dispatchEvent(new Event('input', { bubbles: true }));
  const before = selected.textContent;
  const url = location.href;
  const screen = document.documentElement.dataset.screen;
  // Both mutations happen before observer delivery: a same-screen return is a new opening.
  document.documentElement.dataset.screen = 'chat';
  document.documentElement.dataset.screen = screen;
  radios()[0].click();
  get('chat-forward-confirm').click();
  get('chat-forward-cancel').click();
  check(selected.textContent === before, 'Retired radio changed the selected recipient');
  check(location.href === url, 'Retired confirmation navigated');
};
