import { all as domAll, byId as domById, query as domQuery } from './dom.js';
import { controller as chatsUiController } from './chats-ui.js';
import { copyFrom } from './clipboard.js';
import { controller as devicesController } from './devices.js';
import { removeDialog, removeIdentity, settingsRemove } from './elements.js';
import { sizeText } from './formatting.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import * as localeModule from './locale.js';
import { strings as localeModule_strings, stringsShown, locale } from './locale.js';
import { connectAtLaunch, wizardStep, controller as networkController } from './network.js';
import { knowPeers, shortPeer } from './peer-labels.js';
import { controller as peerNamesController } from './peer-names.js';
import { placePicker } from './places.js';
import * as profileFieldsModule from './profile-fields.js';
import { profileForm as profileFormModule_profileForm } from './profile-form.js';
import { profileInterestTags, showProfileInterests } from './profile-interests.js';
import { controller as profilePhotosController } from './profile-photos.js';
import { seekingDraft, showSeeking, seekingFacts, profileFormValid } from './profile-seeking.js';
import { controller as searchController } from './search.js';
import { controller as settingsController } from './settings.js';
import { progress } from './ui-progress.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.settingsShown = undefined;
/** @type {Promise<void> | undefined} */
controller.nodeApplied = undefined;
/** @type {Promise<void> | undefined} */
controller.profileSaveDone = undefined;
/** @type {Promise<void> | undefined} */
controller.publishDone = undefined;
/** @type {Promise<void> | undefined} */
controller.hideDone = undefined;
/** @type {Promise<void> | undefined} */
controller.statusShown = undefined;

const ready = domById('app-ready', HTMLParagraphElement);
const setup = domById('setup', HTMLElement);
const identity = domById('identity', HTMLDivElement);
const systemChoice = domById('system-choice', HTMLInputElement);
const fileChoice = domById('file-choice', HTMLInputElement);
const createIdentity = domById('create-identity', HTMLButtonElement);
const linkIdentity = domById('link-identity', HTMLButtonElement);
const storageError = domById('storage-error', HTMLParagraphElement);
const bootActions = domById('boot-actions', HTMLDivElement);
const bootRetry = domById('boot-retry', HTMLButtonElement);

const switchIdentity = domById('switch-identity', HTMLButtonElement);

const wizardNetwork = domById('wizard-network', HTMLElement);
const sections = [
  setup,
  identity,
  domById('picker', HTMLElement),
  domById('recovery', HTMLElement),
  wizardNetwork,
  domById('link', HTMLElement),
];
// The opened identity or recovery state the remove dialog acts on.

// Shows one section (or none, the boot page) with its status line.
/** @param {HTMLElement | null} section @param {string} status */
const show = (section, status) => {
  if (removeDialog.open) removeDialog.close();
  devicesController.closeDeviceDialogs?.();
  domById('boot', HTMLElement).hidden = Boolean(section);
  for (const each of sections) each.hidden = each !== section;
  // Ready keeps identity actions in Settings; recovery has no other way out.
  switchIdentity.hidden = section !== sections[3];
  // The backend refuses to remove a damaged or unreadable slot.
  removeIdentity.hidden = !(
    section === sections[3] &&
    ['missing', 'incomplete', 'deleting'].includes(identityState.current?.reason)
  );
  // Setup and the picker opened from an open identity can go back to it.
  for (const id of ['setup-cancel', 'picker-cancel'])
    domById(id, HTMLElement).hidden = !identityState.current;
  // An open identity links from Settings, with its network.
  linkIdentity.hidden = Boolean(identityState.current);
  storageError.hidden = true;
  bootActions.hidden = true;
  ready.textContent = localeModule_strings[status];
};

const showError = () => {
  show(null, 'status.error');
  switchIdentity.hidden = true;
  domById('boot-loading', HTMLParagraphElement).hidden = true;
  storageError.textContent = localeModule_strings['storage.error'];
  storageError.hidden = false;
  bootActions.hidden = false;
  bootRetry.focus();
};

/** @param {string | null | undefined} [reason] */
const reasonText = (reason) =>
  localeModule_strings[`recovery.${reason}`] || localeModule_strings['recovery.unavailable'];

// Copies `text()`, by default the text of `source`; where the webview refuses, shows it in
// `source` and selects it for Ctrl+C instead.

// A result belongs to the identity view that started it, even after its Rust lease ends.

/**
 * @param {import('./identity-state.js').IdentityResponse | PromiseLike<import('./identity-state.js').IdentityResponse>} pending
 * @param {boolean | 'link'} [fresh]
 */
const act = async (pending, fresh = false) => {
  const generation = ++identityState.generation;
  document.dispatchEvent(new Event('dyapp:identity-change'));
  clearTimeout(searchWaitTimer);
  clearInterval(elapsedTimer);
  clearTimeout(usageTimer);
  clearTimeout(statusTimer);
  try {
    const state = await pending;
    if (identityCurrent(generation)) return await render(state, generation, fresh);
    return state;
  } catch (_) {
    if (identityCurrent(generation)) showError();
    return { status: 'error' };
  }
};

/** @param {HTMLElement} element @param {string} id */
const peerLabel = (element, id) => {
  Object.assign(element.dataset, { peerId: id });
  element.title = id;
  return peerNamesController.peerName(id);
};

// An identity as an avatar, its short ID (or why it cannot open) and a caption under it.
/**
 * @param {HTMLElement} element
 * @param {string | undefined} id
 * @param {string | null | undefined} reason
 * @param {string} caption
 */
const who = (element, id, reason, caption) => {
  const avatar = document.createElement('span');
  avatar.className = 'avatar';
  avatar.setAttribute('aria-hidden', 'true');
  const name = document.createElement(id ? 'code' : 'span');
  name.textContent = id ? peerLabel(element, id) : reasonText(reason);
  avatar.textContent = id ? shortPeer(id).slice(1, 3) : '!';
  const note = document.createElement('span');
  note.className = 'caption';
  note.textContent = caption;
  const text = document.createElement('span');
  text.append(name, document.createElement('br'), note);
  if (id && id === identityState.current?.peerId) profilePhotosController.ownAvatar(avatar);
  return [avatar, text];
};
/** @param {{ mode?: import('./identity-state.js').StorageMode }} item */
const modeText = (item) => localeModule_strings[`identity.mode.${item.mode}`] || '';

/**
 * @param {import('./identity-state.js').ReadyIdentity | import('./identity-state.js').RecoveryIdentity} item
 */
const identityButton = (item) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'list-button';
  Object.assign(button.dataset, { status: item.status, mode: item.mode || '' });
  button.append(
    ...who(button, item.status === 'ready' && item.peerId, item.reason, modeText(item)),
  );
  button.addEventListener('click', () => act(invoke('select_identity', { slot: item.slot })));
  return button;
};

// Remove applies to the open identity only, so it sits in that row.

/**
 * @param {import('./identity-state.js').ReadyIdentity | import('./identity-state.js').RecoveryIdentity} item
 */
controller.settingsIdentity = (item) => {
  const row = document.createElement('tr');
  const name = document.createElement('td');
  name.className = 'who';
  name.append(...who(row, item.status === 'ready' && item.peerId, item.reason, modeText(item)));
  const active = document.createElement('td');
  active.className = 'caption';
  active.textContent =
    item.slot === identityState.current.slot ? localeModule_strings['settings.active'] : '';
  if (item.slot === identityState.current.slot) {
    settingsRemove.hidden = false;
    active.append(document.createElement('br'), settingsRemove);
  }
  row.append(name, active);
  row.dataset.active = String(item.slot === identityState.current.slot);
  return row;
};

// Settings → Storage: binary units with one decimal; an unknown number reads Unavailable, not 0.

/** @param {number | null} bytes */
const knownSize = (bytes) =>
  bytes === null ? localeModule_strings['usage.unavailable'] : sizeText(bytes);
/** @param {string} id @param {string} text */
const usageText = (id, text) => {
  domById(id, HTMLElement).textContent = text;
};
/** @type {number | undefined} */
let usageTimer;
controller.showUsage = async (call = invoke) => {
  clearTimeout(usageTimer);
  const generation = identityState.generation;
  const card = domById('usage', HTMLDivElement);
  if (domById('settings', HTMLElement).hidden) return;
  try {
    const usage = await call('storage_usage');
    if (!identityCurrent(generation)) return;
    usageText('usage-identity-dir', usage.identityDir ?? localeModule_strings['usage.unavailable']);
    usageText('usage-messages', knownSize(usage.messages));
    usageText('usage-messages-path', usage.messagesPath);
    domById('usage-node-row', HTMLTableRowElement).hidden = usage.nodePath === null;
    const limited = usage.node !== null && usage.nodeLimit !== null;
    usageText(
      'usage-node',
      limited
        ? localeModule_strings['usage.of']
            .replace('{used}', sizeText(usage.node))
            .replace('{limit}', sizeText(usage.nodeLimit))
        : knownSize(usage.node),
    );
    usageText('usage-node-path', usage.nodePath ?? '');
    const bar = domById('usage-node-bar', HTMLMeterElement);
    bar.hidden = !limited;
    if (limited) Object.assign(bar, { max: usage.nodeLimit, value: usage.node });
    domById('usage-free-row', HTMLTableRowElement).hidden = usage.free === null;
    if (usage.free !== null) usageText('usage-free', sizeText(usage.free));
    card.hidden = false;
  } catch (_) {
    if (!identityCurrent(generation)) return;
    card.hidden = true;
  }
  if (identityCurrent(generation)) usageTimer = setTimeout(controller.showUsage, 10_000);
};

// The status bar: client traffic every 2 s, unavailable until a client swarm starts.
/** @type {number | undefined} */
let statusTimer;
const showStatus = async (call = invoke) => {
  clearTimeout(statusTimer);
  const generation = identityState.generation;
  const { strings } = localeModule;
  const traffic = domById('status-traffic', HTMLSpanElement);
  const messages = domById('status-messages', HTMLSpanElement);
  try {
    const status = await call('status');
    if (!identityCurrent(generation)) return;
    traffic.textContent =
      status.traffic === null
        ? strings['statusbar.noTraffic']
        : strings['statusbar.traffic']
            .replace('{in}', sizeText(status.traffic[0]))
            .replace('{out}', sizeText(status.traffic[1]));
    messages.textContent =
      status.messages === null
        ? strings['statusbar.noMessages']
        : strings['statusbar.messages'].replace('{size}', sizeText(status.messages));
  } catch (_) {
    if (!identityCurrent(generation)) return;
    traffic.textContent = strings['statusbar.noTraffic'];
    messages.textContent = strings['statusbar.noMessages'];
  }
  traffic.title = strings['statusbar.trafficNote'];
  if (identityCurrent(generation)) statusTimer = setTimeout(showStatus, 2000);
};

// Settings → Window: the tray icon and close-to-tray, usable only while a tray host is running.
const trayShow = domById('tray-show', HTMLInputElement);
const trayClose = domById('tray-close', HTMLInputElement);
/** @param {{ show: boolean; closeToTray: boolean; } | null} prefs */
const setTray = async (prefs, call = invoke) => {
  const generation = identityState.generation;
  const labels = ['tray.open', 'tray.quit', 'notice.newMessage', 'stickers.notice'].map(
    (key) => localeModule_strings[key],
  );
  const tray = await call('set_tray', { prefs, labels });
  if (!identityCurrent(generation)) return tray;
  trayShow.checked = tray.show;
  trayShow.disabled = !tray.available;
  trayClose.checked = tray.closeToTray;
  trayClose.disabled = !tray.available || !tray.show;
  domById('tray-note', HTMLParagraphElement).hidden = tray.available;
  return tray;
};
const changeTray = () => setTray({ show: trayShow.checked, closeToTray: trayClose.checked });
trayShow.addEventListener('change', changeTray);
trayClose.addEventListener('change', changeTray);
progress.trayShown = stringsShown.then(() => setTray(null));

// Settings → Appearance: kept by this app for every identity; a language change reloads the page.
/** @param {string | null} theme */
const applyTheme = (theme) => {
  if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
};
applyTheme(localStorage.getItem('theme'));
for (const radio of domAll(document, 'input[name="theme"]', HTMLInputElement)) {
  radio.checked = radio.value === (localStorage.getItem('theme') || 'system');
  radio.addEventListener('change', () => {
    localStorage.setItem('theme', radio.value);
    applyTheme(radio.value);
  });
}
const languageChoice = domById('language', HTMLSelectElement);
languageChoice.value = localStorage.getItem('language') || 'system';
languageChoice.addEventListener('change', () => {
  localStorage.setItem('language', languageChoice.value);
  window.location.reload();
});

// The shell: one area at a time; below 640 px the sidebar slides in over a scrim.
const menuOpen = domById('menu-open', HTMLButtonElement);
/** @param {boolean} open */
const setMenu = (open) => {
  if (open) document.documentElement.dataset.screen = 'menu';
  else delete document.documentElement.dataset.screen;
  menuOpen.setAttribute('aria-expanded', String(open));
};
menuOpen.addEventListener('click', () =>
  setMenu(document.documentElement.dataset.screen !== 'menu'),
);
domById('scrim', HTMLDivElement).addEventListener('click', () => setMenu(false));
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') setMenu(false);
  if (event.ctrlKey && event.key.toLowerCase() === 'q') invoke('quit');
});
/** @param {string} area */
const showArea = (area) => {
  document.dispatchEvent(new CustomEvent('dyapp:area-change', { detail: area }));
  for (const button of domAll(document, '#sidebar [data-area]', HTMLButtonElement)) {
    domById(button.dataset.area, HTMLElement).hidden = button.dataset.area !== area;
    if (button.dataset.area === area) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  }
  setMenu(false);
  // The page scrolls as a whole, so a new area would open at the old area's offset.
  window.scrollTo(0, 0);
  if (area === 'settings') controller.settingsShown = settingsController.showSettings();
  if (area === 'chats-area') progress.chatsDone = chatsUiController.showChats();
};
for (const button of domAll(document, '#sidebar [data-area]', HTMLButtonElement)) {
  button.addEventListener('click', () => showArea(button.dataset.area));
}
const settingsCreate = domById('settings-create', HTMLButtonElement);
const settingsSwitch = domById('settings-switch', HTMLButtonElement);
settingsCreate.addEventListener('click', () => act(invoke('storage_options')));
settingsSwitch.addEventListener('click', () => act({ status: 'picker' }));

// The store and search node the app runs while open (ADR 0030); Windows builds have no node
// commands, so the form stays hidden there.
const nodeForm = domById('node', HTMLFormElement);
const nodeEnabled = domById('node-enabled', HTMLInputElement);
const nodeDisk = domById('node-disk', HTMLInputElement);
const nodeConnections = domById('node-connections', HTMLInputElement);
const nodeSave = domById('node-save', HTMLButtonElement);
/** @param {string} status @param {string | undefined} [detail] */
const nodeStatus = (status, detail) => {
  const text = localeModule_strings[`node.${status}`];
  domById('node-status', HTMLParagraphElement).textContent = detail ? `${text} ${detail}` : text;
  nodeForm.dataset.status = status;
};
/** @param {import('./ipc.js').NodeStatus} result */
const nodeResult = (result) => nodeStatus(result.status, result.peerId);
controller.showNode = async (call = invoke) => {
  const generation = identityState.generation;
  try {
    const result = await call('node_status');
    if (!identityCurrent(generation)) return;
    nodeResult(result);
  } catch (_) {
    if (identityCurrent(generation)) nodeForm.hidden = true;
    return;
  }
  nodeForm.hidden = false;
  try {
    const saved = await call('load_node_settings');
    if (!identityCurrent(generation)) return;
    nodeEnabled.checked = saved.enabled;
    nodeDisk.value = saved.diskMb;
    nodeConnections.value = saved.maxConnections;
  } catch (error) {
    if (identityCurrent(generation)) nodeStatus('error', String(error));
  }
};
/** @param {{ enabled: boolean; diskMb: number; maxConnections: number; }} node */
const saveNodeSettings = (node, call = invoke) => {
  const generation = identityState.generation;
  nodeSave.disabled = true;
  nodeStatus(node.enabled ? 'starting' : 'off');
  return call('save_node_settings', { settings: node })
    .then((result) => {
      if (identityCurrent(generation)) nodeResult(result);
    })
    .catch((error) => {
      if (identityCurrent(generation)) nodeStatus('error', String(error));
    })
    .finally(() => {
      if (identityCurrent(generation)) nodeSave.disabled = false;
    });
};
nodeForm.addEventListener('submit', (event) => {
  event.preventDefault();
  const node = {
    enabled: nodeEnabled.checked,
    diskMb: Number(nodeDisk.value),
    maxConnections: Number(nodeConnections.value),
  };
  if (
    !Number.isInteger(node.diskMb) ||
    node.diskMb < 4 ||
    !Number.isInteger(node.maxConnections) ||
    node.maxConnections < 1
  ) {
    nodeStatus('invalidLimits');
    return;
  }
  controller.nodeApplied = saveNodeSettings(node);
});

// The Rust core supplies field numbers and choices; catalogues supply only visible names.
const profileForm = domById('profile', HTMLFormElement);
const profileScales = domById('profile-scales', HTMLDivElement);
const profileStatus = domById('profile-status', HTMLParagraphElement);
const publishDialog = domById('publish-dialog', HTMLDialogElement);
const publishConfirm = domById('profile-publish-confirm', HTMLButtonElement);
const profileHidden = domById('profile-hidden', HTMLInputElement);
const profileAttested = domById('profile-attested', HTMLInputElement);
const profileDisclosure = domById('profile-disclosure', HTMLInputElement);
const profileCancel = domById('profile-cancel', HTMLButtonElement);
let profileBusy = false;
let profilePublished = false;
/** @param {string} part */
const birthValue = (part) => {
  const value = domById(`birth-${part}`, HTMLInputElement).value;
  return value === '' ? null : Number(value);
};

// The place input holds a suggestion's text; ids maps each shown suggestion to its GeoNames ID,
// so typed text that is no suggestion fails validation instead of being saved or searched.
// Editing the country clears the place.

const placeCountry = domById('profile-country', HTMLSelectElement);
const placeInput = domById('profile-place', HTMLInputElement);
const profilePlace = placePicker(
  placeCountry,
  placeInput,
  domById('profile-places', HTMLDataListElement),
  () => placeCountry.value,
);
const profileDraft = () => ({
  name: domById('profile-name', HTMLInputElement).value,
  ...Object.fromEntries(
    profileFieldsModule.scaleFields.map((scale) => [
      profileFieldsModule.camel(scale.key),
      Number(
        domById(
          `profile-${scale.key === 'relationship_goal' ? 'goal' : scale.key}`,
          HTMLSelectElement,
        ).value,
      ),
    ]),
  ),
  birthYear: birthValue('year'),
  birthMonth: birthValue('month'),
  birthDay: birthValue('day'),
  height: Number(domById('profile-height', HTMLInputElement).value) || null,
  about: domById('profile-about', HTMLTextAreaElement).value,
  interests: profileInterestTags(),
  country: placeCountry.value,
  place: profilePlace.id(),
  ...seekingDraft(),
});
// Without an answering node the way on is the Network area, so the message offers it.
/**
 * @param {HTMLParagraphElement} status @param {string} open @param {string} detail
 * @param {string} key
 */
const statusMessage = (status, open, key, detail) => {
  status.textContent = `${localeModule_strings[key]}${detail ? ` ${detail}` : ''}`;
  const offline = key.endsWith('.notConnected');
  status.classList.toggle('error', offline);
  domById(open, HTMLElement).hidden = !offline;
};
/** @param {string} key */
const profileMessage = (key, detail = '') =>
  statusMessage(profileStatus, 'profile-open-network', key, detail);
// While the search window holds the published profile, say when it reaches search and send it
// once the window ends. A due profile no node took waits for the next publish or app start.
const searchWait = domById('profile-search-wait', HTMLParagraphElement);
/** @type {number | undefined} */
let searchWaitTimer;
const sendSearchProfile = (generation = identityState.generation, call = invoke) => {
  if (!identityCurrent(generation)) return Promise.resolve();
  return call('send_search_profile').then(
    (view) => {
      if (!identityCurrent(generation)) return;
      showSearchWait(view);
      showHidden(view);
    },
    () => {},
  );
};
/** @param {profileFieldsModule.ProfileView} view */
const showSearchWait = (view, send = sendSearchProfile) => {
  clearTimeout(searchWaitTimer);
  const generation = identityState.generation;
  const minutes = view.searchMinutes;
  searchWait.hidden = minutes == null;
  if (minutes == null) return;
  searchWait.textContent = minutes
    ? localeModule_strings['profile.searchIn'].replace('{minutes}', String(minutes))
    : localeModule_strings['profile.searchPending'];
  searchWaitTimer = setTimeout(
    () => {
      if (identityCurrent(generation)) send(generation);
    },
    (minutes || 1) * 60_000,
  );
};
const showProfile = async (call = invoke) => {
  const generation = identityState.generation;
  const [scales, view, countries] = await Promise.all([
    call('profile_scales'),
    call('load_profile'),
    call('place_countries'),
  ]);
  if (!identityCurrent(generation)) return;
  showSearchWait(view);
  if (view.searchMinutes === 0) sendSearchProfile(generation);
  profileFieldsModule.setScaleFields(scales);
  const rows = scales.map((scale) => profileFormModule_profileForm.scaleRow(scale, view.draft));
  /** @param {HTMLDivElement} row */
  const basic = (row) => ['profile-gender', 'profile-goal'].includes(row.lastChild.id);
  domById('profile-basics', HTMLDivElement).replaceChildren(...rows.filter(basic));
  profileScales.replaceChildren(...rows.filter((row) => !basic(row)));
  for (const part of ['year', 'month', 'day']) {
    domById(`birth-${part}`, HTMLInputElement).value =
      view.draft[`birth${part[0].toUpperCase()}${part.slice(1)}`] || '';
  }
  domById('profile-height', HTMLInputElement).value = view.draft.height || '';
  domById('profile-about', HTMLTextAreaElement).value = view.draft.about;
  showProfileInterests(view.draft.interests);
  showSeeking(view.draft);
  placeCountry.replaceChildren(...profileFormModule_profileForm.countryOptions(countries));
  domById('profile-name', HTMLInputElement).value = view.draft.name;
  placeCountry.value = view.draft.country;
  const place =
    view.draft.place &&
    (await call('place_name', {
      country: view.draft.country,
      id: Number(view.draft.place),
      lang: locale,
    }));
  if (!identityCurrent(generation)) return;
  profilePlace.set(place);
  showAge(view);
  showHidden(view);
  profilePhotosController.showProfilePhotos(view.draft.photos, call);
};
// The switch works once a version is published; hidden, the status and notice say so.
/** @param {profileFieldsModule.ProfileView} view */
const showHidden = (view) => {
  profilePublished = Boolean(view.published);
  profileHidden.disabled = !profilePublished;
  profileHidden.checked = view.hidden;
  domById('profile-hidden-notice', HTMLParagraphElement).hidden = !view.hidden;
  if (view.hidden)
    profileMessage(
      view.searchMinutes == null ? 'profile.status.hidden' : 'profile.status.hidePending',
    );
  else
    profileMessage(
      view.published ? 'profile.lastPublished' : 'profile.draft',
      view.published ? String(view.published) : '',
    );
};
/** @param {profileFieldsModule.ProfileView} view */
const showAge = (view) => {
  const [youngest, oldest] = view.ages || [];
  domById('profile-age', HTMLParagraphElement).textContent = view.ages
    ? `${localeModule_strings['profile.age']}: ${youngest === oldest ? youngest : `${youngest}–${oldest}`}`
    : localeModule_strings['profile.ageUnknown'];
};
// Labelled public fields of a draft as [label, value] pairs, with its place named as `place`.
/** @param {import('./profile-fields.js').ProfileDraft} draft @param {string} place */
const publicFacts = (draft, place) => {
  const facts = profileFieldsModule.scaleFields.flatMap((scale) => {
    const option = scale.choices.find(
      (choice) => choice.code === draft[profileFieldsModule.camel(scale.key)],
    );
    return option
      ? [
          [
            localeModule_strings[`profile.field.${scale.key}`],
            localeModule_strings[`profile.value.${scale.key}.${option.key}`],
          ],
        ]
      : [];
  });
  if (draft.country)
    facts.push([
      localeModule_strings['profile.country'],
      profileFieldsModule.countryName(draft.country),
    ]);
  if (draft.place) facts.push([localeModule_strings['profile.place'], place]);
  for (const part of ['Year', 'Month', 'Day']) {
    const value = draft[`birth${part}`];
    if (value !== null) facts.push([localeModule_strings[`profile.birth${part}`], String(value)]);
  }
  if (draft.height) facts.push([localeModule_strings['profile.height'], String(draft.height)]);
  if (draft.about) facts.push([localeModule_strings['profile.about'], draft.about]);
  if (draft.interests?.length)
    facts.push([localeModule_strings['profile.interests'], draft.interests.join('; ')]);
  facts.push(...seekingFacts(draft));
  return facts;
};
// The same fields as lines, or the empty notice.
/** @param {string} place @param {profileFieldsModule.ProfileDraft} draft */
const publicFields = (draft, place) => {
  const fields = publicFacts(draft, place).map(([label, value]) => `${label}: ${value}`);
  if (draft.name) fields.unshift(`${localeModule_strings['profile.name']}: ${draft.name}`);
  if (draft.photos?.length)
    fields.push(`${localeModule_strings['profile.photos']}: ${draft.photos.length}`);
  return fields.length ? fields : [localeModule_strings['profile.empty']];
};
/** @param {string[]} values */
const listItems = (values) =>
  values.map((value) => {
    const item = document.createElement('li');
    item.textContent = value;
    return item;
  });
const saveProfile = async (call = invoke) => {
  if (!profileFormValid()) return null;
  const generation = identityState.generation;
  const view = await call('save_profile', { draft: profileDraft() }).catch((error) => {
    if (!identityCurrent(generation)) return null;
    const refusal = {
      invalidName: 'profile.nameInvalid',
      invalidHeight: 'profile.heightInvalid',
      invalidAbout: 'profile.aboutInvalid',
      invalidInterests: 'profile.interestsInvalid',
    }[String(error)];
    throw refusal ? localeModule_strings[refusal] : error;
  });
  if (!identityCurrent(generation) || !view) return null;
  showProfileInterests(view.draft.interests);
  showAge(view);
  profileMessage(view.problem ? `profile.problem.${view.problem}` : 'profile.saved');
  return view;
};
// The dialog lists exactly the public fields and asks both consents afresh each time.
/** @param {profileFieldsModule.ProfileView} view */
const openPublishDialog = (view) => {
  domById('profile-public-fields', HTMLUListElement).replaceChildren(
    ...listItems(publicFields(view.draft, placeInput.value)),
  );
  domById('profile-date-warning', HTMLParagraphElement).hidden = !(
    view.draft.birthYear &&
    view.draft.birthMonth &&
    view.draft.birthDay
  );
  profileAttested.checked = false;
  profileDisclosure.checked = false;
  publishConfirm.disabled = true;
  domById('publish-consents', HTMLDivElement).hidden = false;
  domById('publish-progress', HTMLDivElement).hidden = true;
  publishDialog.hidden = false;
  publishDialog.showModal();
  profileCancel.focus();
};
const elapsed = domById('publish-elapsed', HTMLParagraphElement);
/** @type {number | undefined} */
let elapsedTimer;
const startElapsed = () => {
  const generation = identityState.generation;
  const start = Date.now();
  const tick = () => {
    if (!identityCurrent(generation)) return;
    const seconds = Math.floor((Date.now() - start) / 1000);
    elapsed.textContent = localeModule_strings['publish.elapsed'].replace(
      '{time}',
      `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`,
    );
  };
  tick();
  elapsedTimer = setInterval(tick, 1000);
  return elapsedTimer;
};
/** @param {boolean} busy */
const setProfileBusy = (busy) => {
  profileBusy = busy;
  for (const element of domAll(
    profileForm,
    'input, select, textarea, button',
    HTMLInputElement,
    HTMLSelectElement,
    HTMLTextAreaElement,
    HTMLButtonElement,
  )) {
    element.disabled = busy;
  }
  profilePhotosController.limitProfilePhotos();
  for (const element of [profileAttested, profileDisclosure]) element.disabled = busy;
  profileHidden.disabled = busy || !profilePublished;
  switchIdentity.disabled = busy;
  removeIdentity.disabled = busy;
  for (const button of [settingsCreate, settingsSwitch, settingsRemove]) button.disabled = busy;
};
/** @param {unknown} error */
const profileFailure = (error) => {
  const code = String(error);
  if (code === 'cancelled') return profileMessage('profile.cancelled');
  if (code.startsWith('blocked: ')) return profileMessage(`profile.problem.${code.slice(9)}`);
  if (['notConnected', 'full'].includes(code)) return profileMessage(`profile.${code}`);
  // The saved draft is now what the other device published.
  if (code === 'changed') {
    return showProfile().then(
      () => profileMessage('profile.changed'),
      (failed) => profileMessage('profile.publishFailed', String(failed)),
    );
  }
  profileMessage('profile.publishFailed', code);
};
profileForm.addEventListener('input', () => {
  if (!profileBusy) profileMessage('profile.draft');
});
const submitProfile = async (save = saveProfile) => {
  if (profileBusy || !profileFormValid()) return;
  const generation = identityState.generation;
  setProfileBusy(true);
  try {
    await save();
  } catch (error) {
    if (identityCurrent(generation)) profileMessage('profile.failed', String(error));
  } finally {
    if (identityCurrent(generation)) setProfileBusy(false);
  }
};
profileForm.addEventListener('submit', (event) => {
  event.preventDefault();
  controller.profileSaveDone = submitProfile();
});
const previewAndPublish = async (save = saveProfile) => {
  if (profileBusy || !profileFormValid()) return;
  const generation = identityState.generation;
  setProfileBusy(true);
  try {
    const view = await save();
    if (identityCurrent(generation) && view && !view.problem) openPublishDialog(view);
  } catch (error) {
    if (identityCurrent(generation)) profileMessage('profile.failed', String(error));
  } finally {
    if (identityCurrent(generation)) setProfileBusy(false);
  }
};
domById('profile-publish', HTMLButtonElement).addEventListener('click', () => previewAndPublish());
for (const box of [profileAttested, profileDisclosure]) {
  box.addEventListener('change', () => {
    publishConfirm.disabled = !(profileAttested.checked && profileDisclosure.checked);
  });
}
// Cancel stops the puzzle while it runs and otherwise closes the dialog; the draft stays.
profileCancel.addEventListener('click', () => {
  if (!profileBusy) return publishDialog.close();
  const generation = identityState.generation;
  profileCancel.disabled = true;
  invoke('cancel_profile_solve').catch((error) => {
    if (identityCurrent(generation)) profileFailure(error);
  });
});
publishDialog.addEventListener('cancel', (event) => {
  event.preventDefault();
  profileCancel.click();
});
publishDialog.addEventListener('close', () => {
  publishDialog.hidden = true;
});
const confirmPublish = async (call = invoke) => {
  if (profileBusy || !profileAttested.checked || !profileDisclosure.checked) return;
  const generation = identityState.generation;
  setProfileBusy(true);
  publishConfirm.disabled = true;
  domById('publish-consents', HTMLDivElement).hidden = true;
  domById('publish-progress', HTMLDivElement).hidden = false;
  const timer = startElapsed();
  try {
    profileMessage('profile.solving');
    await call('solve_profile', { attested: true });
    if (!identityCurrent(generation)) return;
    profileMessage('profile.publishing');
    const published = await call('publish_profile', { attested: true });
    if (!identityCurrent(generation)) return;
    showSearchWait(published);
    showHidden(published);
    profileMessage('profile.published', String(published.published));
  } catch (error) {
    if (identityCurrent(generation)) await profileFailure(error);
  } finally {
    clearInterval(timer);
    if (identityCurrent(generation)) {
      publishDialog.close();
      setProfileBusy(false);
      profileCancel.disabled = false;
    }
  }
};
publishConfirm.addEventListener('click', () => {
  controller.publishDone = confirmPublish();
});
// On hides the profile with a tombstone; off publishes the saved draft again through the dialog.
const hideProfile = async (call = invoke) => {
  if (!profileHidden.checked) {
    profileHidden.checked = true;
    return previewAndPublish();
  }
  const generation = identityState.generation;
  setProfileBusy(true);
  try {
    const view = await call('hide_profile');
    if (!identityCurrent(generation)) return;
    showSearchWait(view);
    showHidden(view);
  } catch (error) {
    if (!identityCurrent(generation)) return;
    profileHidden.checked = false;
    profileFailure(error);
  } finally {
    if (identityCurrent(generation)) setProfileBusy(false);
  }
};
profileHidden.addEventListener('change', () => {
  controller.hideDone = hideProfile();
});

/**
 * @param {import('./identity-state.js').ReadyIdentity} state
 * @param {number} generation
 * @param {boolean | 'link'} fresh
 */
async function renderReady(state, generation, fresh) {
  domById('peer-id', HTMLElement).textContent = state.peerId;
  domById('file-backup', HTMLDivElement).hidden = state.mode !== 'file';
  domById('key-path', HTMLElement).textContent = state.keyPath || '';
  const items = await invoke('list_identities');
  if (!identityCurrent(generation)) return state;
  knowPeers(items);
  const sidebarIdentity = domById('sidebar-identity', HTMLDivElement);
  const [avatar, text] = who(sidebarIdentity, state.peerId, '', modeText(state));
  text.className = 'label';
  domQuery(text, ':scope > .caption', HTMLSpanElement).className = 'sub';
  sidebarIdentity.replaceChildren(avatar, text);
  await networkController.showNetwork();
  if (!identityCurrent(generation)) return state;
  progress.connectionShown = connectAtLaunch();
  await showProfile();
  if (!identityCurrent(generation)) return state;
  setProfileBusy(false);
  profileCancel.disabled = false;
  if (publishDialog.open) publishDialog.close();
  searchController.resetSearch();
  // Starts the node when its setting is on and stops another identity's node; never blocks.
  invoke('apply_node').catch(() => {});
  chatsUiController.announcePresence();
  showArea('profile-area');
  controller.statusShown = showStatus();
  wizardStep(fresh);
  show(fresh ? wizardNetwork : identity, 'status.ready');
  return state;
}

/**
 * @param {import('./identity-state.js').IdentityResponse} state
 * @param {number} [generation]
 * @param {boolean | 'link'} [fresh]
 */
async function render(state, generation = identityState.generation, fresh = false) {
  if (!identityCurrent(generation)) return state;
  if (state.status === 'ready' || state.status === 'recovery') identityState.current = state;
  if (state.status === 'ready') return renderReady(state, generation, fresh);
  if (state.status === 'picker') {
    const items = await invoke('list_identities');
    if (!identityCurrent(generation)) return state;
    domById('identities', HTMLDivElement).replaceChildren(...knowPeers(items).map(identityButton));
    show(sections[2], 'status.pick');
    return state;
  }
  if (state.status === 'recovery') {
    domById('recovery-reason', HTMLParagraphElement).textContent = reasonText(state.reason);
    // The state code and the slot row, for a bug report.
    domById('recovery-details', HTMLPreElement).textContent = [
      state.reason,
      [state.slot, state.mode].filter(Boolean).join(' '),
    ].join('\n');
    domById('recovery-more', HTMLDetailsElement).open = false;
    domById('recovery-peer', HTMLElement).textContent = state.peerId || '';
    domById('recovery-peer-row', HTMLParagraphElement).hidden = !state.peerId;
    show(sections[3], 'status.recovery');
    return state;
  }
  if (state.status !== 'setup') throw new Error('Unknown identity state');
  // Nothing is created until Create; the recommended available store is preselected.
  systemChoice.disabled = state.systemAvailable !== 'true';
  fileChoice.disabled = false;
  createIdentity.disabled = false;
  linkIdentity.disabled = false;
  (systemChoice.disabled ? fileChoice : systemChoice).checked = true;
  domById('system-reason', HTMLSpanElement).textContent = systemChoice.disabled
    ? localeModule_strings[state.systemReason]
    : '';
  show(setup, 'status.choose');
  return state;
}

// A new identity opens on the network step of the wizard, whichever way it goes on from there.
/** @param {boolean | 'link'} wizard */
const create = (wizard) => {
  const mode = systemChoice.checked ? 'system' : 'file';
  for (const each of [systemChoice, fileChoice, createIdentity, linkIdentity]) each.disabled = true;
  act(invoke('create_identity', { mode }), wizard);
};
createIdentity.addEventListener('click', () => create(true));
linkIdentity.addEventListener('click', () => create('link'));
copyFrom(
  domById('copy-recovery-details', HTMLButtonElement),
  domById('recovery-details', HTMLPreElement),
);
switchIdentity.addEventListener('click', () => act({ status: 'picker' }));
domById('add-identity', HTMLButtonElement).addEventListener('click', () =>
  act(invoke('storage_options')),
);
domById('retry', HTMLButtonElement).addEventListener('click', () => act(invoke('identity_state')));
// A failed switch or removal keeps the backend's selection, but stopped its node; rendering the
// reread state restarts that node.
bootRetry.addEventListener('click', () => {
  show(null, 'status.loading');
  domById('boot-loading', HTMLParagraphElement).hidden = false;
  act(invoke('identity_state'));
});

const startupGeneration = identityState.generation;
controller.identityShown = stringsShown
  .then(() => invoke('identity_state'))
  .then((state) => render(state, startupGeneration))
  .catch(() => {
    if (identityCurrent(startupGeneration)) showError();
    return { status: 'error' };
  });

export {
  act,
  confirmPublish,
  elapsed,
  hideProfile,
  identity,
  modeText,
  openPublishDialog,
  peerLabel,
  placeInput,
  previewAndPublish,
  profileAttested,
  profileBusy,
  profileDisclosure,
  profileDraft,
  profileFailure,
  profileHidden,
  profileMessage,
  profileStatus,
  publicFacts,
  publicFields,
  publishConfirm,
  publishDialog,
  reasonText,
  saveNodeSettings,
  saveProfile,
  sections,
  sendSearchProfile,
  setProfileBusy,
  setTray,
  show,
  showArea,
  showError,
  showProfile,
  showSearchWait,
  showStatus,
  startElapsed,
  statusMessage,
  submitProfile,
  who,
  wizardNetwork,
};
