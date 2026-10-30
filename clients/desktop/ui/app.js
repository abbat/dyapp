// Practice D1: visible strings come from the matching i18n catalogue by data-i18n key.
// With --pseudo-locale each English string is accented and 40% longer.
const { invoke } = window.__TAURI__.core;
const accents = { a: 'á', e: 'é', i: 'í', o: 'ó', u: 'ú', y: 'ý', A: 'Á', E: 'É', I: 'Í', O: 'Ó', U: 'Ú' };
const pseudo = (text) => `[${[...text].map((c) => accents[c] || c).join('')}${'·'.repeat(Math.ceil(text.length * 0.4))}]`;

window.stringsShown = Promise.all([invoke('pseudo_locale'), invoke('ui_test_russian')])
  .then(async ([pseudoLocale, testRussian]) => {
    const systemLanguage = (navigator.languages?.[0] || navigator.language || 'en')
      .split(/[-_]/)[0].toLowerCase();
    // Settings → Language overrides the system unless a test or the pseudo-locale sets it.
    const chosen = localStorage.getItem('language');
    const wanted = chosen === 'en' || chosen === 'ru' ? chosen : systemLanguage;
    const language = !pseudoLocale && (testRussian || wanted === 'ru') ? 'ru' : 'en';
    const response = await fetch(`i18n/${language}.json`);
    if (!response.ok) throw new Error('Translation catalogue unavailable');
    const catalogue = await response.json();
    window.locale = language;
    window.pseudoLocale = pseudoLocale;
    document.documentElement.lang = language;
    window.strings = Object.fromEntries(Object.entries(catalogue)
      .map(([key, text]) => [key, pseudoLocale ? pseudo(text) : text]));
    for (const element of document.querySelectorAll('[data-i18n]')) {
      element.textContent = window.strings[element.dataset.i18n];
    }
    // Icon-only buttons in the rail still have a name and a tooltip.
    for (const element of document.querySelectorAll('[data-i18n-label]')) {
      element.setAttribute('aria-label', window.strings[element.dataset.i18nLabel]);
      if (element.tagName === 'BUTTON') element.title = window.strings[element.dataset.i18nLabel];
    }
    for (const element of document.querySelectorAll('[data-i18n-placeholder]')) {
      element.placeholder = window.strings[element.dataset.i18nPlaceholder];
    }
  });
const ready = document.getElementById('app-ready');
const setup = document.getElementById('setup');
const identity = document.getElementById('identity');
const systemChoice = document.getElementById('system-choice');
const fileChoice = document.getElementById('file-choice');
const createIdentity = document.getElementById('create-identity');
const storageError = document.getElementById('storage-error');

const switchIdentity = document.getElementById('switch-identity');
const removeIdentity = document.getElementById('remove-identity');
const removeConfirm = document.getElementById('remove-confirm');
const removeAction = document.getElementById('remove-action');
const removeDialog = document.getElementById('remove');
const wizardNetwork = document.getElementById('wizard-network');
const sections = [setup, identity, document.getElementById('picker'),
  document.getElementById('recovery'), wizardNetwork];
// The opened identity or recovery state the remove dialog acts on.
let current = null;

// Shows one section (or none, the boot page) with its status line.
const show = (section, status) => {
  if (removeDialog.open) removeDialog.close();
  document.getElementById('boot').hidden = Boolean(section);
  for (const each of sections) each.hidden = each !== section;
  // Ready keeps identity actions in Settings; recovery has no other way out.
  switchIdentity.hidden = section !== sections[3];
  // The backend refuses to remove a damaged or unreadable slot.
  removeIdentity.hidden = !(section === sections[3]
    && ['missing', 'incomplete', 'deleting'].includes(current?.reason));
  // Setup and the picker opened from an open identity can go back to it.
  for (const id of ['setup-cancel', 'picker-cancel']) document.getElementById(id).hidden = !current;
  storageError.hidden = true;
  ready.textContent = window.strings[status];
};

const showError = () => {
  show(null, 'status.error');
  switchIdentity.hidden = true;
  document.getElementById('boot-loading').hidden = true;
  storageError.textContent = window.strings['storage.error'];
  storageError.hidden = false;
};

const reasonText = (reason) => window.strings[`recovery.${reason}`]
  || window.strings['recovery.unavailable'];

// Copies `text()`, by default the text of `source`; where the webview refuses, shows it in
// `source` and selects it for Ctrl+C instead.
const copyFrom = (button, source, text = () => source.textContent) => button.addEventListener('click', () => {
  const value = text();
  window.copied = Promise.resolve(value).then((copy) => navigator.clipboard.writeText(copy)).then(() => {
    button.textContent = window.strings['action.copied'];
    setTimeout(() => { button.textContent = window.strings['action.copy']; }, 2000);
  }, () => {
    source.textContent = value;
    window.getSelection().selectAllChildren(source);
  });
});

const act = async (pending) => {
  try {
    return await render(await pending);
  } catch (_) {
    showError();
    return { status: 'error' };
  }
};

// A git-style short Peer ID: "@" and the last 6 characters, longer until no other known ID
// shares the tail. It helps to recognize a peer and is no check; the full ID stays in the title.
const shortPeerId = (id, known) => {
  let length = Math.min(6, id.length);
  while (length < id.length && known.some((other) => other !== id && other.endsWith(id.slice(-length)))) {
    length += 1;
  }
  return `@${id.slice(-length)}`;
};
window.shortPeerId = shortPeerId;
// The local identities are the only known peers until contacts exist.
let knownPeers = [];
const knowPeers = (items) => {
  knownPeers = items.filter((item) => item.peerId).map((item) => item.peerId);
  return items;
};
const peerLabel = (element, id) => {
  Object.assign(element.dataset, { peerId: id });
  element.title = id;
  return shortPeerId(id, knownPeers);
};

// An identity as an avatar, its short ID (or why it cannot open) and a caption under it.
const who = (element, id, reason, caption) => {
  const avatar = document.createElement('span');
  avatar.className = 'avatar';
  avatar.setAttribute('aria-hidden', 'true');
  const name = document.createElement(id ? 'code' : 'span');
  name.textContent = id ? peerLabel(element, id) : reasonText(reason);
  avatar.textContent = id ? name.textContent.slice(1, 3) : '!';
  const note = document.createElement('span');
  note.className = 'caption';
  note.textContent = caption;
  const text = document.createElement('span');
  text.append(name, document.createElement('br'), note);
  return [avatar, text];
};
const modeText = (item) => window.strings[`identity.mode.${item.mode}`] || '';

const identityButton = (item) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'list-button';
  Object.assign(button.dataset, { status: item.status, mode: item.mode || '' });
  button.append(...who(button, item.status === 'ready' && item.peerId, item.reason, modeText(item)));
  button.addEventListener('click', () => act(invoke('select_identity', { slot: item.slot })));
  return button;
};

// Remove applies to the open identity only, so it sits in that row.
const settingsRemove = document.getElementById('settings-remove');
const settingsIdentity = (item) => {
  const row = document.createElement('tr');
  const name = document.createElement('td');
  name.className = 'who';
  name.append(...who(row, item.status === 'ready' && item.peerId, item.reason, modeText(item)));
  const active = document.createElement('td');
  active.className = 'caption';
  active.textContent = item.slot === current.slot ? window.strings['settings.active'] : '';
  if (item.slot === current.slot) active.append(document.createElement('br'), settingsRemove);
  row.append(name, active);
  row.dataset.active = String(item.slot === current.slot);
  return row;
};

// Settings → Storage: binary units with one decimal; an unknown number reads Unavailable, not 0.
const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
const sizeText = (bytes) => {
  let unit = 0;
  let value = bytes;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit ? 1 : 0;
  const number = new Intl.NumberFormat(window.locale, {
    minimumFractionDigits: digits, maximumFractionDigits: digits,
  }).format(value);
  return `${number} ${window.strings[`usage.unit.${units[unit]}`]}`;
};
const knownSize = (bytes) => (bytes === null ? window.strings['usage.unavailable'] : sizeText(bytes));
const usageText = (id, text) => { document.getElementById(id).textContent = text; };
let usageTimer;
const showUsage = async () => {
  clearTimeout(usageTimer);
  const card = document.getElementById('usage');
  if (document.getElementById('settings').hidden) return;
  try {
    const usage = await invoke('storage_usage');
    usageText('usage-identity-dir', usage.identityDir ?? window.strings['usage.unavailable']);
    usageText('usage-messages', knownSize(usage.messages));
    usageText('usage-messages-path', usage.messagesPath);
    document.getElementById('usage-node-row').hidden = usage.nodePath === null;
    const limited = usage.node !== null && usage.nodeLimit !== null;
    usageText('usage-node', limited ? window.strings['usage.of']
      .replace('{used}', sizeText(usage.node)).replace('{limit}', sizeText(usage.nodeLimit))
      : knownSize(usage.node));
    usageText('usage-node-path', usage.nodePath ?? '');
    const bar = document.getElementById('usage-node-bar');
    bar.hidden = !limited;
    if (limited) Object.assign(bar, { max: usage.nodeLimit, value: usage.node });
    document.getElementById('usage-free-row').hidden = usage.free === null;
    if (usage.free !== null) usageText('usage-free', sizeText(usage.free));
    card.hidden = false;
  } catch (_) {
    card.hidden = true;
  }
  usageTimer = setTimeout(showUsage, 10_000);
};

// The status bar: client traffic every 2 s, unavailable until a client swarm starts.
let statusTimer;
const showStatus = async () => {
  clearTimeout(statusTimer);
  const { strings } = window;
  const traffic = document.getElementById('status-traffic');
  const messages = document.getElementById('status-messages');
  try {
    const status = await invoke('status');
    traffic.textContent = status.traffic === null ? strings['statusbar.noTraffic']
      : strings['statusbar.traffic'].replace('{in}', sizeText(status.traffic[0]))
        .replace('{out}', sizeText(status.traffic[1]));
    messages.textContent = status.messages === null ? strings['statusbar.noMessages']
      : strings['statusbar.messages'].replace('{size}', sizeText(status.messages));
  } catch (_) {
    traffic.textContent = strings['statusbar.noTraffic'];
    messages.textContent = strings['statusbar.noMessages'];
  }
  traffic.title = strings['statusbar.trafficNote'];
  statusTimer = setTimeout(showStatus, 2000);
};

// Settings → Window: the tray icon and close-to-tray, usable only while a tray host is running.
const trayShow = document.getElementById('tray-show');
const trayClose = document.getElementById('tray-close');
const setTray = async (prefs) => {
  const labels = [window.strings['tray.open'], window.strings['tray.quit']];
  const tray = await invoke('set_tray', { prefs, labels });
  trayShow.checked = tray.show;
  trayShow.disabled = !tray.available;
  trayClose.checked = tray.closeToTray;
  trayClose.disabled = !tray.available || !tray.show;
  document.getElementById('tray-note').hidden = tray.available;
  return tray;
};
const changeTray = () => setTray({ show: trayShow.checked, closeToTray: trayClose.checked });
trayShow.addEventListener('change', changeTray);
trayClose.addEventListener('change', changeTray);
window.trayShown = window.stringsShown.then(() => setTray(null));

// Settings → Appearance: kept by this app for every identity; a language change reloads the page.
const applyTheme = (theme) => {
  if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
};
applyTheme(localStorage.getItem('theme'));
for (const radio of document.querySelectorAll('input[name="theme"]')) {
  radio.checked = radio.value === (localStorage.getItem('theme') || 'system');
  radio.addEventListener('change', () => {
    localStorage.setItem('theme', radio.value);
    applyTheme(radio.value);
  });
}
const languageChoice = document.getElementById('language');
languageChoice.value = localStorage.getItem('language') || 'system';
languageChoice.addEventListener('change', () => {
  localStorage.setItem('language', languageChoice.value);
  window.location.reload();
});

// Settings → Privacy: stored per identity on this device, saved on every change.
const privacyInputs = {
  sendViews: 'send-views',
  sendReadReceipts: 'send-read-receipts',
  sendTyping: 'send-typing',
  presenceMatches: 'presence-matches',
  notifications: 'message-notifications',
};
const privacyError = document.getElementById('privacy-error');
const showPrivacy = async () => {
  try {
    const privacy = await invoke('privacy_settings');
    for (const [key, id] of Object.entries(privacyInputs)) {
      document.getElementById(id).checked = privacy[key];
    }
    document.getElementById('presence-nobody').checked = !privacy.presenceMatches;
    privacyError.hidden = true;
  } catch (_) {
    privacyError.hidden = false;
  }
};
document.getElementById('privacy').addEventListener('change', () => {
  const privacy = Object.fromEntries(Object.entries(privacyInputs)
    .map(([key, id]) => [key, document.getElementById(id).checked]));
  window.privacySaved = invoke('set_privacy', { privacy })
    .then(() => { privacyError.hidden = true; }, () => { privacyError.hidden = false; });
});

// Settings → Blocked people: Unblock shows the peer in later searches again.
const showBlocked = async () => {
  const list = document.getElementById('blocked-list');
  let peers = [];
  try {
    peers = await invoke('blocked_peers');
    privacyError.hidden = true;
  } catch (_) {
    privacyError.hidden = false;
  }
  list.replaceChildren(...peers.map((peerId, index) => {
    const row = document.createElement('tr');
    const name = document.createElement('td');
    name.className = 'who';
    name.append(...who(row, peerId, null, ''));
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    button.id = `unblock-${index + 1}`;
    button.textContent = window.strings['settings.unblock'];
    button.addEventListener('click', () => {
      button.disabled = true;
      window.blockedShown = invoke('unblock_peer', { peerId })
        .catch(() => { privacyError.hidden = false; })
        .then(showBlocked);
    });
    const action = document.createElement('td');
    action.append(button);
    row.append(name, action);
    return row;
  }));
  document.getElementById('blocked-none').hidden = peers.length > 0;
};

const showSettings = async () => {
  const list = document.getElementById('settings-identities');
  const error = document.getElementById('settings-error');
  // Remove waits under the error until a row for the open identity takes it.
  error.after(settingsRemove);
  list.replaceChildren();
  error.hidden = true;
  window.nodeShown = showNode();
  window.usageShown = showUsage();
  window.trayShown = setTray(null);
  window.privacyShown = showPrivacy();
  window.blockedShown = showBlocked();
  try {
    const items = await invoke('list_identities');
    list.replaceChildren(...knowPeers(items).map(settingsIdentity));
    error.hidden = true;
  } catch (_) {
    list.replaceChildren();
    error.textContent = window.strings['storage.error'];
    error.hidden = false;
  }
};

// The shell: one area at a time; below 640 px the sidebar slides in over a scrim.
const menuOpen = document.getElementById('menu-open');
const setMenu = (open) => {
  if (open) document.documentElement.dataset.screen = 'menu';
  else delete document.documentElement.dataset.screen;
  menuOpen.setAttribute('aria-expanded', String(open));
};
menuOpen.addEventListener('click', () => setMenu(document.documentElement.dataset.screen !== 'menu'));
document.getElementById('scrim').addEventListener('click', () => setMenu(false));
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') setMenu(false);
  if (event.ctrlKey && event.key.toLowerCase() === 'q') invoke('quit');
});
const showArea = (area) => {
  for (const button of document.querySelectorAll('#sidebar [data-area]')) {
    document.getElementById(button.dataset.area).hidden = button.dataset.area !== area;
    if (button.dataset.area === area) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  }
  setMenu(false);
  if (area === 'settings') showSettings();
};
for (const button of document.querySelectorAll('#sidebar [data-area]')) {
  button.addEventListener('click', () => showArea(button.dataset.area));
}
const settingsCreate = document.getElementById('settings-create');
const settingsSwitch = document.getElementById('settings-switch');
settingsCreate.addEventListener('click', () => act(invoke('storage_options')));
settingsSwitch.addEventListener('click', () => act({ status: 'picker' }));

// The store and search node the app runs while open (ADR 0030); Windows builds have no node
// commands, so the form stays hidden there.
const nodeForm = document.getElementById('node');
const nodeEnabled = document.getElementById('node-enabled');
const nodeDisk = document.getElementById('node-disk');
const nodeConnections = document.getElementById('node-connections');
const nodeSave = document.getElementById('node-save');
const nodeStatus = (status, detail) => {
  const text = window.strings[`node.${status}`];
  document.getElementById('node-status').textContent = detail ? `${text} ${detail}` : text;
  nodeForm.dataset.status = status;
};
const nodeResult = (result) => nodeStatus(result.status, result.peerId);
const showNode = async () => {
  try {
    nodeResult(await invoke('node_status'));
  } catch (_) {
    nodeForm.hidden = true;
    return;
  }
  nodeForm.hidden = false;
  try {
    const saved = await invoke('load_node_settings');
    nodeEnabled.checked = saved.enabled;
    nodeDisk.value = saved.diskMb;
    nodeConnections.value = saved.maxConnections;
  } catch (error) {
    nodeStatus('error', String(error));
  }
};
nodeForm.addEventListener('submit', (event) => {
  event.preventDefault();
  const node = {
    enabled: nodeEnabled.checked,
    diskMb: Number(nodeDisk.value),
    maxConnections: Number(nodeConnections.value),
  };
  if (!Number.isInteger(node.diskMb) || node.diskMb < 4
    || !Number.isInteger(node.maxConnections) || node.maxConnections < 1) {
    nodeStatus('invalidLimits');
    return;
  }
  nodeSave.disabled = true;
  nodeStatus(node.enabled ? 'starting' : 'off');
  window.nodeApplied = invoke('save_node_settings', { settings: node })
    .then(nodeResult)
    .catch((error) => nodeStatus('error', String(error)))
    .finally(() => { nodeSave.disabled = false; });
});

// The Rust core supplies field numbers and choices; catalogues supply only visible names.
const profileForm = document.getElementById('profile');
const profileScales = document.getElementById('profile-scales');
const profileStatus = document.getElementById('profile-status');
const publishDialog = document.getElementById('publish-dialog');
const publishConfirm = document.getElementById('profile-publish-confirm');
const profileHidden = document.getElementById('profile-hidden');
const profileAttested = document.getElementById('profile-attested');
const profileDisclosure = document.getElementById('profile-disclosure');
const profileCancel = document.getElementById('profile-cancel');
let scaleFields = [];
let profileBusy = false;
let profilePublished = false;
const camel = (key) => key.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
const birthValue = (part) => {
  const value = document.getElementById(`birth-${part}`).value;
  return value === '' ? null : Number(value);
};
const placeLabel = (place) => (place.region ? `${place.name}, ${place.region}` : place.name);
const placeText = (country, id) => (id ? invoke('place_name', {
  country, id: Number(id), lang: window.locale,
}).then((place) => (place ? placeLabel(place) : id)) : Promise.resolve(''));
const countryName = (code) => new Intl.DisplayNames([window.locale], { type: 'region' }).of(code);
// The place input holds a suggestion's text; ids maps each shown suggestion to its GeoNames ID,
// so typed text that is no suggestion fails validation instead of being saved or searched.
// Editing the country clears the place.
const placePicker = (countryInput, input, list, country) => {
  let ids = new Map();
  const show = (places) => {
    ids = new Map(places.map((place) => [placeLabel(place), String(place.id)]));
    list.replaceChildren(...[...ids.keys()].map((label) => {
      const option = document.createElement('option');
      option.value = label;
      return option;
    }));
  };
  const check = () => input.setCustomValidity(
    input.value && !ids.has(input.value) ? window.strings['profile.placeUnknown'] : '');
  const set = (place) => {
    show(place ? [place] : []);
    input.value = place ? placeLabel(place) : '';
    check();
  };
  countryInput.addEventListener('input', () => set(null));
  input.addEventListener('input', () => {
    const text = input.value;
    if (ids.has(text)) return check();
    window.placesShown = invoke('search_places', {
      country: country(), prefix: text, lang: window.locale, limit: 20,
    }).then((places) => {
      if (input.value === text) show(places);
      check();
    });
  });
  return { set, id: () => ids.get(input.value) || '' };
};
const placeCountry = document.getElementById('profile-country');
const placeInput = document.getElementById('profile-place');
const profilePlace = placePicker(placeCountry, placeInput,
  document.getElementById('profile-places'), () => placeCountry.value);
const profileDraft = () => ({
  ...Object.fromEntries(scaleFields.map((scale) => [
    camel(scale.key), Number(document.getElementById(
      `profile-${scale.key === 'relationship_goal' ? 'goal' : scale.key}`).value),
  ])),
  birthYear: birthValue('year'), birthMonth: birthValue('month'), birthDay: birthValue('day'),
  country: placeCountry.value, place: profilePlace.id(),
});
// Without an answering node the way on is the Network area, so the message offers it.
const statusMessage = (status, open, key, detail) => {
  status.textContent = `${window.strings[key]}${detail ? ` ${detail}` : ''}`;
  const offline = key.endsWith('.notConnected');
  status.classList.toggle('error', offline);
  document.getElementById(open).hidden = !offline;
};
const profileMessage = (key, detail = '') => statusMessage(profileStatus, 'profile-open-network', key, detail);
// While the search window holds the published profile, say when it reaches search and send it
// once the window ends. A due profile no node took waits for the next publish or app start.
const searchWait = document.getElementById('profile-search-wait');
let searchWaitTimer;
const sendSearchProfile = () => invoke('send_search_profile').then(showSearchWait, () => {});
const showSearchWait = (view) => {
  clearTimeout(searchWaitTimer);
  const minutes = view.searchMinutes;
  searchWait.hidden = minutes == null;
  if (minutes == null) return;
  searchWait.textContent = minutes
    ? window.strings['profile.searchIn'].replace('{minutes}', String(minutes))
    : window.strings['profile.searchPending'];
  if (minutes) searchWaitTimer = setTimeout(sendSearchProfile, minutes * 60_000);
};
const showProfile = async () => {
  const [scales, view, countries] = await Promise.all(
    [invoke('profile_scales'), invoke('load_profile'), invoke('place_countries')]);
  showSearchWait(view);
  if (view.searchMinutes === 0) sendSearchProfile();
  scaleFields = scales;
  const rows = scales.map((scale) => {
    const label = document.createElement('label');
    const select = document.createElement('select');
    select.id = `profile-${scale.key === 'relationship_goal' ? 'goal' : scale.key}`;
    label.htmlFor = select.id;
    label.textContent = window.strings[`profile.field.${scale.key}`];
    const unset = document.createElement('option');
    unset.value = '0';
    unset.textContent = window.strings['profile.unset'];
    select.append(unset, ...scale.choices.map((choice) => {
      const option = document.createElement('option');
      option.value = String(choice.code);
      option.textContent = window.strings[`profile.value.${scale.key}.${choice.key}`];
      return option;
    }));
    select.value = String(view.draft[camel(scale.key)] || 0);
    const row = document.createElement('div');
    row.className = 'field';
    row.append(label, select);
    return row;
  });
  const basic = (row) => ['profile-gender', 'profile-goal'].includes(row.lastChild.id);
  document.getElementById('profile-basics').replaceChildren(...rows.filter(basic));
  profileScales.replaceChildren(...rows.filter((row) => !basic(row)));
  for (const part of ['year', 'month', 'day']) {
    document.getElementById(`birth-${part}`).value = view.draft[`birth${part[0].toUpperCase()}${part.slice(1)}`] || '';
  }
  const unsetCountry = document.createElement('option');
  unsetCountry.value = '';
  unsetCountry.textContent = window.strings['profile.unset'];
  placeCountry.replaceChildren(unsetCountry, ...countries
    .map((code) => [code, countryName(code)])
    .sort(([, a], [, b]) => a.localeCompare(b, window.locale))
    .map(([code, name]) => {
      const option = document.createElement('option');
      option.value = code;
      option.textContent = name;
      return option;
    }));
  placeCountry.value = view.draft.country;
  const place = view.draft.place && await invoke('place_name', {
    country: view.draft.country, id: Number(view.draft.place), lang: window.locale,
  });
  profilePlace.set(place);
  showAge(view);
  showHidden(view);
};
// The switch works once a version is published; hidden, the status and notice say so.
const showHidden = (view) => {
  profilePublished = Boolean(view.published);
  profileHidden.disabled = !profilePublished;
  profileHidden.checked = view.hidden;
  document.getElementById('profile-hidden-notice').hidden = !view.hidden;
  if (view.hidden) profileMessage('profile.status.hidden');
  else profileMessage(view.published ? 'profile.lastPublished' : 'profile.draft',
    view.published ? String(view.published) : '');
};
const showAge = (view) => {
  const [youngest, oldest] = view.ages || [];
  document.getElementById('profile-age').textContent = view.ages
    ? `${window.strings['profile.age']}: ${youngest === oldest ? youngest : `${youngest}–${oldest}`}`
    : window.strings['profile.ageUnknown'];
};
// Labelled public fields of a draft as [label, value] pairs, with its place named as `place`.
const publicFacts = (draft, place) => {
  const facts = scaleFields.flatMap((scale) => {
    const option = scale.choices.find((choice) => choice.code === draft[camel(scale.key)]);
    return option ? [[window.strings[`profile.field.${scale.key}`],
      window.strings[`profile.value.${scale.key}.${option.key}`]]] : [];
  });
  if (draft.country) facts.push([window.strings['profile.country'], countryName(draft.country)]);
  if (draft.place) facts.push([window.strings['profile.place'], place]);
  for (const part of ['Year', 'Month', 'Day']) {
    const value = draft[`birth${part}`];
    if (value !== null) facts.push([window.strings[`profile.birth${part}`], String(value)]);
  }
  return facts;
};
// The same fields as lines, or the empty notice.
const publicFields = (draft, place) => {
  const fields = publicFacts(draft, place).map(([label, value]) => `${label}: ${value}`);
  return fields.length ? fields : [window.strings['profile.empty']];
};
const listItems = (values) => values.map((value) => {
  const item = document.createElement('li');
  item.textContent = value;
  return item;
});
const saveProfile = async () => {
  if (!profileForm.reportValidity()) return null;
  const view = await invoke('save_profile', { draft: profileDraft() });
  showAge(view);
  profileMessage(view.problem ? `profile.problem.${view.problem}` : 'profile.saved');
  return view;
};
// The dialog lists exactly the public fields and asks both consents afresh each time.
const openPublishDialog = (view) => {
  document.getElementById('profile-public-fields')
    .replaceChildren(...listItems(publicFields(view.draft, placeInput.value)));
  document.getElementById('profile-date-warning').hidden = !(view.draft.birthYear
    && view.draft.birthMonth && view.draft.birthDay);
  profileAttested.checked = false;
  profileDisclosure.checked = false;
  publishConfirm.disabled = true;
  document.getElementById('publish-consents').hidden = false;
  document.getElementById('publish-progress').hidden = true;
  publishDialog.hidden = false;
  publishDialog.showModal();
  profileCancel.focus();
};
const elapsed = document.getElementById('publish-elapsed');
let elapsedTimer;
const startElapsed = () => {
  const start = Date.now();
  const tick = () => {
    const seconds = Math.floor((Date.now() - start) / 1000);
    elapsed.textContent = window.strings['publish.elapsed']
      .replace('{time}', `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`);
  };
  tick();
  elapsedTimer = setInterval(tick, 1000);
};
const setProfileBusy = (busy) => {
  profileBusy = busy;
  for (const element of profileForm.querySelectorAll('input, select, button')) {
    element.disabled = busy;
  }
  for (const element of [profileAttested, profileDisclosure]) element.disabled = busy;
  profileHidden.disabled = busy || !profilePublished;
  switchIdentity.disabled = busy;
  removeIdentity.disabled = busy;
  for (const button of [settingsCreate, settingsSwitch, settingsRemove]) button.disabled = busy;
};
const profileFailure = (error) => {
  const code = String(error);
  if (code === 'cancelled') return profileMessage('profile.cancelled');
  if (code.startsWith('blocked: ')) return profileMessage(`profile.problem.${code.slice(9)}`);
  if (code === 'notConnected') return profileMessage('profile.notConnected');
  profileMessage('profile.publishFailed', code);
};
profileForm.addEventListener('input', () => {
  if (!profileBusy) profileMessage('profile.draft');
});
profileForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  if (profileBusy || !profileForm.reportValidity()) return;
  setProfileBusy(true);
  try { await saveProfile(); } catch (error) { profileMessage('profile.failed', String(error)); }
  finally { setProfileBusy(false); }
});
const previewAndPublish = async () => {
  if (profileBusy || !profileForm.reportValidity()) return;
  setProfileBusy(true);
  try {
    const view = await saveProfile();
    if (view && !view.problem) openPublishDialog(view);
  } catch (error) { profileMessage('profile.failed', String(error)); }
  finally { setProfileBusy(false); }
};
document.getElementById('profile-publish').addEventListener('click', previewAndPublish);
for (const box of [profileAttested, profileDisclosure]) {
  box.addEventListener('change', () => {
    publishConfirm.disabled = !(profileAttested.checked && profileDisclosure.checked);
  });
}
// Cancel stops the puzzle while it runs and otherwise closes the dialog; the draft stays.
profileCancel.addEventListener('click', () => {
  if (!profileBusy) return publishDialog.close();
  profileCancel.disabled = true;
  invoke('cancel_profile_solve').catch(profileFailure);
});
publishDialog.addEventListener('cancel', (event) => {
  event.preventDefault();
  profileCancel.click();
});
publishDialog.addEventListener('close', () => { publishDialog.hidden = true; });
publishConfirm.addEventListener('click', async () => {
  if (profileBusy || !profileAttested.checked || !profileDisclosure.checked) return;
  setProfileBusy(true);
  publishConfirm.disabled = true;
  document.getElementById('publish-consents').hidden = true;
  document.getElementById('publish-progress').hidden = false;
  startElapsed();
  try {
    profileMessage('profile.solving');
    await invoke('solve_profile', { attested: true });
    profileMessage('profile.publishing');
    const published = await invoke('publish_profile', { attested: true });
    showSearchWait(published);
    showHidden(published);
    profileMessage('profile.published', String(published.published));
  } catch (error) {
    profileFailure(error);
  } finally {
    clearInterval(elapsedTimer);
    publishDialog.close();
    setProfileBusy(false);
    profileCancel.disabled = false;
  }
});
// On hides the profile with a tombstone; off publishes the saved draft again through the dialog.
profileHidden.addEventListener('change', async () => {
  if (!profileHidden.checked) {
    profileHidden.checked = true;
    return previewAndPublish();
  }
  setProfileBusy(true);
  try {
    showHidden(await invoke('hide_profile'));
  } catch (error) {
    profileHidden.checked = false;
    profileFailure(error);
  } finally {
    setProfileBusy(false);
  }
});

// A verified sample from the active network's search nodes. The query stays in the form, and
// filters appear only for fields every reached node offers and the client re-checks.
const searchForm = document.getElementById('search');
const searchRetry = document.getElementById('search-retry');
const searchAgain = document.getElementById('search-again');
const searchResults = document.getElementById('search-results');
const COUNTRY_FIELD = 4;
const PLACE_FIELD = 20;
const searchCountry = document.getElementById('search-country');
const searchCountryCode = () => searchCountry.value.trim().toUpperCase();
const searchPlace = placePicker(searchCountry, document.getElementById('search-place'),
  document.getElementById('search-places'), searchCountryCode);
const BIRTH_AGE_FIELD = 1001;
let searchBusy = false;
const searchMessage = (key, detail = '') =>
  statusMessage(document.getElementById('search-status'), 'search-open-network', key, detail);
// Hidden filters are disabled, so they neither block validation nor reach the query.
const showFilter = (element, shown) => {
  element.hidden = !shown;
  for (const input of element.querySelectorAll('input, select')) input.disabled = !shown || searchBusy;
};
const showSearchFilters = (offered) => {
  const scales = document.getElementById('search-scales');
  const kept = new Map([...scales.querySelectorAll('select')].map((select) => [select.id, select.value]));
  scales.replaceChildren(...scaleFields.filter((scale) => offered.includes(scale.field)).map((scale) => {
    const label = document.createElement('label');
    const select = document.createElement('select');
    select.id = `search-${scale.key}`;
    select.dataset.field = String(scale.field);
    label.htmlFor = select.id;
    label.textContent = window.strings[`profile.field.${scale.key}`];
    const any = document.createElement('option');
    any.value = '0';
    any.textContent = window.strings['search.any'];
    select.append(any, ...scale.choices.map((choice) => {
      const option = document.createElement('option');
      option.value = String(choice.code);
      option.textContent = window.strings[`profile.value.${scale.key}.${choice.key}`];
      return option;
    }));
    select.value = kept.get(select.id) || '0';
    const row = document.createElement('div');
    row.className = 'field';
    row.append(label, select);
    return row;
  }));
  showFilter(document.getElementById('search-age'), offered.includes(BIRTH_AGE_FIELD));
  showFilter(document.getElementById('search-country-row'), offered.includes(COUNTRY_FIELD));
  showFilter(document.getElementById('search-place-row'),
    offered.includes(COUNTRY_FIELD) && offered.includes(PLACE_FIELD));
  showFilter(scales, true);
  document.getElementById('search-filters').hidden = !offered.length;
};
const searchQuery = () => {
  const filled = (id) => !document.getElementById(id).disabled && document.getElementById(id).value.trim();
  const conditions = [...document.querySelectorAll('#search-scales select:enabled')]
    .filter((select) => select.value !== '0')
    .map((select) => ({ field: Number(select.dataset.field), op: 'eq', ints: [Number(select.value)] }));
  if (filled('search-country')) conditions.push({ field: COUNTRY_FIELD, op: 'eq', strings: [searchCountryCode()] });
  const place = filled('search-place') && searchPlace.id();
  if (place) conditions.push({ field: PLACE_FIELD, op: 'eq', strings: [place] });
  const [min, max] = [filled('search-age-min'), filled('search-age-max')];
  const age = min || max ? [Number(min || 18), Number(max || 150)] : null;
  const company = document.getElementById('search-company').value.trim();
  const kind = company ? document.getElementById('search-employer').value : 'any';
  const employer = kind === 'same' ? { kind, company }
    : kind === 'exclude' ? { kind, companies: company.split(';').map((name) => name.trim()).filter(Boolean) }
    : { kind: 'any' };
  return { age, conditions, employer, limit: 0 };
};
const failureText = (failure) => {
  const text = window.strings[`search.failure.${failure.kind}`];
  return failure.kind === 'rateLimited' ? `${text} ${Math.ceil(failure.retryAfterMs / 1000)}` : text;
};
// Age in years from public birth fields today, a range while the birthday may be ahead.
const ageText = (draft) => {
  if (draft.birthYear === null) return '';
  const now = new Date();
  const years = now.getFullYear() - draft.birthYear;
  const [month, day] = [now.getMonth() + 1, now.getDate()];
  const passed = draft.birthMonth === null ? null
    : draft.birthMonth !== month ? draft.birthMonth < month
      : draft.birthDay === null ? null : draft.birthDay <= day;
  return passed === null ? `${years - 1}–${years}` : String(passed ? years : years - 1);
};
const genderText = (draft) => {
  const scale = scaleFields.find((field) => field.key === 'gender');
  const option = scale?.choices.find((choice) => choice.code === draft.gender);
  return option ? window.strings[`profile.value.gender.${option.key}`] : '';
};
const summaryText = (found) => [genderText(found.draft), ageText(found.draft), found.place]
  .filter(Boolean).join(' · ');
// Cards show the letters of the short ID until profiles carry photos; nothing marks a
// self-declared claim as verified.
const resultCard = (found, index) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'list-button';
  button.id = `result-${index + 1}`;
  button.dataset.peerId = found.peerId;
  button.append(...who(button, found.peerId, '', summaryText(found)));
  button.addEventListener('click', () => openFound(found, button));
  return button;
};
const searchMain = document.getElementById('search-main');
const profileView = document.getElementById('profile-view');
let viewed = null;
const openFound = (found, card) => {
  viewed = { found, card };
  const short = peerLabel(document.getElementById('view-peer'), found.peerId);
  document.getElementById('view-peer').textContent = short;
  document.getElementById('view-title').textContent = short;
  document.querySelector('#view-photos .main').textContent = short.slice(1, 3);
  document.getElementById('view-summary').textContent = summaryText(found);
  document.getElementById('view-fields').replaceChildren(...publicFacts(found.draft, found.place)
    .flatMap(([label, value]) => {
      const term = document.createElement('dt');
      const detail = document.createElement('dd');
      term.textContent = label;
      detail.textContent = value;
      return [term, detail];
    }));
  searchMain.hidden = true;
  profileView.hidden = false;
  document.getElementById('view-back').focus();
};
const closeFound = () => {
  profileView.hidden = true;
  searchMain.hidden = false;
  if (viewed?.card.isConnected) viewed.card.focus();
  viewed = null;
};
document.getElementById('view-back').addEventListener('click', closeFound);
copyFrom(document.getElementById('copy-view-peer'), document.getElementById('view-peer'),
  () => viewed.found.peerId);
// Blocking is local: the card goes, and later searches drop the peer.
document.getElementById('view-block').addEventListener('click', async () => {
  const { found, card } = viewed;
  try {
    await invoke('block_peer', { peerId: found.peerId });
  } catch (error) {
    return searchMessage('search.failed', String(error));
  }
  card.remove();
  closeFound();
  return searchMessage('search.blocked');
});
const searchError = document.getElementById('search-error');
const showSearchError = (key, details) => {
  document.getElementById('search-error-text').textContent = window.strings[key];
  document.getElementById('search-failures').textContent = details;
  searchError.hidden = false;
};
copyFrom(document.getElementById('copy-search-details'), document.getElementById('search-failures'));
// Loading blocks repeated requests, not navigation.
const setSearchBusy = (busy) => {
  searchBusy = busy;
  document.getElementById('search-loading').hidden = !busy;
  for (const button of searchForm.querySelectorAll('button')) button.disabled = busy;
  for (const input of searchForm.querySelectorAll('input, select')) {
    const hidden = input.closest('[hidden]');
    input.disabled = busy || Boolean(hidden && searchForm.contains(hidden));
  }
};
const clearSearch = () => {
  searchError.hidden = true;
  document.getElementById('search-empty').hidden = true;
  searchResults.replaceChildren();
  searchAgain.hidden = true;
};
const runSearch = async () => {
  if (searchBusy || !searchForm.reportValidity()) return;
  const query = searchQuery();
  closeFound();
  clearSearch();
  setSearchBusy(true);
  searchMessage('search.loading');
  try {
    const result = await invoke('search_profiles', { query });
    knownPeers = [...new Set([...knownPeers, ...result.profiles.map((found) => found.peerId)])];
    showSearchFilters(result.offered);
    const found = await Promise.all(result.profiles.map(async (profile) => ({
      ...profile, place: await placeText(profile.draft.country, profile.draft.place),
    })));
    searchResults.replaceChildren(...found.map(resultCard));
    const count = String(found.length);
    if (result.partial || result.failures.length) {
      searchMessage('search.partial', count);
      showSearchError('search.partialError', result.failures.map(failureText).join('\n'));
    } else if (!found.length) searchMessage('search.empty');
    else searchMessage('search.results', count);
    document.getElementById('search-empty').hidden = found.length > 0;
    searchAgain.hidden = !found.length;
  } catch (error) {
    const code = String(error);
    if (code === 'notConnected') searchMessage('search.notConnected');
    else searchMessage('search.failed', '');
    showSearchError(code === 'notConnected' ? 'search.offline' : 'search.error', code);
  } finally {
    setSearchBusy(false);
  }
};
const resetSearch = () => {
  searchForm.reset();
  searchPlace.set(null);
  showSearchFilters([]);
  closeFound();
  clearSearch();
  searchMessage('search.idle');
};
searchForm.addEventListener('submit', (event) => {
  event.preventDefault();
  window.searchDone = runSearch();
});
for (const id of ['profile-open-network', 'search-open-network']) {
  document.getElementById(id).addEventListener('click', () => showArea('network-area'));
}
for (const button of [searchRetry, searchAgain]) {
  button.addEventListener('click', () => { window.searchDone = runSearch(); });
}

async function render(state) {
  const fresh = created;
  created = false;
  if (state.status === 'ready' || state.status === 'recovery') current = state;
  if (state.status === 'ready') {
    document.getElementById('peer-id').textContent = state.peerId;
    document.getElementById('file-backup').hidden = state.mode !== 'file';
    document.getElementById('key-path').textContent = state.keyPath || '';
    knowPeers(await invoke('list_identities'));
    const sidebarIdentity = document.getElementById('sidebar-identity');
    const [avatar, text] = who(sidebarIdentity, state.peerId, '', modeText(state));
    text.className = 'label';
    text.lastChild.className = 'sub';
    sidebarIdentity.replaceChildren(avatar, text);
    await window.showNetwork();
    window.connectionShown = connectAtLaunch();
    await showProfile();
    resetSearch();
    // Starts the node when its setting is on and stops another identity's node; never blocks.
    invoke('apply_node').catch(() => {});
    showArea('profile-area');
    window.statusShown = showStatus();
    wizardStep(fresh);
    show(fresh ? wizardNetwork : identity, 'status.ready');
    return state;
  }
  if (state.status === 'picker') {
    const items = await invoke('list_identities');
    document.getElementById('identities').replaceChildren(...knowPeers(items).map(identityButton));
    show(sections[2], 'status.pick');
    return state;
  }
  if (state.status === 'recovery') {
    document.getElementById('recovery-reason').textContent = reasonText(state.reason);
    // The state code and the slot row, for a bug report.
    document.getElementById('recovery-details').textContent = [state.reason,
      [state.slot, state.mode].filter(Boolean).join(' ')].join('\n');
    document.getElementById('recovery-more').open = false;
    document.getElementById('recovery-peer').textContent = state.peerId || '';
    document.getElementById('recovery-peer-row').hidden = !state.peerId;
    show(sections[3], 'status.recovery');
    return state;
  }
  if (state.status !== 'setup') throw new Error('Unknown identity state');
  // Nothing is created until Create; the recommended available store is preselected.
  systemChoice.disabled = state.systemAvailable !== 'true';
  fileChoice.disabled = false;
  createIdentity.disabled = false;
  (systemChoice.disabled ? fileChoice : systemChoice).checked = true;
  document.getElementById('system-reason').textContent = systemChoice.disabled
    ? window.strings[state.systemReason] : '';
  show(setup, 'status.choose');
  return state;
}

// A new identity opens on the network step of the wizard.
let created = false;
createIdentity.addEventListener('click', () => {
  const mode = systemChoice.checked ? 'system' : 'file';
  for (const each of [systemChoice, fileChoice, createIdentity]) each.disabled = true;
  created = true;
  act(invoke('create_identity', { mode }));
});
copyFrom(document.getElementById('copy-recovery-details'),
  document.getElementById('recovery-details'));
switchIdentity.addEventListener('click', () => act({ status: 'picker' }));
document.getElementById('add-identity')
  .addEventListener('click', () => act(invoke('storage_options')));
document.getElementById('retry')
  .addEventListener('click', () => act(invoke('identity_state')));

// Second confirmation: Remove stays disabled until the checkbox is ticked.
const removeAsk = document.getElementById('remove-ask');
const removeOffline = document.getElementById('remove-offline');
const removeRetry = document.getElementById('remove-retry');
const removeAnyway = document.getElementById('remove-anyway');
let removePublished = false;
const showRemove = async () => {
  const items = await invoke('list_identities');
  const copies = current.peerId && items.some((item) => item.slot !== current.slot
    && item.status === 'ready' && item.peerId === current.peerId);
  const whoRow = document.getElementById('remove-who');
  whoRow.replaceChildren(...who(whoRow, current.peerId, current.reason, modeText(current)));
  const peer = document.getElementById('remove-peer');
  peer.textContent = current.peerId || reasonText(current.reason);
  peer.parentElement.hidden = !current.peerId;
  const path = document.getElementById('remove-path');
  path.hidden = !current.keyPath;
  path.querySelector('code').textContent = current.keyPath || '';
  document.getElementById('remove-copies').textContent = !current.peerId ? ''
    : window.strings[copies ? 'remove.otherCopy' : 'remove.lastCopy'];
  // A missing key or another copy means nothing irreplaceable is lost.
  document.getElementById('remove-warning').hidden = !current.peerId || copies;
  // A published profile is withdrawn from search before the key goes.
  const view = current.status === 'ready' && await invoke('load_profile').catch(() => null);
  removePublished = Boolean(view && view.published);
  document.getElementById('remove-text').hidden = !removePublished;
  removeAsk.hidden = false;
  removeOffline.hidden = true;
  removeConfirm.checked = false;
  removeAction.disabled = true;
  removeDialog.hidden = false;
  removeDialog.showModal();
  document.getElementById('remove-cancel').focus();
};
removeDialog.addEventListener('close', () => { removeDialog.hidden = true; });
for (const button of [removeIdentity, settingsRemove]) {
  button.addEventListener('click', () => showRemove().catch(showError));
}
for (const id of ['setup-cancel', 'picker-cancel']) {
  document.getElementById(id).addEventListener('click', () => (current.status === 'ready'
    ? show(identity, 'status.ready') : show(sections[3], 'status.recovery')));
}
removeConfirm.addEventListener('change', () => { removeAction.disabled = !removeConfirm.checked; });
const removeNow = () => {
  const removed = current;
  // Nothing is open after a removal until the backend reports the next state.
  current = null;
  return act(invoke('remove_identity', { slot: removed.slot, peerId: removed.peerId || null }));
};
// The tombstone goes first; when no node takes it, the person chooses to wait or remove anyway.
const withdrawThenRemove = async () => {
  if (removePublished) {
    try {
      await invoke('hide_profile');
    } catch (_) {
      removeAsk.hidden = true;
      removeOffline.hidden = false;
      removeRetry.disabled = false;
      removeAnyway.disabled = false;
      document.getElementById('remove-cancel-offline').focus();
      return null;
    }
  }
  return removeNow();
};
removeAction.addEventListener('click', () => {
  removeAction.disabled = true;
  window.removeDone = withdrawThenRemove();
});
removeRetry.addEventListener('click', () => {
  removeRetry.disabled = true;
  removeAnyway.disabled = true;
  window.removeDone = withdrawThenRemove();
});
removeAnyway.addEventListener('click', () => {
  removeAnyway.disabled = true;
  window.removeDone = removeNow();
});
for (const id of ['remove-cancel', 'remove-cancel-offline']) {
  document.getElementById(id).addEventListener('click', () => removeDialog.close());
}
window.identityShown = window.stringsShown
  .then(() => invoke('identity_state'))
  .then(render)
  .catch(() => { showError(); return { status: 'error' }; });
