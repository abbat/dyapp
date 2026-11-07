// Private drafts belong to one loaded profile and identity, never the public draft.
const notePanel = document.getElementById('view-note');
const noteText = document.getElementById('view-note-text');
const noteSave = document.getElementById('view-note-save');
const noteClear = document.getElementById('view-note-clear');
const noteRetry = document.getElementById('view-note-retry');
const noteStatus = document.getElementById('view-note-status');
let noteView = null;
const noteCurrent = (target) =>
  noteView === target && viewed === target.view && identityCurrent(target.generation);
const renderNote = (target) => {
  noteText.disabled = target.busy || !target.loaded;
  noteSave.disabled = target.busy || !target.loaded || noteText.value === target.saved;
  noteClear.disabled = target.busy || !target.loaded || !target.saved;
  noteRetry.hidden = target.loaded;
  noteRetry.disabled = target.busy;
  noteStatus.textContent = target.status ? window.strings[`notes.${target.status}`] : '';
  noteStatus.hidden = !target.status;
};
window.clearContactNote = () => {
  noteView = null;
  notePanel.hidden = true;
  noteText.value = '';
  noteText.setCustomValidity('');
  noteText.disabled = true;
};
window.showContactNote = async (call = invoke) => {
  window.clearContactNote();
  const view = viewed;
  if (!view || view.found.peerId === current?.peerId) return;
  const target = {
    view,
    generation: identityGeneration,
    call,
    saved: '',
    loaded: false,
    busy: true,
    status: 'loading',
  };
  noteView = target;
  notePanel.hidden = false;
  renderNote(target);
  try {
    const text = await call('contact_note', { peerId: view.found.peerId });
    if (!noteCurrent(target)) return;
    if (typeof text !== 'string') throw new Error('invalid note');
    target.saved = text;
    target.loaded = true;
    noteText.value = text;
    target.status = '';
  } catch {
    if (!noteCurrent(target)) return;
    target.status = 'loadFailed';
  } finally {
    if (noteCurrent(target)) {
      target.busy = false;
      renderNote(target);
    }
  }
};
const prepareNoteSave = (clear) => {
  const target = noteView;
  if (!target || !noteCurrent(target) || !target.loaded || target.busy) return;
  const note = clear ? '' : noteText.value;
  noteText.setCustomValidity([...note].length > 2000 ? window.strings['notes.invalid'] : '');
  if (!clear && !noteText.reportValidity()) return;
  if (note === target.saved) return;
  target.busy = true;
  target.status = 'saving';
  renderNote(target);
  return { target, note };
};
window.saveContactNote = async (clear = false) => {
  const prepared = prepareNoteSave(clear);
  if (!prepared) return;
  const { target, note } = prepared;
  try {
    await target.call('set_contact_note', { peerId: target.view.found.peerId, note });
    if (!noteCurrent(target)) return;
    target.saved = note;
    noteText.value = note;
    noteText.setCustomValidity('');
    target.status = note ? 'saved' : 'cleared';
  } catch (error) {
    if (!noteCurrent(target)) return;
    target.status = String(error) === 'invalidNote' ? 'invalid' : 'saveFailed';
  } finally {
    if (noteCurrent(target)) {
      target.busy = false;
      renderNote(target);
    }
  }
};
noteText.addEventListener('input', () => {
  noteText.setCustomValidity('');
  if (noteView && noteCurrent(noteView)) {
    noteView.status = '';
    renderNote(noteView);
  }
});
noteSave.addEventListener('click', () => {
  window.noteChanged = window.saveContactNote();
});
noteClear.addEventListener('click', () => {
  window.noteChanged = window.saveContactNote(true);
});
noteRetry.addEventListener('click', () => {
  if (noteView && noteCurrent(noteView) && !noteView.busy) {
    window.noteShown = window.showContactNote(noteView.call);
  }
});
document.addEventListener('dyapp:identity-change', window.clearContactNote);
