// The list reads local IDs and labels; only pressing a row fetches a verified profile.
const savedPanel = document.getElementById('saved-profiles');
const savedRows = document.getElementById('saved-profile-rows');
const savedStatus = document.getElementById('saved-profile-status');
const savedRefresh = document.getElementById('saved-profile-refresh');
let savedRun = 0;
let savedSelection = 0;
let savedCall = invoke;
const savedCurrent = (context) => context.run === savedRun
  && identityCurrent(context.generation) && savedPanel.open;
const savedMessage = (key) => { savedStatus.textContent = key ? window.strings[`favorites.${key}`] : ''; };
const savedRow = (peerId, context) => {
  const row = document.createElement('li');
  row.className = 'actions';
  row.dataset.peerId = peerId;
  const open = document.createElement('button');
  open.type = 'button';
  open.className = 'btn';
  open.textContent = window.peerName(peerId);
  const remove = document.createElement('button');
  remove.type = 'button';
  remove.className = 'btn quiet';
  remove.textContent = window.strings['favorites.remove'];
  remove.setAttribute('aria-label', `${window.strings['favorites.remove']} ${window.peerName(peerId)}`);
  let busy = false;
  const run = async (opening) => {
    if (busy || !savedCurrent(context) || !row.isConnected) return;
    const selection = opening ? ++savedSelection : savedSelection;
    busy = true;
    open.disabled = true;
    remove.disabled = true;
    savedMessage(opening ? 'opening' : 'saving');
    try {
      if (opening) {
        const [found] = await withPlaces([await context.call('shared_profile', { peerId })]);
        if (selection === savedSelection && savedCurrent(context) && row.isConnected && !searchMain.hidden
          && !document.getElementById('search-area').hidden) openFound(found, open, context.call);
      } else {
        await context.call('set_favorite', { peerId, favorite: false });
        if (!savedCurrent(context) || !row.isConnected) return;
        await window.showSaved(context.call);
        if (savedPanel.open && identityCurrent(context.generation)) savedRefresh.focus();
      }
    } catch {
      if ((!opening || selection === savedSelection) && savedCurrent(context) && row.isConnected) {
        savedMessage(opening ? 'openFailed' : 'changeFailed');
      }
    } finally {
      if (savedCurrent(context) && row.isConnected) {
        busy = false;
        open.disabled = false;
        remove.disabled = false;
      }
    }
  };
  open.addEventListener('click', () => { window.savedOpened = run(true); });
  remove.addEventListener('click', () => { window.savedRemoved = run(false); });
  row.append(open, remove);
  return row;
};
window.showSaved = async (call = savedCall) => {
  savedCall = call;
  const context = { run: ++savedRun, generation: identityGeneration, call };
  savedRows.replaceChildren();
  savedRefresh.disabled = true;
  savedMessage('listLoading');
  try {
    const peers = await call('favorite_peers');
    if (!savedCurrent(context)) return;
    await loadPeerNames(peers, call);
    if (!savedCurrent(context)) return;
    savedRows.replaceChildren(...peers.map((peer) => savedRow(peer, context)));
    savedMessage(peers.length ? '' : 'empty');
  } catch {
    if (savedCurrent(context)) savedMessage('listFailed');
  } finally {
    if (savedCurrent(context)) savedRefresh.disabled = false;
  }
};
window.refreshSaved = () => {
  if (savedPanel.open && !searchMain.hidden && !document.getElementById('search-area').hidden) {
    window.savedShown = window.showSaved();
  }
};
const closeSaved = () => {
  ++savedRun;
  savedPanel.open = false;
  savedRows.replaceChildren();
  savedMessage('');
  savedRefresh.disabled = false;
};
savedPanel.addEventListener('toggle', () => {
  if (savedPanel.open) {
    if (!savedRefresh.disabled) window.savedShown = window.showSaved();
  } else {
    ++savedRun;
    savedRefresh.disabled = false;
  }
});
savedRefresh.addEventListener('click', () => { window.savedShown = window.showSaved(); });
document.addEventListener('dyapp:identity-change', () => { closeSaved(); savedCall = invoke; });
document.addEventListener('dyapp:area-change', (event) => {
  if (event.detail !== 'search-area') closeSaved();
});
