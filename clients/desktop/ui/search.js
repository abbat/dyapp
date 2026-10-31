// The Search area; loaded after app.js, whose helpers (invoke, showArea, placePicker,
// statusMessage) it shares through the page's global scope.
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
