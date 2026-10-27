// Practice D1: visible strings come from the matching i18n catalogue by data-i18n key.
// With --pseudo-locale each English string is accented and 40% longer.
const { invoke } = window.__TAURI__.core;
const accents = { a: 'á', e: 'é', i: 'í', o: 'ó', u: 'ú', y: 'ý', A: 'Á', E: 'É', I: 'Í', O: 'Ó', U: 'Ú' };
const pseudo = (text) => `[${[...text].map((c) => accents[c] || c).join('')}${'·'.repeat(Math.ceil(text.length * 0.4))}]`;

window.stringsShown = Promise.all([invoke('pseudo_locale'), invoke('ui_test_russian')])
  .then(async ([pseudoLocale, testRussian]) => {
    const systemLanguage = (navigator.languages?.[0] || navigator.language || 'en')
      .split(/[-_]/)[0].toLowerCase();
    const language = !pseudoLocale && (testRussian || systemLanguage === 'ru') ? 'ru' : 'en';
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
  });
const ready = document.getElementById('app-ready');
const setup = document.getElementById('setup');
const identity = document.getElementById('identity');
const systemChoice = document.getElementById('system-choice');
const fileChoice = document.getElementById('file-choice');
const storageError = document.getElementById('storage-error');

const switchIdentity = document.getElementById('switch-identity');
const removeIdentity = document.getElementById('remove-identity');
const removeConfirm = document.getElementById('remove-confirm');
const removeAction = document.getElementById('remove-action');
const settings = document.getElementById('settings');
const sections = [setup, identity, document.getElementById('picker'),
  document.getElementById('recovery'), document.getElementById('remove'), settings];
// The opened identity or recovery state the remove screen acts on.
let current = null;

// Shows one section (or none) with its status line.
const show = (section, status) => {
  if (section === identity && network.parentElement !== identity) {
    identity.insertBefore(network, document.getElementById('profile'));
  }
  for (const each of sections) each.hidden = each !== section;
  switchIdentity.hidden = section !== identity && section !== sections[3];
  // The backend refuses to remove a damaged or unreadable slot.
  removeIdentity.hidden = section !== identity && !(section === sections[3]
    && ['missing', 'incomplete', 'deleting'].includes(current?.reason));
  storageError.hidden = true;
  ready.textContent = window.strings[status];
};

const showError = () => {
  show(null, 'status.error');
  switchIdentity.hidden = true;
  storageError.textContent = window.strings['storage.error'];
  storageError.hidden = false;
};

const reasonText = (reason) => window.strings[`recovery.${reason}`]
  || window.strings['recovery.unavailable'];

const act = async (pending) => {
  try {
    return await render(await pending);
  } catch (_) {
    showError();
    return { status: 'error' };
  }
};

const identityButton = (item) => {
  const button = document.createElement('button');
  button.type = 'button';
  Object.assign(button.dataset, { status: item.status, mode: item.mode || '' });
  const name = item.status === 'ready' ? item.peerId : reasonText(item.reason);
  const mode = window.strings[`identity.mode.${item.mode}`];
  button.textContent = mode ? `${name} — ${mode}` : name;
  button.addEventListener('click', () => act(invoke('select_identity', { slot: item.slot })));
  const entry = document.createElement('li');
  entry.append(button);
  return entry;
};

const settingsIdentity = (item) => {
  const entry = document.createElement('li');
  const name = item.status === 'ready' ? item.peerId : reasonText(item.reason);
  const mode = window.strings[`identity.mode.${item.mode}`] || '';
  const active = item.slot === current.slot ? window.strings['settings.active'] : '';
  entry.textContent = [name, mode, active].filter(Boolean).join(' — ');
  entry.dataset.active = String(item.slot === current.slot);
  return entry;
};

const showSettings = async () => {
  const list = document.getElementById('settings-identities');
  const error = document.getElementById('settings-error');
  list.replaceChildren();
  error.hidden = true;
  settings.insertBefore(network, document.getElementById('settings-back'));
  show(settings, 'settings.title');
  window.nodeShown = showNode();
  try {
    const items = await invoke('list_identities');
    list.replaceChildren(...items.map(settingsIdentity));
    error.hidden = true;
  } catch (_) {
    list.replaceChildren();
    error.textContent = window.strings['storage.error'];
    error.hidden = false;
  }
};
document.getElementById('settings-open').addEventListener('click', showSettings);
document.getElementById('settings-back').addEventListener('click', () => show(identity, 'status.ready'));
document.getElementById('settings-create')
  .addEventListener('click', () => act(invoke('storage_options')));
document.getElementById('settings-switch')
  .addEventListener('click', () => act({ status: 'picker' }));

// Bootstrap settings of the open identity: draft rows, a bounded check, then Continue.
const network = document.getElementById('network');
const seeds = document.getElementById('seeds');
const networkName = document.getElementById('network-name');
const checkButton = document.getElementById('check-connection');
const cancelButton = document.getElementById('cancel-check');
const continueButton = document.getElementById('continue-connection');
const seedRows = document.getElementById('seed-rows');
const networkStatus = (key, detail) => {
  const text = window.strings[key];
  document.getElementById('network-status').textContent = detail ? `${text} ${detail}` : text;
};
const draft = () => ({
  seeds: seeds.value.split('\n').map((row) => row.trim()).filter(Boolean),
  name: networkName.value,
});

// Fills the form from the draft, else the active settings; a damaged file is shown, not reset.
const showNetwork = async () => {
  seedRows.replaceChildren();
  continueButton.disabled = true;
  try {
    const active = await invoke('load_network', { file: 'active' });
    const saved = await invoke('load_network', { file: 'draft' }) || active;
    seeds.value = saved ? saved.seeds.join('\n') : '';
    networkName.value = saved ? saved.name : '';
    networkStatus(active ? 'network.active' : 'network.idle');
  } catch (error) {
    networkStatus('network.failed', String(error));
  }
};

const rowItem = (seed, row) => {
  const entry = document.createElement('li');
  entry.dataset.status = row.status;
  const detail = row.peerId || row.reason;
  const text = `${seed}: ${window.strings[`network.row.${row.status}`]}`;
  entry.textContent = detail ? `${text} ${detail}` : text;
  return entry;
};

// A row-level problem the user must correct outranks nodes that are merely down.
const summary = (rows) => {
  const has = (...codes) => rows.some((row) => codes.includes(row.status));
  if (has('connected')) return 'network.connected';
  if (has('invalidAddress', 'invalidPeerId', 'duplicate')) return 'network.invalidAddress';
  if (has('wrongPeer', 'wrongNetwork')) return 'network.wrong';
  return 'network.unreachable';
};

const check = async () => {
  const rows = draft();
  checkButton.disabled = true;
  continueButton.disabled = true;
  cancelButton.hidden = false;
  seedRows.replaceChildren();
  networkStatus('network.checking');
  try {
    const result = await invoke('check_connection', { draft: rows, waitMs: 15000 });
    seedRows.replaceChildren(...result.map((row, at) => rowItem(rows.seeds[at], row)));
    continueButton.disabled = !result.some((row) => row.status === 'connected');
    networkStatus(summary(result));
  } catch (error) {
    const [code, ...reason] = String(error).split(': ');
    if (['empty', 'cancelled', 'invalidNetwork'].includes(code)) {
      networkStatus(`network.${code}`, reason.join(': '));
    } else {
      networkStatus('network.failed', String(error));
    }
  } finally {
    checkButton.disabled = false;
    cancelButton.hidden = true;
  }
};

network.addEventListener('submit', (event) => {
  event.preventDefault();
  window.connectionChecked = check();
});
// Edits invalidate the last success and are kept even if the app closes before a check.
network.addEventListener('input', () => { continueButton.disabled = true; });
network.addEventListener('change', () => invoke('save_network_draft', { draft: draft() })
  .catch((error) => networkStatus('network.failed', String(error))));
cancelButton.addEventListener('click', () => invoke('cancel_connection_check'));
continueButton.addEventListener('click', () => {
  continueButton.disabled = true;
  window.connectionActivated = invoke('activate_connection')
    .then(() => {
      networkStatus('network.active');
      // A node that waited for a network starts on it now.
      invoke('apply_node').catch(() => {});
    })
    .catch((error) => networkStatus('network.failed', String(error)));
});

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
const profilePreview = document.getElementById('profile-preview');
const profileAttested = document.getElementById('profile-attested');
const profileDisclosure = document.getElementById('profile-disclosure');
const profileCancel = document.getElementById('profile-cancel');
let scaleFields = [];
let profileBusy = false;
const camel = (key) => key.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
const birthValue = (part) => {
  const value = document.getElementById(`birth-${part}`).value;
  return value === '' ? null : Number(value);
};
// The place input holds a suggestion's text; placeIds maps each shown suggestion to its GeoNames
// ID, so typed text that is no suggestion fails validation instead of being saved.
const placeCountry = document.getElementById('profile-country');
const placeInput = document.getElementById('profile-place');
const placeList = document.getElementById('profile-places');
let placeIds = new Map();
const placeLabel = (place) => (place.region ? `${place.name}, ${place.region}` : place.name);
const showPlaces = (places) => {
  placeIds = new Map(places.map((place) => [placeLabel(place), String(place.id)]));
  placeList.replaceChildren(...[...placeIds.keys()].map((label) => {
    const option = document.createElement('option');
    option.value = label;
    return option;
  }));
};
const checkPlace = () => placeInput.setCustomValidity(
  placeInput.value && !placeIds.has(placeInput.value) ? window.strings['profile.placeUnknown'] : '');
const countryName = (code) => new Intl.DisplayNames([window.locale], { type: 'region' }).of(code);
placeCountry.addEventListener('input', () => {
  placeInput.value = '';
  showPlaces([]);
  checkPlace();
});
placeInput.addEventListener('input', () => {
  const text = placeInput.value;
  if (placeIds.has(text)) return checkPlace();
  window.placesShown = invoke('search_places', {
    country: placeCountry.value, prefix: text, lang: window.locale, limit: 20,
  }).then((places) => {
    if (placeInput.value === text) showPlaces(places);
    checkPlace();
  });
});
const profileDraft = () => ({
  ...Object.fromEntries(scaleFields.map((scale) => [
    camel(scale.key), Number(document.getElementById(`profile-${scale.key}`).value),
  ])),
  birthYear: birthValue('year'), birthMonth: birthValue('month'), birthDay: birthValue('day'),
  country: placeCountry.value, place: placeIds.get(placeInput.value) || '',
});
const profileMessage = (key, detail = '') => {
  profileStatus.textContent = `${window.strings[key]}${detail ? ` ${detail}` : ''}`;
};
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
  profileScales.replaceChildren(...scales.map((scale) => {
    const label = document.createElement('label');
    const select = document.createElement('select');
    select.id = `profile-${scale.key}`;
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
    row.append(label, select);
    return row;
  }));
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
  showPlaces(place ? [place] : []);
  placeInput.value = place ? placeLabel(place) : '';
  checkPlace();
  profileAttested.checked = false;
  profileDisclosure.checked = false;
  profilePreview.hidden = true;
  profileMessage(view.published ? 'profile.lastPublished' : 'profile.draft',
    view.published ? String(view.published) : '');
};
// Labelled public fields of a draft, or the empty notice.
const publicFields = (draft) => {
  const fields = scaleFields.flatMap((scale) => {
    const option = scale.choices.find((choice) => choice.code === draft[camel(scale.key)]);
    return option ? [`${window.strings[`profile.field.${scale.key}`]}: ${window.strings[
      `profile.value.${scale.key}.${option.key}`]}`] : [];
  });
  if (draft.country) fields.push(`${window.strings['profile.country']}: ${countryName(draft.country)}`);
  if (draft.place) fields.push(`${window.strings['profile.place']}: ${placeInput.value}`);
  for (const part of ['year', 'month', 'day']) {
    const value = draft[`birth${part[0].toUpperCase()}${part.slice(1)}`];
    if (value !== null) fields.push(`${window.strings[`profile.birth${part[0].toUpperCase()}${part.slice(1)}`]}: ${value}`);
  }
  return fields.length ? fields : [window.strings['profile.empty']];
};
const listItems = (values) => values.map((value) => {
  const item = document.createElement('li');
  item.textContent = value;
  return item;
});
const previewProfile = (view) => {
  document.getElementById('profile-public-fields')
    .replaceChildren(...listItems(publicFields(view.draft)));
  const [youngest, oldest] = view.ages || [];
  document.getElementById('profile-age').textContent = view.ages
    ? `${window.strings['profile.age']}: ${youngest === oldest ? youngest : `${youngest}–${oldest}`}`
    : window.strings['profile.ageUnknown'];
  document.getElementById('profile-date-warning').hidden = !(view.draft.birthYear
    && view.draft.birthMonth && view.draft.birthDay);
  profilePreview.hidden = false;
};
const saveProfile = async () => {
  if (!profileForm.reportValidity()) return null;
  const view = await invoke('save_profile', { draft: profileDraft() });
  profileMessage(view.problem ? `profile.problem.${view.problem}` : 'profile.saved');
  return view;
};
const setProfileBusy = (busy, solving = false) => {
  profileBusy = busy;
  for (const element of profileForm.querySelectorAll('input, select, button')) {
    if (element !== profileCancel) element.disabled = busy;
  }
  switchIdentity.disabled = busy;
  removeIdentity.disabled = busy;
  for (const id of ['settings-create', 'settings-switch', 'settings-remove']) {
    document.getElementById(id).disabled = busy;
  }
  profileCancel.hidden = !solving;
};
const profileFailure = (error) => {
  const code = String(error);
  if (code === 'cancelled') return profileMessage('profile.cancelled');
  if (code.startsWith('blocked: ')) return profileMessage(`profile.problem.${code.slice(9)}`);
  if (code === 'notConnected') return profileMessage('profile.notConnected');
  profileMessage('profile.publishFailed', code);
};
profileForm.addEventListener('input', (event) => {
  if (event.target !== profileAttested && event.target !== profileDisclosure && !profileBusy) {
    profileAttested.checked = false;
    profileDisclosure.checked = false;
    profilePreview.hidden = true;
    profileMessage('profile.draft');
  }
});
profileForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  if (profileBusy || !profileForm.reportValidity()) return;
  setProfileBusy(true);
  try { await saveProfile(); } catch (error) { profileMessage('profile.failed', String(error)); }
  finally { setProfileBusy(false); }
});
document.getElementById('profile-preview-action').addEventListener('click', async () => {
  if (profileBusy || !profileForm.reportValidity()) return;
  setProfileBusy(true);
  try {
    const view = await saveProfile();
    if (view) {
      profileDisclosure.checked = false;
      previewProfile(view);
    }
  } catch (error) { profileMessage('profile.failed', String(error)); }
  finally { setProfileBusy(false); }
});
profileCancel.addEventListener('click', () => {
  profileCancel.disabled = true;
  invoke('cancel_profile_solve').catch(profileFailure);
});
document.getElementById('profile-publish').addEventListener('click', async () => {
  if (profileBusy || !profileForm.reportValidity()) return;
  const attested = profileAttested.checked;
  const acknowledged = profileDisclosure.checked;
  const needsReview = profilePreview.hidden;
  setProfileBusy(true);
  try {
    const view = await saveProfile();
    previewProfile(view);
    if (view.problem) return;
    if (needsReview) return profileMessage('profile.reviewBeforePublish');
    if (!acknowledged) return profileMessage('profile.problem.notAcknowledged');
    if (!attested) return profileMessage('profile.problem.notAttested');
    profileCancel.hidden = false;
    profileMessage('profile.solving');
    await invoke('solve_profile', { attested: true });
    profileCancel.hidden = true;
    profileMessage('profile.publishing');
    const published = await invoke('publish_profile', { attested: true });
    previewProfile(published);
    showSearchWait(published);
    profileMessage('profile.published', String(published.published));
  } catch (error) {
    profileFailure(error);
  } finally {
    profileAttested.checked = false;
    profileDisclosure.checked = false;
    setProfileBusy(false);
    profileCancel.disabled = false;
  }
});

// A verified sample from the active network's search nodes. The query stays in the form, and
// filters appear only for fields every reached node offers and the client re-checks.
const searchForm = document.getElementById('search');
const searchRetry = document.getElementById('search-retry');
const searchAgain = document.getElementById('search-again');
const searchResults = document.getElementById('search-results');
const searchFailures = document.getElementById('search-failures');
const COUNTRY_FIELD = 4;
const BIRTH_AGE_FIELD = 1001;
let searchBusy = false;
const searchMessage = (key, detail = '') => {
  document.getElementById('search-status').textContent = `${window.strings[key]}${detail ? ` ${detail}` : ''}`;
};
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
    row.append(label, select);
    return row;
  }));
  showFilter(document.getElementById('search-age'), offered.includes(BIRTH_AGE_FIELD));
  showFilter(document.getElementById('search-country-row'), offered.includes(COUNTRY_FIELD));
  showFilter(scales, true);
  document.getElementById('search-filters').hidden = !offered.length;
};
const searchQuery = () => {
  const filled = (id) => !document.getElementById(id).disabled && document.getElementById(id).value.trim();
  const conditions = [...document.querySelectorAll('#search-scales select:enabled')]
    .filter((select) => select.value !== '0')
    .map((select) => ({ field: Number(select.dataset.field), op: 'eq', ints: [Number(select.value)] }));
  const country = filled('search-country');
  if (country) conditions.push({ field: COUNTRY_FIELD, op: 'eq', strings: [country.toUpperCase()] });
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
// Results show only public fields; nothing marks a self-declared claim as verified.
const foundItem = (found) => {
  const details = document.createElement('details');
  const summary = document.createElement('summary');
  summary.textContent = found.peerId;
  const fields = document.createElement('ul');
  fields.replaceChildren(...listItems(publicFields(found.draft)));
  details.append(summary, fields);
  const item = document.createElement('li');
  item.dataset.peerId = found.peerId;
  item.append(details);
  return item;
};
// Loading blocks repeated requests, not navigation.
const setSearchBusy = (busy) => {
  searchBusy = busy;
  for (const button of searchForm.querySelectorAll('button')) button.disabled = busy;
  for (const input of searchForm.querySelectorAll('input, select')) {
    input.disabled = busy || Boolean(input.closest('[hidden]'));
  }
};
const runSearch = async () => {
  if (searchBusy || !searchForm.reportValidity()) return;
  const query = searchQuery();
  setSearchBusy(true);
  searchRetry.hidden = true;
  searchAgain.hidden = true;
  searchMessage('search.loading');
  try {
    const result = await invoke('search_profiles', { query });
    showSearchFilters(result.offered);
    searchFailures.replaceChildren(...listItems(result.failures.map(failureText)));
    searchResults.replaceChildren(...result.profiles.map(foundItem));
    const count = String(result.profiles.length);
    if (result.partial) searchMessage('search.partial', count);
    else if (!result.profiles.length) searchMessage('search.empty');
    else searchMessage('search.results', count);
    searchRetry.hidden = !result.partial;
    searchAgain.hidden = false;
  } catch (error) {
    searchFailures.replaceChildren();
    searchResults.replaceChildren();
    const code = String(error);
    if (code === 'notConnected') searchMessage('search.notConnected');
    else searchMessage('search.failed', code);
    searchRetry.hidden = false;
  } finally {
    setSearchBusy(false);
  }
};
const resetSearch = () => {
  searchForm.reset();
  showSearchFilters([]);
  searchFailures.replaceChildren();
  searchResults.replaceChildren();
  searchRetry.hidden = true;
  searchAgain.hidden = true;
  searchMessage('search.idle');
};
searchForm.addEventListener('submit', (event) => {
  event.preventDefault();
  window.searchDone = runSearch();
});
for (const button of [searchRetry, searchAgain]) {
  button.addEventListener('click', () => { window.searchDone = runSearch(); });
}

async function render(state) {
  if (state.status === 'ready' || state.status === 'recovery') current = state;
  if (state.status === 'ready') {
    document.getElementById('peer-id').textContent = state.peerId;
    document.getElementById('file-backup').hidden = state.mode !== 'file';
    document.getElementById('key-path').textContent = state.keyPath || '';
    await showNetwork();
    await showProfile();
    resetSearch();
    // Starts the node when its setting is on and stops another identity's node; never blocks.
    invoke('apply_node').catch(() => {});
    show(identity, 'status.ready');
    return state;
  }
  if (state.status === 'picker') {
    const items = await invoke('list_identities');
    document.getElementById('identities').replaceChildren(...items.map(identityButton));
    show(sections[2], 'status.pick');
    return state;
  }
  if (state.status === 'recovery') {
    document.getElementById('recovery-reason').textContent = reasonText(state.reason);
    document.getElementById('recovery-peer').textContent = state.peerId || '';
    show(sections[3], 'status.recovery');
    return state;
  }
  if (state.status !== 'setup') throw new Error('Unknown identity state');
  systemChoice.disabled = state.systemAvailable !== 'true';
  fileChoice.disabled = false;
  document.getElementById('system-reason').textContent = systemChoice.disabled
    ? window.strings[state.systemReason] : '';
  show(setup, 'status.choose');
  return state;
}

const choose = (mode) => {
  systemChoice.disabled = true;
  fileChoice.disabled = true;
  return act(invoke('create_identity', { mode }));
};

systemChoice.addEventListener('click', () => choose('system'));
fileChoice.addEventListener('click', () => choose('file'));
switchIdentity.addEventListener('click', () => act({ status: 'picker' }));
document.getElementById('add-identity')
  .addEventListener('click', () => act(invoke('storage_options')));
document.getElementById('retry')
  .addEventListener('click', () => act(invoke('identity_state')));

// Second confirmation: Remove stays disabled until the checkbox is ticked.
let removeReturnToSettings = false;
const showRemove = async () => {
  const items = await invoke('list_identities');
  const copies = current.peerId && items.some((item) => item.slot !== current.slot
    && item.status === 'ready' && item.peerId === current.peerId);
  document.getElementById('remove-peer').textContent = current.peerId
    || reasonText(current.reason);
  document.getElementById('remove-mode').textContent = window.strings[
    `identity.mode.${current.mode}`] || '';
  const path = document.getElementById('remove-path');
  path.hidden = !current.keyPath;
  path.querySelector('code').textContent = current.keyPath || '';
  document.getElementById('remove-copies').textContent = !current.peerId ? ''
    : window.strings[copies ? 'remove.otherCopy' : 'remove.lastCopy'];
  // A missing key or another copy means nothing irreplaceable is lost.
  document.getElementById('remove-warning').hidden = !current.peerId || copies;
  removeConfirm.checked = false;
  removeAction.disabled = true;
  show(sections[4], 'remove.title');
};
removeIdentity.addEventListener('click', () => {
  removeReturnToSettings = false;
  showRemove().catch(showError);
});
document.getElementById('settings-remove').addEventListener('click', () => {
  removeReturnToSettings = true;
  showRemove().catch(showError);
});
removeConfirm.addEventListener('change', () => { removeAction.disabled = !removeConfirm.checked; });
removeAction.addEventListener('click', () => {
  removeAction.disabled = true;
  act(invoke('remove_identity', { slot: current.slot, peerId: current.peerId || null }));
});
document.getElementById('remove-cancel').addEventListener('click', () => {
  if (removeReturnToSettings) {
    removeReturnToSettings = false;
    showSettings();
  } else {
    act(current);
  }
});
window.identityShown = window.stringsShown
  .then(() => invoke('identity_state'))
  .then(render)
  .catch(() => { showError(); return { status: 'error' }; });
