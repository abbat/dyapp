import { all as domAll, byId as domById, eventDetail } from './dom.js';
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
import { drawContactQr, grayscaleFrame, safetyCameraFrame } from './contact-qr.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.favoriteShown = undefined;
/** @type {Promise<void> | undefined} */
controller.peerPhotosShown = undefined;
/** @type {Promise<void> | undefined} */
controller.likeDone = undefined;
/** @type {Promise<void> | undefined} */
controller.passDone = undefined;
/** @type {Promise<boolean> | undefined} */
controller.safetyClearDone = undefined;
/** @type {Promise<void> | undefined} */
controller.safetyCopied = undefined;
/** @type {Promise<void> | undefined} */
controller.safetyDone = undefined;
/** @type {Promise<void> | undefined} */
controller.confirmDone = undefined;
/** @type {Promise<void> | undefined} */
controller.cameraDone = undefined;
/** @type {Promise<void> | undefined} */
controller.scanQrDone = undefined;
/** @type {Promise<void> | undefined} */
controller.blockDone = undefined;
/** @type {Promise<void> | undefined} */
controller.searchDone = undefined;
/** @type {{ stream: MediaStream | null }} */
export const cameraState = { stream: null };

// The Search area imports its identity, navigation, place and status helpers.
// A verified sample from the active network's search nodes. The query stays in the form, and
// filters appear only for fields every reached node offers and the client re-checks.
const searchForm = domById('search', HTMLFormElement);
const searchRetry = domById('search-retry', HTMLButtonElement);
const searchAgain = domById('search-again', HTMLButtonElement);
const searchResults = domById('search-results', HTMLDivElement);
const COUNTRY_FIELD = 4;
const PLACE_FIELD = 20;
const searchCountry = domById('search-country', HTMLInputElement);
const searchCountryCode = () => searchCountry.value.trim().toUpperCase();
const searchPlace = placePicker(
  searchCountry,
  domById('search-place', HTMLInputElement),
  domById('search-places', HTMLDataListElement),
  searchCountryCode,
);
const BIRTH_AGE_FIELD = 1001;
const HEIGHT_FIELD = 46;
const SEEKING_GENDERS_FIELD = 53;
const SEEKING_AGE_FIELD = 54;
let searchBusy = false;
/** @param {string} key */
const searchMessage = (key, detail = '') =>
  statusMessage(domById('search-status', HTMLParagraphElement), 'search-open-network', key, detail);
// Hidden filters are disabled, so they neither block validation nor reach the query.
/** @param {HTMLDivElement} element @param {boolean} shown */
const showFilter = (element, shown) => {
  element.hidden = !shown;
  for (const input of domAll(element, 'input, select', HTMLInputElement, HTMLSelectElement))
    input.disabled = !shown || searchBusy;
};
/** @param {number[]} offered */
const showSearchFilters = (offered) => {
  const scales = domById('search-scales', HTMLDivElement);
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
  showFilter(domById('search-age', HTMLDivElement), offered.includes(BIRTH_AGE_FIELD));
  showFilter(domById('search-height', HTMLDivElement), offered.includes(HEIGHT_FIELD));
  showFilter(domById('search-interest-row', HTMLDivElement), offered.includes(19));
  showFilter(domById('search-country-row', HTMLDivElement), offered.includes(COUNTRY_FIELD));
  showFilter(
    domById('search-place-row', HTMLDivElement),
    offered.includes(COUNTRY_FIELD) && offered.includes(PLACE_FIELD),
  );
  showFilter(scales, true);
  showFilter(
    domById('search-mutual-row', HTMLDivElement),
    offered.includes(SEEKING_GENDERS_FIELD) && offered.includes(SEEKING_AGE_FIELD),
  );
  domById('search-filters', HTMLDivElement).hidden = !offered.length;
};
/** @returns {import('./ipc.js').EmployerFilter} */
const searchEmployer = () => {
  const company = domById('search-company', HTMLInputElement).value.trim();
  const kind = company ? domById('search-employer', HTMLSelectElement).value : 'any';
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
  /** @param {string} id */
  const filled = (id) => {
    const input = domById(id, HTMLElement);
    if (!(input instanceof HTMLInputElement || input instanceof HTMLSelectElement))
      throw new Error('Expected a search input or select');
    return input.disabled ? '' : input.value.trim();
  };
  /** @type {Array<{ field: number, op: 'eq' | 'range', ints?: number[], strings?: string[] }>} */
  const conditions = [...domAll(document, '#search-scales select:enabled', HTMLSelectElement)]
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
  /** @type {[number, number] | null} */
  const age = min || max ? [Number(min || 18), Number(max || 150)] : null;
  const [from, to] = [filled('search-height-min'), filled('search-height-max')];
  if (from || to)
    conditions.push({
      field: HEIGHT_FIELD,
      op: 'range',
      ints: [Number(from || 50), Number(to || 272)],
    });
  const employer = searchEmployer();
  const mutual = domById('search-mutual', HTMLInputElement);
  return { age, conditions, employer, limit: 0, mutual: mutual.checked && !mutual.disabled };
};
/** @param {import('./ipc.js').SearchFailure} failure */
const failureText = (failure) => {
  const text = strings[`search.failure.${failure.kind}`];
  return failure.kind === 'rateLimited'
    ? `${text} ${Math.ceil(failure.retryAfterMs / 1000)}`
    : text;
};
// Age in years from public birth fields today, a range while the birthday may be ahead.
/** @param {import('./profile-fields.js').ProfileDraft} draft */
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
/** @param {import('./profile-fields.js').ProfileDraft} draft */
const genderText = (draft) => {
  const scale = profileFieldsModule.scaleFields.find((field) => field.key === 'gender');
  const option = scale?.choices.find((choice) => choice.code === draft.gender);
  return option ? strings[`profile.value.gender.${option.key}`] : '';
};
/** @param {FoundWithPlace} found */
const summaryText = (found) =>
  [genderText(found.draft), ageText(found.draft), found.place].filter(Boolean).join(' · ');
// Cards show the letters of the short ID until `showCardPhotos` draws over them; nothing marks a
// self-declared claim as verified.
/**
 * @param {string} id
 * @param {FoundWithPlace} found
 */
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
const searchMain = domById('search-main', HTMLDivElement);
const profileView = domById('profile-view', HTMLDivElement);
/** @param {import('./ipc.js').FoundProfile[]} profiles */
const withPlaces = (profiles) =>
  Promise.all(
    profiles.map(async (profile) => ({
      ...profile,
      place: await placesController.placeText(profile.draft.country, profile.draft.place),
    })),
  );
const viewStatus = domById('view-status', HTMLParagraphElement);
/** @param {import('./ipc.js').Likes} likes */
const showLikes = (likes) => {
  domById('view-like', HTMLButtonElement).hidden = likes.ours || likes.passed;
  domById('view-unlike', HTMLButtonElement).hidden = !likes.ours;
  domById('view-match', HTMLParagraphElement).hidden = !likes.matched;
  domById('view-pass', HTMLButtonElement).hidden = likes.ours || likes.passed;
  domById('view-unpass', HTMLButtonElement).hidden = !likes.passed;
};
/** @typedef {import('./ipc.js').FoundProfile & { place: string }} FoundWithPlace */
/** @typedef {{ found: FoundWithPlace, card: HTMLElement, actionStarted: boolean, actionBusy?: boolean }} ProfileOpening */
/** @type {ProfileOpening | null} */
let viewed = null;
let safetyScanToken = 0;

/** @type {number | null} */
let cameraTimer = null;
let cameraPending = false;
/** @param {string} key */
const showSafetyStatus = (key) => {
  const status = domById('view-safety-scan-status', HTMLParagraphElement);
  status.textContent = strings[key];
  status.hidden = false;
};
const stopSafetyCamera = () => {
  safetyScanToken += 1;
  clearTimeout(cameraTimer ?? undefined);
  cameraTimer = null;
  cameraPending = false;
  cameraState.stream?.getTracks().forEach((track) => {
    track.stop();
  });
  cameraState.stream = null;
  const preview = domById('view-safety-preview', HTMLVideoElement);
  preview.srcObject = null;
  preview.hidden = true;
  const button = domById('view-safety-camera', HTMLButtonElement);
  button.textContent = strings['view.safety.camera.start'];
};
document.addEventListener('dyapp:identity-change', stopSafetyCamera);
document.addEventListener('dyapp:area-change', (event) => {
  if (eventDetail(event) !== 'search-area') stopSafetyCamera();
});
document.addEventListener('visibilitychange', () => {
  if (document.hidden) stopSafetyCamera();
});
/** @param {boolean} verified */
const showVerified = (verified) => {
  domById('view-verified', HTMLParagraphElement).hidden = !verified;
  const state = domById('view-safety-state', HTMLParagraphElement);
  state.dataset.i18n = verified ? 'view.safety.verified' : 'view.safety.unverified';
  state.textContent = strings[state.dataset.i18n];
  domById('view-confirm', HTMLButtonElement).hidden = verified;
  domById('view-unverify', HTMLButtonElement).hidden = !verified;
};
/** @param {FoundWithPlace} found */
const showFound = (found) => {
  const short = shortPeer(found.peerId);
  peerLabel(domById('view-peer', HTMLElement), found.peerId);
  domById('view-peer', HTMLElement).textContent = short;
  domById('view-title', HTMLHeadingElement).textContent = peerNamesController.peerName(
    found.peerId,
  );
  domById('view-summary', HTMLParagraphElement).textContent = summaryText(found);
  domById('view-fields', HTMLDListElement).replaceChildren(
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
/** @param {FoundWithPlace} found @param {HTMLElement} card */
const openFound = (found, card, call = invoke) => {
  const generation = identityState.generation;
  stopSafetyCamera();
  /** @type {ProfileOpening} */
  const target = { found, card, actionStarted: false };
  viewed = target;
  for (const id of ['view-like', 'view-unlike', 'view-pass', 'view-unpass'])
    domById(id, HTMLButtonElement).disabled = false;
  domById('view-message', HTMLButtonElement).disabled = false;
  domById('view-safety-image', HTMLInputElement).value = '';
  domById('view-safety-scan-status', HTMLParagraphElement).hidden = true;
  if (!card.closest('#search-area')) showArea('search-area');
  viewStatus.textContent = '';
  domById('view-safety-panel', HTMLDivElement).hidden = true;
  domById('view-safety-number', HTMLParagraphElement).textContent = '';
  const qr = domById('view-safety-qr', HTMLCanvasElement);
  qr.width = 0;
  qr.height = 0;
  domById('view-safety', HTMLButtonElement).setAttribute('aria-expanded', 'false');
  domById('view-confirm', HTMLButtonElement).disabled = false;
  domById('view-unverify', HTMLButtonElement).disabled = false;
  showVerified(false);
  call('contact_verified', { peerId: found.peerId }).then(
    (verified) => {
      if (identityCurrent(generation) && viewed === target) showVerified(verified === true);
    },
    () => {},
  );
  showLikes({ ours: false, matched: false, passed: false });
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
  domById('view-back', HTMLButtonElement).focus();
};
const closeFound = () => {
  stopSafetyCamera();
  favoritesController.clearFavorite();
  contactNotesController.clearContactNote();
  domById('view-message', HTMLButtonElement).disabled = true;
  profileView.hidden = true;
  searchMain.hidden = false;
  if (viewed?.card.isConnected && !viewed.card.hidden) viewed.card.focus();
  viewed = null;
};
domById('view-back', HTMLButtonElement).addEventListener('click', () => {
  if (!viewed) return;
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
/** @param {boolean} liked */
const sendLike = async (liked, call = invoke) => {
  const target = viewed;
  if (!target || target.actionBusy) return;
  const { found } = target;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed === target;
  target.actionBusy = true;
  target.actionStarted = true;
  const buttons = ['view-like', 'view-unlike', 'view-pass', 'view-unpass'].map((id) =>
    domById(id, HTMLButtonElement),
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
domById('view-like', HTMLButtonElement).addEventListener('click', () => {
  controller.likeDone = sendLike(true);
});
const unlikeDialog = domById('unlike-dialog', HTMLDialogElement);
domById('view-unlike', HTMLButtonElement).addEventListener('click', () => {
  unlikeDialog.hidden = false;
  unlikeDialog.showModal();
  domById('unlike-cancel', HTMLButtonElement).focus();
});
unlikeDialog.addEventListener('close', () => {
  unlikeDialog.hidden = true;
});
domById('unlike-cancel', HTMLButtonElement).addEventListener('click', () => unlikeDialog.close());
domById('unlike-confirm', HTMLButtonElement).addEventListener('click', () => {
  unlikeDialog.close();
  controller.likeDone = sendLike(false);
});
// A pass stays with the own devices: the card leaves its list at once and the open profile can undo it.
/** @param {boolean} passed */
const sendPass = async (passed, call = invoke) => {
  const target = viewed;
  if (!target || target.actionBusy) return;
  const { found, card } = target;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed === target;
  target.actionBusy = true;
  target.actionStarted = true;
  const buttons = ['view-like', 'view-unlike', 'view-pass', 'view-unpass'].map((id) =>
    domById(id, HTMLButtonElement),
  );
  for (const button of buttons) button.disabled = true;
  viewStatus.textContent = strings['view.pass.saving'];
  try {
    const likes = await call('pass_peer', { peerId: found.peerId, passed });
    if (!currentView()) return;
    showLikes(likes);
    card.hidden = passed;
    if (card.closest('#chats-liked-list')) {
      domById('chats-liked-count', HTMLSpanElement).textContent = String(
        domAll(document, '#chats-liked-list [data-peer-id]:not([hidden])', HTMLElement).length,
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
/** @type {Array<[string, boolean]>} */
const passActions = [
  ['view-pass', true],
  ['view-unpass', false],
];
for (const [id, passed] of passActions) {
  domById(id, HTMLButtonElement).addEventListener('click', () => {
    controller.passDone = sendPass(passed);
  });
}
copyFrom(
  domById('copy-view-peer', HTMLButtonElement),
  domById('view-peer', HTMLElement),
  () => viewed?.found.peerId ?? '',
);
const copySafety = (call = invoke) => {
  const button = domById('copy-safety-number', HTMLButtonElement);
  const source = domById('view-safety-number', HTMLParagraphElement);
  const found = viewed?.found;
  if (!found) return Promise.resolve();
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  const token = ++clipboardController.clipboardCopyToken;
  return call('copy_safety_number', { peerId: found.peerId }).then(
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
      if (currentView()) window.getSelection()?.selectAllChildren(source);
    },
  );
};
domById('copy-safety-number', HTMLButtonElement).addEventListener('click', () => {
  controller.safetyCopied = copySafety();
});
const loadSafety = (call = invoke) => {
  if (!viewed) return Promise.resolve();
  const { found } = viewed;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  const button = domById('view-safety', HTMLButtonElement);
  const panel = domById('view-safety-panel', HTMLDivElement);
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
      domById('view-safety-number', HTMLParagraphElement).textContent = (
        number.match(/.{5}/g) ?? []
      ).join(' ');
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
domById('view-safety', HTMLButtonElement).addEventListener('click', () => {
  controller.safetyDone = loadSafety();
});
// Only this path writes the mark; the answer is the signed profile the mark was made from.
const confirmContact = (call = invoke) => {
  if (!viewed) return Promise.resolve();
  const { found, card } = viewed;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  const button = domById('view-confirm', HTMLButtonElement);
  button.disabled = true;
  showSafetyStatus('view.safety.confirm.checking');
  return call('confirm_contact', { peerId: found.peerId }).then(
    async (fresh) => {
      if (!currentView()) return;
      button.disabled = false;
      showVerified(true);
      showSafetyStatus('view.safety.confirm.done');
      domById('view-unverify', HTMLButtonElement).focus();
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
  if (!viewed) return Promise.resolve();
  const { found, card } = viewed;
  const generation = identityState.generation;
  const currentView = () => identityCurrent(generation) && viewed?.found === found;
  const button = domById('view-unverify', HTMLButtonElement);
  button.disabled = true;
  return call('unverify_contact', { peerId: found.peerId }).then(
    async () => {
      if (!currentView()) return;
      button.disabled = false;
      showVerified(false);
      showSafetyStatus('view.safety.confirm.removed');
      domById('view-confirm', HTMLButtonElement).focus();
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
const verifyDialog = domById('verify-dialog', HTMLDialogElement);
domById('view-confirm', HTMLButtonElement).addEventListener('click', () => {
  verifyDialog.hidden = false;
  verifyDialog.showModal();
  domById('verify-cancel', HTMLButtonElement).focus();
});
verifyDialog.addEventListener('close', () => {
  verifyDialog.hidden = true;
});
document.addEventListener('dyapp:identity-change', () => {
  if (verifyDialog.open) verifyDialog.close();
});
domById('verify-cancel', HTMLButtonElement).addEventListener('click', () => verifyDialog.close());
domById('verify-confirm', HTMLButtonElement).addEventListener('click', () => {
  verifyDialog.close();
  controller.confirmDone = confirmContact();
});
domById('view-unverify', HTMLButtonElement).addEventListener('click', () => {
  controller.confirmDone = removeMark();
});
// The UI smoke passes a PNG made from the rendered canvas through this same file path.
/** @param {File} file */
controller.scanQrFile = async (file) => {
  const found = viewed?.found;
  const generation = identityState.generation;
  const panel = domById('view-safety-panel', HTMLDivElement);
  const token = ++safetyScanToken;
  const current = () =>
    identityCurrent(generation) &&
    token === safetyScanToken &&
    viewed?.found === found &&
    !panel.hidden;
  /** @param {string} key */
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
    if (!context) throw new Error('Canvas rendering is unavailable');
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
/**
 * @param {() => boolean} current @param {FoundWithPlace} found
 * @param {{ width: number, height: number, pixels: number[] }} frame
 */
const scanSafetyCameraFrame = async (found, frame, current) => {
  const result = await invoke('scan_contact_qr', { peerId: found.peerId, ...frame });
  if (!current()) return true;
  if (result === 'notFound' || result === 'unreadable') return false;
  stopSafetyCamera();
  showSafetyStatus(`view.safety.scan.${result}`);
  return true;
};
/**
 * @param {HTMLVideoElement} preview @param {() => boolean} current
 * @param {FoundWithPlace} found
 */
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
/**
 * @param {HTMLVideoElement} preview @param {() => boolean} current
 * @param {FoundWithPlace} found
 */
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
    const name = error instanceof Error ? error.name : '';
    const kind = ['NotAllowedError', 'PermissionDeniedError'].includes(name)
      ? 'denied'
      : ['NotFoundError', 'DevicesNotFoundError', 'OverconstrainedError'].includes(name)
        ? 'unavailable'
        : 'failed';
    stopSafetyCamera();
    showSafetyStatus(`view.safety.camera.${kind}`);
  }
};
domById('view-safety-camera', HTMLButtonElement).addEventListener('click', () => {
  if (cameraPending || cameraState.stream) {
    stopSafetyCamera();
    showSafetyStatus('view.safety.camera.stopped');
    return;
  }
  const found = viewed?.found;
  const generation = identityState.generation;
  const panel = domById('view-safety-panel', HTMLDivElement);
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
    !domById('search-area', HTMLElement).hidden &&
    !document.hidden;
  const button = domById('view-safety-camera', HTMLButtonElement);
  const preview = domById('view-safety-preview', HTMLVideoElement);
  cameraPending = true;
  button.textContent = strings['view.safety.camera.stop'];
  showSafetyStatus('view.safety.camera.starting');
  controller.cameraDone = startSafetyCamera(found, preview, current);
});
domById('view-safety-select', HTMLButtonElement).addEventListener('click', () => {
  domById('view-safety-image', HTMLInputElement).click();
});
const safetyImageInput = domById('view-safety-image', HTMLInputElement);
safetyImageInput.addEventListener('change', () => {
  const input = safetyImageInput;
  const file = input.files?.[0];
  input.value = '';
  if (file) controller.scanQrDone = controller.scanQrFile(file);
});
// The local block takes effect even if token revocation cannot reach a node.
const blockViewed = async (call = invoke) => {
  if (!viewed) return;
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
    domById('chats-liked-count', HTMLSpanElement).textContent = String(
      domById('chats-liked-list', HTMLDivElement).children.length,
    );
  closeFound();
  if (savedRow) savedProfilesController.refreshSaved();
  return searchMessage(`search.blocked.${result.revocation}`);
};
domById('view-block', HTMLButtonElement).addEventListener('click', () => {
  controller.blockDone = blockViewed();
});
const searchError = domById('search-error', HTMLDivElement);
/** @param {string} key @param {string} details */
const showSearchError = (key, details) => {
  domById('search-error-text', HTMLParagraphElement).textContent = strings[key];
  domById('search-failures', HTMLPreElement).textContent = details;
  searchError.hidden = false;
};
copyFrom(
  domById('copy-search-details', HTMLButtonElement),
  domById('search-failures', HTMLPreElement),
);
// Loading blocks repeated requests, not navigation.
/** @param {boolean} busy */
const setSearchBusy = (busy) => {
  searchBusy = busy;
  domById('search-loading', HTMLDivElement).hidden = !busy;
  for (const button of searchForm.querySelectorAll('button')) button.disabled = busy;
  for (const input of domAll(searchForm, 'input, select', HTMLInputElement, HTMLSelectElement)) {
    const hidden = input.closest('[hidden]');
    input.disabled = busy || Boolean(hidden && searchForm.contains(hidden));
  }
};
const clearSearch = () => {
  searchError.hidden = true;
  domById('search-empty', HTMLDivElement).hidden = true;
  searchResults.replaceChildren();
  searchAgain.hidden = true;
};
/**
 * @param {import('./ipc.js').SearchResults} result @param {import('./ipc.js').Invoke} call
 * @param {FoundWithPlace[]} found
 */
const renderSearchResults = (result, found, call) => {
  showSearchFilters(result.offered);
  searchResults.replaceChildren(
    ...found.map((item, index) => resultCard(item, `result-${index + 1}`)),
  );
  peerPhotosController.showCardPhotos(
    found,
    [...domAll(searchResults, ':scope > *', HTMLElement)],
    call,
  );
  const count = String(found.length);
  if (result.partial || result.failures.length) {
    searchMessage('search.partial', count);
    showSearchError('search.partialError', result.failures.map(failureText).join('\n'));
  } else if (!found.length) searchMessage('search.empty');
  else searchMessage('search.results', count);
  domById('search-empty', HTMLDivElement).hidden = found.length > 0;
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
  domById(id, HTMLElement).addEventListener('click', () => showArea('network-area'));
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
