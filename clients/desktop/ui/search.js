import { statusMessage, who, peerLabel, publicFacts, showArea } from './app.js';
import { copyFrom, controller as clipboardController } from './clipboard.js';
import { controller as contactNotesController } from './contact-notes.js';
import { controller as favoritesController } from './favorites.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { shortPeer, addKnownPeers } from './peer-labels.js';
import { loadPeerNames, controller as peerNamesController } from './peer-names.js';
import { controller as peerPhotosController } from './peer-photos.js';
import { placePicker, controller as placesController } from './places.js';
import * as profileFieldsModule from './profile-fields.js';
import { controller as savedProfilesController } from './saved-profiles.js';
import { progress } from './ui-progress.js';
export const controller = {};
export const cameraState = { stream: null };

// The Search area imports its identity, navigation, place and status helpers.
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
const searchPlace = placePicker(
  searchCountry,
  document.getElementById('search-place'),
  document.getElementById('search-places'),
  searchCountryCode,
);
const BIRTH_AGE_FIELD = 1001;
const HEIGHT_FIELD = 46;
const SEEKING_GENDERS_FIELD = 53;
const SEEKING_AGE_FIELD = 54;
let searchBusy = false;
const searchMessage = (key, detail = '') =>
  statusMessage(document.getElementById('search-status'), 'search-open-network', key, detail);
// Hidden filters are disabled, so they neither block validation nor reach the query.
const showFilter = (element, shown) => {
  element.hidden = !shown;
  for (const input of element.querySelectorAll('input, select'))
    input.disabled = !shown || searchBusy;
};
const showSearchFilters = (offered) => {
  const scales = document.getElementById('search-scales');
  const kept = new Map(
    [...scales.querySelectorAll('select')].map((select) => [select.id, select.value]),
  );
  scales.replaceChildren(
    ...profileFieldsModule.scaleFields
      .filter((scale) => offered.includes(scale.field))
      .map((scale) => {
        const label = document.createElement('label');
        const select = document.createElement('select');
        select.id = `search-${scale.key}`;
        select.dataset.field = String(scale.field);
        label.htmlFor = select.id;
        label.textContent = strings[`profile.field.${scale.key}`];
        const any = document.createElement('option');
        any.value = '0';
        any.textContent = strings['search.any'];
        select.append(
          any,
          ...scale.choices.map((choice) => {
            const option = document.createElement('option');
            option.value = String(choice.code);
            option.textContent = strings[`profile.value.${scale.key}.${choice.key}`];
            return option;
          }),
        );
        select.value = kept.get(select.id) || '0';
        const row = document.createElement('div');
        row.className = 'field';
        row.append(label, select);
        return row;
      }),
  );
  showFilter(document.getElementById('search-age'), offered.includes(BIRTH_AGE_FIELD));
  showFilter(document.getElementById('search-height'), offered.includes(HEIGHT_FIELD));
  showFilter(document.getElementById('search-interest-row'), offered.includes(19));
  showFilter(document.getElementById('search-country-row'), offered.includes(COUNTRY_FIELD));
  showFilter(
    document.getElementById('search-place-row'),
    offered.includes(COUNTRY_FIELD) && offered.includes(PLACE_FIELD),
  );
  showFilter(scales, true);
  showFilter(
    document.getElementById('search-mutual-row'),
    offered.includes(SEEKING_GENDERS_FIELD) && offered.includes(SEEKING_AGE_FIELD),
  );
  document.getElementById('search-filters').hidden = !offered.length;
};
const searchEmployer = () => {
  const company = document.getElementById('search-company').value.trim();
  const kind = company ? document.getElementById('search-employer').value : 'any';
  if (kind === 'same') return { kind, company };
  if (kind === 'exclude') {
    const companies = company
      .split(';')
      .map((name) => name.trim())
      .filter(Boolean);
    return { kind, companies };
  }
  return { kind: 'any' };
};
const searchQuery = () => {
  const filled = (id) =>
    !document.getElementById(id).disabled && document.getElementById(id).value.trim();
  const conditions = [...document.querySelectorAll('#search-scales select:enabled')]
    .filter((select) => select.value !== '0')
    .map((select) => ({
      field: Number(select.dataset.field),
      op: 'eq',
      ints: [Number(select.value)],
    }));
  if (filled('search-country'))
    conditions.push({ field: COUNTRY_FIELD, op: 'eq', strings: [searchCountryCode()] });
  if (filled('search-interest'))
    conditions.push({ field: 19, op: 'eq', strings: [filled('search-interest')] });
  const place = filled('search-place') && searchPlace.id();
  if (place) conditions.push({ field: PLACE_FIELD, op: 'eq', strings: [place] });
  const [min, max] = [filled('search-age-min'), filled('search-age-max')];
  const age = min || max ? [Number(min || 18), Number(max || 150)] : null;
  const [from, to] = [filled('search-height-min'), filled('search-height-max')];
  if (from || to)
    conditions.push({
      field: HEIGHT_FIELD,
      op: 'range',
      ints: [Number(from || 50), Number(to || 272)],
    });
  const employer = searchEmployer();
  const mutual = document.getElementById('search-mutual');
  return { age, conditions, employer, limit: 0, mutual: mutual.checked && !mutual.disabled };
};
const failureText = (failure) => {
  const text = strings[`search.failure.${failure.kind}`];
  return failure.kind === 'rateLimited'
    ? `${text} ${Math.ceil(failure.retryAfterMs / 1000)}`
    : text;
};
// Age in years from public birth fields today, a range while the birthday may be ahead.
const ageText = (draft) => {
  if (draft.birthYear === null) return '';
  const now = new Date();
  const years = now.getFullYear() - draft.birthYear;
  const [month, day] = [now.getMonth() + 1, now.getDate()];
  const passed =
    draft.birthMonth === null
      ? null
      : draft.birthMonth !== month
        ? draft.birthMonth < month
        : draft.birthDay === null
          ? null
          : draft.birthDay <= day;
  return passed === null ? `${years - 1}–${years}` : String(passed ? years : years - 1);
};
const genderText = (draft) => {
  const scale = profileFieldsModule.scaleFields.find((field) => field.key === 'gender');
  const option = scale?.choices.find((choice) => choice.code === draft.gender);
  return option ? strings[`profile.value.gender.${option.key}`] : '';
};
const summaryText = (found) =>
  [genderText(found.draft), ageText(found.draft), found.place].filter(Boolean).join(' · ');
// Cards show the letters of the short ID until `showCardPhotos` draws over them; nothing marks a
// self-declared claim as verified.
const resultCard = (found, id) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'list-button';
  button.id = id;
  button.dataset.peerId = found.peerId;
  button.append(...who(button, found.peerId, '', summaryText(found)));
  button.addEventListener('click', () => openFound(found, button));
  return button;
};
const searchMain = document.getElementById('search-main');
const profileView = document.getElementById('profile-view');
const withPlaces = (profiles) =>
  Promise.all(
    profiles.map(async (profile) => ({
      ...profile,
      place: await placesController.placeText(profile.draft.country, profile.draft.place),
    })),
  );
const viewStatus = document.getElementById('view-status');
const showLikes = (likes) => {
  document.getElementById('view-like').hidden = likes.ours || likes.passed;
  document.getElementById('view-unlike').hidden = !likes.ours;
  document.getElementById('view-match').hidden = !likes.matched;
  document.getElementById('view-pass').hidden = likes.ours || likes.passed;
  document.getElementById('view-unpass').hidden = !likes.passed;
};
let viewed = null;
let safetyScanToken = 0;

let cameraTimer = null;
let cameraPending = false;
const showSafetyStatus = (key) => {
  const status = document.getElementById('view-safety-scan-status');
  status.textContent = strings[key];
  status.hidden = false;
};
const stopSafetyCamera = () => {
  safetyScanToken += 1;
  clearTimeout(cameraTimer);
  cameraTimer = null;
  cameraPending = false;
  cameraState.stream?.getTracks().forEach((track) => {
    track.stop();
  });
  cameraState.stream = null;
  const preview = document.getElementById('view-safety-preview');
  preview.srcObject = null;
  preview.hidden = true;
  const button = document.getElementById('view-safety-camera');
  button.textContent = strings['view.safety.camera.start'];
};
document.addEventListener('dyapp:identity-change', stopSafetyCamera);
document.addEventListener('dyapp:area-change', (event) => {
  if (event.detail !== 'search-area') stopSafetyCamera();
});
document.addEventListener('visibilitychange', () => {
  if (document.hidden) stopSafetyCamera();
});
const showVerified = (verified) => {
  document.getElementById('view-verified').hidden = !verified;
  const state = document.getElementById('view-safety-state');
  state.dataset.i18n = verified ? 'view.safety.verified' : 'view.safety.unverified';
  state.textContent = strings[state.dataset.i18n];
  document.getElementById('view-confirm').hidden = verified;
  document.getElementById('view-unverify').hidden = !verified;
};
const showFound = (found) => {
  const short = shortPeer(found.peerId);
  peerLabel(document.getElementById('view-peer'), found.peerId);
  document.getElementById('view-peer').textContent = short;
  document.getElementById('view-title').textContent = peerNamesController.peerName(found.peerId);
  document.getElementById('view-summary').textContent = summaryText(found);
  document.getElementById('view-fields').replaceChildren(
    ...publicFacts(found.draft, found.place).flatMap(([label, value]) => {
      const term = document.createElement('dt');
      const detail = document.createElement('dd');
      term.textContent = label;
      detail.textContent = value;
      return [term, detail];
    }),
  );
};
// A card from Chats opens the profile in Search too; Back returns to the card's area.
const openFound = (found, card, call = invoke) => {
  const generation = identityState.generation;
  stopSafetyCamera();
  const target = { found, card, actionStarted: false };
  viewed = target;
  for (const id of ['view-like', 'view-unlike', 'view-pass', 'view-unpass'])
    document.getElementById(id).disabled = false;
  document.getElementById('view-message').disabled = false;
  document.getElementById('view-safety-image').value = '';
  document.getElementById('view-safety-scan-status').hidden = true;
  if (!card.closest('#search-area')) showArea('search-area');
  viewStatus.textContent = '';
  document.getElementById('view-safety-panel').hidden = true;
  document.getElementById('view-safety-number').textContent = '';
  const qr = document.getElementById('view-safety-qr');
  qr.width = 0;
  qr.height = 0;
  document.getElementById('view-safety').setAttribute('aria-expanded', 'false');
  document.getElementById('view-confirm').disabled = false;
  document.getElementById('view-unverify').disabled = false;
  showVerified(false);
  call('contact_verified', { peerId: found.peerId }).then(
    (verified) => {
      if (identityCurrent(generation) && viewed === target) showVerified(verified === true);
    },
    () => {},
  );
  showLikes({ ours: false, matched: false });
  call('peer_likes', { peerId: found.peerId }).then(
    (likes) => {
      if (identityCurrent(generation) && viewed === target && !target.actionStarted)
        showLikes(likes);
    },
    () => {},
  );
  showFound(found);
  controller.favoriteShown = favoritesController.showFavorite(call);
  progress.noteShown = contactNotesController.showContactNote(call);
  controller.peerPhotosShown = peerPhotosController.showFoundPhotos(found, call);
  searchMain.hidden = true;
  profileView.hidden = false;
  document.getElementById('view-back').focus();
};
const closeFound = () => {
  stopSafetyCamera();
  favoritesController.clearFavorite();
  contactNotesController.clearContactNote();
  document.getElementById('view-message').disabled = true;
  profileView.hidden = true;
  searchMain.hidden = false;
  if (viewed?.card.isConnected && !viewed.card.hidden) viewed.card.focus();
  viewed = null;
};
document.getElementById('view-back').addEventListener('click', () => {
  const { card } = viewed;
  const area = card.closest('section')?.id;
  closeFound();
  if (area === 'search-area') {
    savedProfilesController.refreshSaved();
    return;
  }
  if (!area || !card.isConnected) return;
  showArea(area);
  card.focus();
});
// A like goes to the pair group on the active network; the first one invites the peer.
const sendLike = async (liked, call = invoke) => {
  const target = viewed;
  if (!target || target.actionBusy) return;
  const { found } = target;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed === target;
  target.actionBusy = true;
  target.actionStarted = true;
  const buttons = ['view-like', 'view-unlike', 'view-pass', 'view-unpass'].map((id) =>
    document.getElementById(id),
  );
  for (const button of buttons) button.disabled = true;
  viewStatus.textContent = strings['view.sending'];
  try {
    const likes = await call('like_peer', { peerId: found.peerId, liked });
    if (!currentView()) return;
    showLikes(likes);
    viewStatus.textContent = '';
  } catch (error) {
    if (!currentView()) return;
    const code = String(error);
    viewStatus.textContent =
      strings[`view.like.${code}`] ?? `${strings['view.like.failed']} ${code}`;
    // A failed send can still be staged, so the stored state decides the button and says that
    // the catch-up pass will retry it.
    await call('peer_likes', { peerId: found.peerId }).then(
      (likes) => {
        if (!currentView()) return;
        showLikes(likes);
        if (likes.ours === liked) viewStatus.textContent = strings['view.like.pending'];
      },
      () => {},
    );
  } finally {
    target.actionBusy = false;
    if (currentView()) for (const button of buttons) button.disabled = false;
  }
};
document.getElementById('view-like').addEventListener('click', () => {
  controller.likeDone = sendLike(true);
});
const unlikeDialog = document.getElementById('unlike-dialog');
document.getElementById('view-unlike').addEventListener('click', () => {
  unlikeDialog.hidden = false;
  unlikeDialog.showModal();
  document.getElementById('unlike-cancel').focus();
});
unlikeDialog.addEventListener('close', () => {
  unlikeDialog.hidden = true;
});
document.getElementById('unlike-cancel').addEventListener('click', () => unlikeDialog.close());
document.getElementById('unlike-confirm').addEventListener('click', () => {
  unlikeDialog.close();
  controller.likeDone = sendLike(false);
});
// A pass stays with the own devices: the card leaves its list at once and the open profile can undo it.
const sendPass = async (passed, call = invoke) => {
  const target = viewed;
  if (!target || target.actionBusy) return;
  const { found, card } = target;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed === target;
  target.actionBusy = true;
  target.actionStarted = true;
  const buttons = ['view-like', 'view-unlike', 'view-pass', 'view-unpass'].map((id) =>
    document.getElementById(id),
  );
  for (const button of buttons) button.disabled = true;
  viewStatus.textContent = strings['view.pass.saving'];
  try {
    const likes = await call('pass_peer', { peerId: found.peerId, passed });
    if (!currentView()) return;
    showLikes(likes);
    card.hidden = passed;
    if (card.closest('#chats-liked-list')) {
      document.getElementById('chats-liked-count').textContent = String(
        document.querySelectorAll('#chats-liked-list [data-peer-id]:not([hidden])').length,
      );
    }
    viewStatus.textContent = strings[passed ? 'view.passed' : 'view.unpassed'];
  } catch (error) {
    if (currentView()) viewStatus.textContent = `${strings['view.pass.failed']} ${String(error)}`;
  } finally {
    target.actionBusy = false;
    if (currentView()) for (const button of buttons) button.disabled = false;
  }
};
for (const [id, passed] of [
  ['view-pass', true],
  ['view-unpass', false],
]) {
  document.getElementById(id).addEventListener('click', () => {
    controller.passDone = sendPass(passed);
  });
}
copyFrom(
  document.getElementById('copy-view-peer'),
  document.getElementById('view-peer'),
  () => viewed.found.peerId,
);
const copySafety = (call = invoke) => {
  const button = document.getElementById('copy-safety-number');
  const source = document.getElementById('view-safety-number');
  const found = viewed?.found;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  const token = ++clipboardController.clipboardCopyToken;
  return call('copy_safety_number', { peerId: found?.peerId }).then(
    (copy) => {
      if (currentView()) {
        button.textContent = strings['action.copied'];
        setTimeout(() => {
          if (currentView()) button.textContent = strings['view.safety.copy'];
        }, 2000);
      }
      // Clear the old sensitive value even if the user switched identities before copy completed.
      setTimeout(() => {
        controller.safetyClearDone =
          token === clipboardController.clipboardCopyToken
            ? invoke('clear_safety_clipboard', { expected: copy }).catch(() => false)
            : Promise.resolve(false);
      }, 60000);
    },
    () => {
      if (currentView()) window.getSelection().selectAllChildren(source);
    },
  );
};
document.getElementById('copy-safety-number').addEventListener('click', () => {
  controller.safetyCopied = copySafety();
});
const drawContactQr = ({ size, modules }, canvas = document.getElementById('view-safety-qr')) => {
  if (!Number.isInteger(size) || size < 21 || size > 177 || modules?.length !== size * size) {
    throw new Error('invalid contact QR matrix');
  }
  const scale = 6;
  canvas.width = (size + 8) * scale;
  canvas.height = canvas.width;
  const context = canvas.getContext('2d');
  context.fillStyle = '#fff';
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.fillStyle = '#000';
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      if (modules[y * size + x]) context.fillRect((x + 4) * scale, (y + 4) * scale, scale, scale);
    }
  }
};
const loadSafety = (call = invoke) => {
  const { found } = viewed;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  const button = document.getElementById('view-safety');
  const panel = document.getElementById('view-safety-panel');
  if (!panel.hidden) {
    stopSafetyCamera();
    panel.hidden = true;
    button.setAttribute('aria-expanded', 'false');
    return Promise.resolve();
  }
  button.disabled = true;
  return Promise.all([
    call('contact_safety_number', { peerId: found.peerId }),
    call('contact_safety_qr', { peerId: found.peerId }),
  ])
    .then(([number, matrix]) => {
      if (!currentView()) return;
      drawContactQr(matrix);
      document.getElementById('view-safety-number').textContent = number.match(/.{5}/g).join(' ');
      panel.hidden = false;
      button.setAttribute('aria-expanded', 'true');
    })
    .catch(() => {
      if (currentView()) viewStatus.textContent = strings['view.safety.failed'];
    })
    .finally(() => {
      if (currentView()) button.disabled = false;
    });
};
document.getElementById('view-safety').addEventListener('click', () => {
  controller.safetyDone = loadSafety();
});
// Only this path writes the mark; the answer is the signed profile the mark was made from.
const confirmContact = (call = invoke) => {
  const { found, card } = viewed;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  const button = document.getElementById('view-confirm');
  button.disabled = true;
  showSafetyStatus('view.safety.confirm.checking');
  return call('confirm_contact', { peerId: found.peerId }).then(
    async (fresh) => {
      if (!currentView()) return;
      button.disabled = false;
      showVerified(true);
      showSafetyStatus('view.safety.confirm.done');
      document.getElementById('view-unverify').focus();
      const [placed] = await withPlaces([fresh]).catch(() => [{ ...fresh, place: found.place }]);
      await loadPeerNames([found.peerId], call);
      if (!currentView()) return;
      Object.assign(found, placed);
      showFound(found);
      card.replaceChildren(...who(card, found.peerId, '', summaryText(found)));
      peerPhotosController.showCardPhotos([found], [card], call);
    },
    (error) => {
      if (!currentView()) return;
      const key = `view.safety.confirm.${error}`;
      showSafetyStatus(strings[key] ? key : 'view.safety.confirm.failed');
      button.disabled = false;
      button.focus();
    },
  );
};
const removeMark = (call = invoke) => {
  const { found, card } = viewed;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  const button = document.getElementById('view-unverify');
  button.disabled = true;
  return call('unverify_contact', { peerId: found.peerId }).then(
    async () => {
      if (!currentView()) return;
      button.disabled = false;
      showVerified(false);
      showSafetyStatus('view.safety.confirm.removed');
      document.getElementById('view-confirm').focus();
      await loadPeerNames([found.peerId], call);
      if (!currentView()) return;
      showFound(found);
      card.replaceChildren(...who(card, found.peerId, '', summaryText(found)));
      peerPhotosController.showCardPhotos([found], [card], call);
    },
    () => {
      if (!currentView()) return;
      button.disabled = false;
      showSafetyStatus('view.safety.confirm.removeFailed');
      button.focus();
    },
  );
};
const verifyDialog = document.getElementById('verify-dialog');
document.getElementById('view-confirm').addEventListener('click', () => {
  verifyDialog.hidden = false;
  verifyDialog.showModal();
  document.getElementById('verify-cancel').focus();
});
verifyDialog.addEventListener('close', () => {
  verifyDialog.hidden = true;
});
document.addEventListener('dyapp:identity-change', () => {
  if (verifyDialog.open) verifyDialog.close();
});
document.getElementById('verify-cancel').addEventListener('click', () => verifyDialog.close());
document.getElementById('verify-confirm').addEventListener('click', () => {
  verifyDialog.close();
  controller.confirmDone = confirmContact();
});
document.getElementById('view-unverify').addEventListener('click', () => {
  controller.confirmDone = removeMark();
});
const grayscaleFrame = (context, width, height) => {
  const rgba = context.getImageData(0, 0, width, height).data;
  const pixels = new Array(width * height);
  for (let i = 0; i < pixels.length; i += 1) {
    const at = i * 4;
    const luma = Math.round((rgba[at] * 299 + rgba[at + 1] * 587 + rgba[at + 2] * 114) / 1000);
    pixels[i] = Math.round((luma * rgba[at + 3] + 255 * (255 - rgba[at + 3])) / 255);
  }
  return pixels;
};
// The UI smoke passes a PNG made from the rendered canvas through this same file path.
controller.scanQrFile = async (file) => {
  const found = viewed?.found;
  const generation = identityState.generation;
  const panel = document.getElementById('view-safety-panel');
  const token = ++safetyScanToken;
  const current = () =>
    identityCurrent(generation) &&
    token === safetyScanToken &&
    viewed?.found === found &&
    !panel.hidden;
  const report = (key) => {
    if (!current()) return;
    showSafetyStatus(`view.safety.scan.${key}`);
  };
  if (!file || !found) return;
  if (!['image/png', 'image/jpeg'].includes(file.type)) return report('unsupported');
  if (file.size > 2 * 1024 * 1024) return report('tooLarge');
  report('checking');
  let bitmap;
  try {
    // A File source needs no blob: URL; the desktop CSP allows only self images.
    bitmap = await createImageBitmap(file);
    if (!current()) return;
    if (!bitmap.width || !bitmap.height || bitmap.width * bitmap.height > 16_000_000) {
      return report('tooLarge');
    }
    const scale = Math.min(1, 768 / Math.max(bitmap.width, bitmap.height));
    const width = Math.max(1, Math.round(bitmap.width * scale));
    const height = Math.max(1, Math.round(bitmap.height * scale));
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    context.fillStyle = '#fff';
    context.fillRect(0, 0, width, height);
    context.drawImage(bitmap, 0, 0, width, height);
    const pixels = grayscaleFrame(context, width, height);
    const result = await invoke('scan_contact_qr', { peerId: found.peerId, width, height, pixels });
    report(result);
  } catch (_) {
    report(bitmap ? 'failed' : 'loadFailed');
  } finally {
    bitmap?.close();
  }
};
const safetyCameraFrame = (preview) => {
  if (!preview.videoWidth || !preview.videoHeight) return null;
  const scale = Math.min(1, 768 / Math.max(preview.videoWidth, preview.videoHeight));
  const width = Math.max(1, Math.round(preview.videoWidth * scale));
  const height = Math.max(1, Math.round(preview.videoHeight * scale));
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  context.drawImage(preview, 0, 0, width, height);
  return { width, height, pixels: grayscaleFrame(context, width, height) };
};
const scanSafetyCameraFrame = async (found, frame, current) => {
  const result = await invoke('scan_contact_qr', { peerId: found.peerId, ...frame });
  if (!current()) return true;
  if (result === 'notFound' || result === 'unreadable') return false;
  stopSafetyCamera();
  showSafetyStatus(`view.safety.scan.${result}`);
  return true;
};
const safetyCameraScanner = (found, preview, current) => {
  const scan = async () => {
    if (!current() || !cameraState.stream) return;
    try {
      const frame = safetyCameraFrame(preview);
      if (frame && (await scanSafetyCameraFrame(found, frame, current))) return;
      if (!current()) return;
      if (cameraState.stream.getVideoTracks()[0]?.readyState === 'ended') {
        stopSafetyCamera();
        showSafetyStatus('view.safety.camera.unavailable');
        return;
      }
      cameraTimer = setTimeout(scan, 300);
    } catch (_) {
      if (current()) {
        stopSafetyCamera();
        showSafetyStatus('view.safety.camera.failed');
      }
    }
  };
  return scan;
};
const startSafetyCamera = async (found, preview, current) => {
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: { width: { ideal: 640 }, height: { ideal: 480 } },
    });
    if (!current()) {
      stream.getTracks().forEach((track) => {
        track.stop();
      });
      return;
    }
    cameraState.stream = stream;
    cameraPending = false;
    preview.srcObject = stream;
    preview.hidden = false;
    await preview.play();
    if (!current()) {
      if (cameraState.stream === stream) stopSafetyCamera();
      return;
    }
    showSafetyStatus('view.safety.camera.scanning');
    const scan = safetyCameraScanner(found, preview, current);
    cameraTimer = setTimeout(scan, 300);
  } catch (error) {
    if (!current()) return;
    const kind = ['NotAllowedError', 'PermissionDeniedError'].includes(error?.name)
      ? 'denied'
      : ['NotFoundError', 'DevicesNotFoundError', 'OverconstrainedError'].includes(error?.name)
        ? 'unavailable'
        : 'failed';
    stopSafetyCamera();
    showSafetyStatus(`view.safety.camera.${kind}`);
  }
};
document.getElementById('view-safety-camera').addEventListener('click', () => {
  if (cameraPending || cameraState.stream) {
    stopSafetyCamera();
    showSafetyStatus('view.safety.camera.stopped');
    return;
  }
  const found = viewed?.found;
  const generation = identityState.generation;
  const panel = document.getElementById('view-safety-panel');
  if (!found || panel.hidden) return;
  if (!navigator.mediaDevices?.getUserMedia) {
    showSafetyStatus('view.safety.camera.unsupported');
    return;
  }
  const token = ++safetyScanToken;
  const current = () =>
    identityCurrent(generation) &&
    token === safetyScanToken &&
    viewed?.found === found &&
    !panel.hidden &&
    !document.getElementById('search-area').hidden &&
    !document.hidden;
  const button = document.getElementById('view-safety-camera');
  const preview = document.getElementById('view-safety-preview');
  cameraPending = true;
  button.textContent = strings['view.safety.camera.stop'];
  showSafetyStatus('view.safety.camera.starting');
  controller.cameraDone = startSafetyCamera(found, preview, current);
});
document.getElementById('view-safety-select').addEventListener('click', () => {
  document.getElementById('view-safety-image').click();
});
document.getElementById('view-safety-image').addEventListener('change', (event) => {
  const input = event.currentTarget;
  const file = input.files?.[0];
  input.value = '';
  if (file) controller.scanQrDone = controller.scanQrFile(file);
});
// The local block takes effect even if token revocation cannot reach a node.
const blockViewed = async (call = invoke) => {
  const { found, card } = viewed;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  let result;
  try {
    result = await call('block_peer', { peerId: found.peerId });
  } catch (error) {
    if (currentView()) searchMessage('search.failed', String(error));
    return;
  }
  if (!currentView()) return;
  const wasLiked = Boolean(card.closest('#chats-liked-list'));
  const savedRow = card.closest('#saved-profile-rows li');
  (savedRow || card).remove();
  if (wasLiked)
    document.getElementById('chats-liked-count').textContent = String(
      document.getElementById('chats-liked-list').children.length,
    );
  closeFound();
  if (savedRow) savedProfilesController.refreshSaved();
  return searchMessage(`search.blocked.${result.revocation}`);
};
document.getElementById('view-block').addEventListener('click', () => {
  controller.blockDone = blockViewed();
});
const searchError = document.getElementById('search-error');
const showSearchError = (key, details) => {
  document.getElementById('search-error-text').textContent = strings[key];
  document.getElementById('search-failures').textContent = details;
  searchError.hidden = false;
};
copyFrom(
  document.getElementById('copy-search-details'),
  document.getElementById('search-failures'),
);
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
const renderSearchResults = (result, found, call) => {
  showSearchFilters(result.offered);
  searchResults.replaceChildren(
    ...found.map((item, index) => resultCard(item, `result-${index + 1}`)),
  );
  peerPhotosController.showCardPhotos(found, [...searchResults.children], call);
  const count = String(found.length);
  if (result.partial || result.failures.length) {
    searchMessage('search.partial', count);
    showSearchError('search.partialError', result.failures.map(failureText).join('\n'));
  } else if (!found.length) searchMessage('search.empty');
  else searchMessage('search.results', count);
  document.getElementById('search-empty').hidden = found.length > 0;
  searchAgain.hidden = !found.length;
};
const runSearch = async (call = invoke) => {
  profileFieldsModule.checkRange('search-age-min', 'search-age-max');
  profileFieldsModule.checkRange('search-height-min', 'search-height-max');
  if (searchBusy || !searchForm.reportValidity()) return;
  const generation = identityState.generation;
  const query = searchQuery();
  closeFound();
  clearSearch();
  setSearchBusy(true);
  searchMessage('search.loading');
  try {
    const result = await call('search_profiles', { query });
    if (!identityCurrent(generation)) return;
    const found = await withPlaces(result.profiles);
    if (!identityCurrent(generation)) return;
    addKnownPeers(result.profiles);
    await loadPeerNames(
      found.map((item) => item.peerId),
      call,
    );
    if (!identityCurrent(generation)) return;
    renderSearchResults(result, found, call);
  } catch (error) {
    if (!identityCurrent(generation)) return;
    const code = String(error);
    if (code === 'notConnected') searchMessage('search.notConnected');
    else searchMessage('search.failed', '');
    showSearchError(
      {
        notConnected: 'search.offline',
        mutualUnavailable: 'search.mutualUnavailable',
        invalidInterests: 'search.interestInvalid',
      }[code] || 'search.error',
      code,
    );
  } finally {
    if (identityCurrent(generation)) setSearchBusy(false);
  }
};
controller.resetSearch = () => {
  setSearchBusy(false);
  searchForm.reset();
  searchPlace.set(null);
  showSearchFilters([]);
  closeFound();
  clearSearch();
  searchMessage('search.idle');
};
searchForm.addEventListener('submit', (event) => {
  event.preventDefault();
  controller.searchDone = runSearch();
});
for (const id of ['profile-open-network', 'search-open-network', 'chats-open-network']) {
  document.getElementById(id).addEventListener('click', () => showArea('network-area'));
}
for (const button of [searchRetry, searchAgain]) {
  button.addEventListener('click', () => {
    controller.searchDone = runSearch();
  });
}

export {
  blockViewed,
  cameraPending,
  clearSearch,
  closeFound,
  confirmContact,
  copySafety,
  drawContactQr,
  loadSafety,
  openFound,
  profileView,
  removeMark,
  resultCard,
  runSearch,
  searchBusy,
  searchMain,
  searchQuery,
  searchResults,
  sendLike,
  sendPass,
  showSearchFilters,
  viewStatus,
  viewed,
  withPlaces,
};
