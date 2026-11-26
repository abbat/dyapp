import { byId as domById } from './dom.js';
import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { countryName, scaleFields } from './profile-fields.js';

/** @typedef {import('./ipc.js').Invoke} Invoke
 * @typedef {import('./ipc.js').LinkReviewField} Field
 * @typedef {import('./ipc.js').LinkReviewSource} Source
 * @typedef {import('./ipc.js').LinkReviewProfile} Profile
 * @typedef {import('./ipc.js').LinkReviewSnapshot} Snapshot
 * @typedef {import('./ipc.js').LinkReviewPixels} Pixels
 * @typedef {{ call: Invoke, generation: number, snapshot: Snapshot | null,
 *   choices: Map<Field, Source>, preview: Profile | null, saved: boolean,
 *   completed: boolean, expired: boolean, busy: boolean,
 *   photos: Map<string, Pixels | 'failed'>, photoPending: Set<string> }} Opening
 */
const dialog = domById('link-review-dialog', HTMLDialogElement);
const fields = domById('link-review-fields', HTMLDivElement);
const previewRows = domById('link-review-preview-rows', HTMLDivElement);
const status = domById('link-review-status', HTMLParagraphElement);
const previewButton = domById('link-review-preview', HTMLButtonElement);
const editButton = domById('link-review-edit', HTMLButtonElement);
const saveButton = domById('link-review-save', HTMLButtonElement);
const refreshButton = domById('link-review-refresh', HTMLButtonElement);
const resumeButton = domById('link-review-resume', HTMLButtonElement);

/** @type {Opening | null} */
let current = null;
/** @type {Element | null} */
let openedFrom = null;
/** @param {string} key */
const text = (key) => strings[`linkReview.${key}`];
/** @param {Opening | null} opening @returns {opening is Opening} */
const active = (opening) =>
  Boolean(opening && current === opening && dialog.open && identityCurrent(opening.generation));
/** @type {Record<Field, string>} */
const fieldLabels = {
  name: strings['linkReview.field.name'],
  location: strings['linkReview.field.location'],
  income: strings['linkReview.field.income'],
  hasKids: strings['linkReview.field.hasKids'],
  wantsMoreKids: strings['linkReview.field.wantsMoreKids'],
  interests: strings['linkReview.field.interests'],
  photos: strings['linkReview.field.photos'],
  education: strings['linkReview.field.education'],
  profession: strings['linkReview.field.profession'],
  company: strings['linkReview.field.company'],
  industry: strings['linkReview.field.industry'],
  careerStage: strings['linkReview.field.careerStage'],
  workLifeBalance: strings['linkReview.field.workLifeBalance'],
  birth: strings['linkReview.field.birth'],
  gender: strings['linkReview.field.gender'],
  orientation: strings['linkReview.field.orientation'],
  relationshipGoal: strings['linkReview.field.relationshipGoal'],
  rolePreference: strings['linkReview.field.rolePreference'],
  careerAmbition: strings['linkReview.field.careerAmbition'],
  financialPhilosophy: strings['linkReview.field.financialPhilosophy'],
  employmentStatus: strings['linkReview.field.employmentStatus'],
  fitnessLevel: strings['linkReview.field.fitnessLevel'],
  zodiac: strings['linkReview.field.zodiac'],
  height: strings['linkReview.field.height'],
  about: strings['linkReview.field.about'],
  smoking: strings['linkReview.field.smoking'],
  drinking: strings['linkReview.field.drinking'],
  tattoos: strings['linkReview.field.tattoos'],
  maritalStatus: strings['linkReview.field.maritalStatus'],
  pets: strings['linkReview.field.pets'],
  seeking: strings['linkReview.field.seeking'],
};
const unset = () => text('unpublished');
/** @param {string | number | null | undefined} value */
const optional = (value) =>
  value === null || value === undefined || value === '' ? unset() : String(value);
/** @param {boolean | null} value */
const bool = (value) => (value === null ? unset() : text(value ? 'yes' : 'no'));
/** @param {number[]} bytes */
const hex = (bytes) => bytes.map((byte) => byte.toString(16).padStart(2, '0')).join('');
/** @param {string} label @param {string | number | null | undefined} value */
const row = (label, value) => `${label}: ${optional(value)}`;

/** @param {string} key @param {number} code */
const scale = (key, code) => {
  if (code === 0) return unset();
  const scale = scaleFields.find((candidate) => candidate.key === key);
  const choice = scale?.choices.find((candidate) => candidate.code === code);
  return choice
    ? strings[`profile.value.${key}.${choice.key}`]
    : text('unknownValue').replace('{value}', String(code));
};
/** @param {Field} field @param {Profile} profile */
const textValue = (field, profile) => {
  /** @type {Partial<Record<Field, string>>} */
  const values = {
    name: profile.fields.name,
    interests: profile.interests.join('; '),
    profession: profile.profession,
    company: profile.company,
    industry: profile.industry,
    careerStage: profile.careerStage,
    workLifeBalance: profile.workLifeBalance,
    about: profile.fields.about,
    height: optional(profile.fields.height),
  };
  return values[field];
};
/** @param {Field} field @param {Profile} profile */
const scalarValue = (field, profile) => {
  /** @type {Partial<Record<Field, [string, number]>>} */
  const values = {
    education: ['education_level', profile.educationLevel],
    gender: ['gender', profile.fields.gender],
    orientation: ['orientation', profile.fields.orientation],
    relationshipGoal: ['relationship_goal', profile.fields.relationshipGoal],
    rolePreference: ['role_preference', profile.fields.rolePreference],
    careerAmbition: ['career_ambition', profile.fields.careerAmbition],
    financialPhilosophy: ['financial_philosophy', profile.fields.financialPhilosophy],
    employmentStatus: ['employment_status', profile.fields.employmentStatus],
    fitnessLevel: ['fitness_level', profile.fields.fitnessLevel],
    zodiac: ['zodiac', profile.fields.zodiac],
    smoking: ['smoking', profile.fields.smoking],
    drinking: ['drinking', profile.fields.drinking],
    tattoos: ['tattoos', profile.fields.tattoos],
    maritalStatus: ['marital_status', profile.fields.maritalStatus],
    pets: ['pets', profile.fields.pets],
  };
  const selected = values[field];
  return selected ? scale(...selected) : null;
};
/** @param {Field} field @param {Profile} profile */
const groupValue = (field, profile) => {
  const draft = profile.fields;
  switch (field) {
    case 'location':
      return [
        row(
          strings['profile.country'],
          draft.country ? `${countryName(draft.country)} (${draft.country})` : unset(),
        ),
        row(strings['profile.place'], draft.place),
      ].join('\n');
    case 'income':
      return [
        row(text('incomeFrom'), profile.incomeFrom),
        row(text('incomeTo'), profile.incomeTo),
      ].join('\n');
    case 'hasKids':
      return bool(profile.hasKids);
    case 'wantsMoreKids':
      return bool(profile.wantsMoreKids);
    case 'birth':
      return [
        row(strings['profile.birthYear'], draft.birthYear),
        row(strings['profile.birthMonth'], draft.birthMonth),
        row(strings['profile.birthDay'], draft.birthDay),
      ].join('\n');
    case 'seeking':
      return [
        row(strings['profile.notLooking'], bool(draft.notLooking)),
        row(
          strings['profile.seekingGenders'],
          draft.seekingGenders.map((code) => scale('gender', code)).join('; '),
        ),
        row(strings['profile.seekingAgeFrom'], draft.seekingAgeFrom),
        row(strings['profile.seekingAgeTo'], draft.seekingAgeTo),
      ].join('\n');
    case 'photos':
      return text('photoCount').replace('{count}', String(profile.photos.length));
    default:
      throw new Error(`Missing comparison field: ${field}`);
  }
};
/** @param {Field} field @param {Profile} profile */
const value = (field, profile) => {
  const direct = textValue(field, profile);
  if (direct !== undefined) return optional(direct);
  const scalar = scalarValue(field, profile);
  return scalar === null ? groupValue(field, profile) : scalar;
};

/** @param {Opening} opening */
const complete = (opening) =>
  opening.snapshot?.differences.every((field) => opening.choices.has(field)) ?? false;
/** @param {Opening} opening */
const selected = (opening) =>
  opening.snapshot
    ? opening.snapshot.differences.map((field) => ({
        field,
        source: /** @type {Source} */ (opening.choices.get(field)),
      }))
    : [];
/** @param {Opening} opening @param {string} [key] @param {boolean} [error] */
const message = (opening, key = '', error = false) => {
  if (!active(opening)) return;
  status.textContent = key ? strings[key] : '';
  status.hidden = !key;
  status.classList.toggle('error', error);
};
/** @param {unknown} error */
const failureKey = (error) =>
  ({
    reviewRequired: 'linkReview.reviewRequired',
    privateStateReviewRequired: 'linkReview.privateState',
    enrollmentAuthorizationRequired: 'linkReview.enrollment',
    revoked: 'linkReview.revoked',
    deviceNotListed: 'linkReview.deviceNotListed',
    invalidSelection: 'linkReview.invalidSelection',
    invalidProfile: 'linkReview.invalidProfile',
    notConnected: 'linkReview.offline',
  })[String(error)] || 'linkReview.failed';

/** @param {HTMLCanvasElement} canvas @param {Pixels} photo */
/** @param {unknown} error */
const completionFailure = (error) =>
  [
    'reviewRequired',
    'privateStateReviewRequired',
    'enrollmentAuthorizationRequired',
    'revoked',
    'deviceNotListed',
    'invalidSelection',
    'invalidProfile',
  ].includes(String(error))
    ? failureKey(error)
    : 'linkReview.uncertain';

/** @param {HTMLCanvasElement} canvas @param {Pixels} photo */
const drawPhoto = (canvas, photo) => {
  const length = photo.width * photo.height * 4;
  if (!Number.isSafeInteger(length) || length !== photo.pixels.length)
    throw new Error('Invalid comparison thumbnail');
  canvas.width = photo.width;
  canvas.height = photo.height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Photo canvas unavailable');
  context.putImageData(
    new ImageData(new Uint8ClampedArray(photo.pixels), photo.width, photo.height),
    0,
    0,
  );
};
/** @param {Source} source @param {number} index */
const photoNodes = (source, index) => ({
  canvas: domById(`link-review-photo-${source}-${index}`, HTMLCanvasElement),
  status: domById(`link-review-photo-status-${source}-${index}`, HTMLParagraphElement),
  retry: domById(`link-review-photo-retry-${source}-${index}`, HTMLButtonElement),
});
/** @param {Opening} opening @param {Source} source @param {number} index */
const showPhoto = (opening, source, index) => {
  if (!active(opening)) return;
  const key = `${source}:${index}`;
  const cached = opening.photos.get(key);
  const { canvas, status: photoStatus, retry } = photoNodes(source, index);
  retry.hidden = cached !== 'failed';
  photoStatus.hidden = Boolean(cached && cached !== 'failed');
  if (cached && cached !== 'failed') {
    drawPhoto(canvas, cached);
  } else {
    photoStatus.textContent = text(cached === 'failed' ? 'photoUnavailable' : 'photoLoading');
  }
};
/** @param {Opening} opening @param {Source} source @param {number} index */
const loadPhoto = async (opening, source, index) => {
  const key = `${source}:${index}`;
  if (
    !opening.snapshot ||
    opening.photos.has(key) ||
    opening.photoPending.has(key) ||
    !active(opening)
  )
    return;
  opening.photoPending.add(key);
  showPhoto(opening, source, index);
  try {
    const photo = await opening.call('link_review_photo', {
      reviewId: opening.snapshot.reviewId,
      source,
      index,
    });
    opening.photos.set(key, photo);
  } catch (_) {
    if (active(opening)) opening.photos.set(key, 'failed');
  } finally {
    opening.photoPending.delete(key);
    showPhoto(opening, source, index);
  }
};
/** @param {Opening} opening @param {Profile} profile @param {Source} source */
const photoReferences = (opening, profile, source) => {
  const fragment = document.createDocumentFragment();
  for (const [index, photo] of profile.photos.entries()) {
    const title = document.createElement('p');
    title.className = 'caption';
    title.textContent = text('photoNumber').replace('{number}', String(index + 1));
    const canvas = document.createElement('canvas');
    canvas.id = `link-review-photo-${source}-${index}`;
    canvas.className = 'link-review-photo';
    canvas.setAttribute('role', 'img');
    canvas.setAttribute('aria-label', title.textContent);
    const photoStatus = document.createElement('p');
    photoStatus.id = `link-review-photo-status-${source}-${index}`;
    photoStatus.className = 'caption';
    const retry = document.createElement('button');
    retry.id = `link-review-photo-retry-${source}-${index}`;
    retry.type = 'button';
    retry.className = 'btn quiet';
    retry.textContent = text('photoRetry');
    retry.hidden = true;
    retry.addEventListener('click', () => {
      if (!active(opening)) return;
      opening.photos.delete(`${source}:${index}`);
      controller.photoDone = loadPhoto(opening, source, index);
    });
    const references = document.createElement('details');
    const summary = document.createElement('summary');
    summary.textContent = text('photoReferences');
    const thumbnail = document.createElement('code');
    thumbnail.className = 'full-id';
    thumbnail.textContent = `${text('thumbnailHash')}: ${hex(photo.thumbnail)}`;
    const chunks = photo.full.map((hash, chunk) => {
      const code = document.createElement('code');
      code.className = 'full-id';
      code.textContent = `${text('chunkHash').replace('{number}', String(chunk + 1))}: ${hex(hash)}`;
      return code;
    });
    references.append(summary, thumbnail, ...chunks);
    fragment.append(title, canvas, photoStatus, retry, references);
  }
  return fragment;
};
/** @param {Opening} opening @param {Field} field */
const renderField = (opening, field) => {
  const snapshot = opening.snapshot;
  if (!snapshot) throw new Error('Comparison is not ready');
  const group = document.createElement('fieldset');
  group.className = 'card link-review-field';
  group.dataset.field = field;
  const legend = document.createElement('legend');
  legend.textContent = fieldLabels[field];
  group.append(legend);
  for (const source of /** @type {Source[]} */ (['local', 'remote'])) {
    const profile = snapshot[source];
    const label = document.createElement('label');
    label.className = 'choice';
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = `link-review-${snapshot.reviewId}-${field}`;
    input.value = source;
    input.id = `link-review-${field}-${source}`;
    input.checked = opening.choices.get(field) === source;
    input.addEventListener('change', () => {
      if (!active(opening) || opening.busy || opening.expired || opening.saved) return;
      opening.choices.set(field, source);
      const hadPreview = opening.preview;
      opening.preview = null;
      if (hadPreview) {
        render(opening);
        domById(input.id, HTMLInputElement).focus();
      } else {
        sync(opening);
      }
    });
    const sourceText = document.createElement('span');
    sourceText.textContent = text(source);
    label.append(input, sourceText);
    const content = document.createElement('p');
    content.id = `link-review-${field}-${source}-value`;
    content.className = 'link-review-value';
    content.textContent = value(field, profile);
    group.append(label, content);
    if (field === 'photos') group.append(photoReferences(opening, profile, source));
  }
  return group;
};
/** @param {Opening} opening */
const sync = (opening) => {
  if (!active(opening)) return;
  const disabled = opening.busy || opening.expired || !opening.snapshot || opening.saved;
  for (const group of fields.querySelectorAll('fieldset')) group.disabled = disabled;
  previewButton.disabled = disabled || !complete(opening);
  saveButton.disabled = disabled || !complete(opening);
  refreshButton.disabled = opening.busy;
  resumeButton.disabled = opening.busy || opening.completed || opening.expired;
  previewButton.hidden = Boolean(opening.preview);
  editButton.hidden = !opening.preview;
  domById('link-review-preview-title', HTMLHeadingElement).hidden = !opening.preview;
  previewRows.hidden = !opening.preview;
  resumeButton.hidden = !opening.saved;
  saveButton.hidden = Boolean(opening.saved);
};
/** @param {Opening} opening */
const render = (opening) => {
  if (!active(opening)) return;
  fields.replaceChildren(
    ...(opening.snapshot?.differences.map((field) => renderField(opening, field)) ?? []),
  );
  const preview = opening.preview;
  const snapshot = opening.snapshot;
  previewRows.replaceChildren(
    ...(preview && snapshot
      ? snapshot.differences.map((field) => {
          const content = document.createElement('p');
          content.className = 'link-review-value';
          content.dataset.field = field;
          content.textContent = `${fieldLabels[field]}\n${value(field, preview)}`;
          return content;
        })
      : []),
  );
  sync(opening);
  if (opening.snapshot?.differences.includes('photos')) {
    for (const source of /** @type {Source[]} */ (['local', 'remote'])) {
      for (const index of opening.snapshot[source].photos.keys()) {
        showPhoto(opening, source, index);
        controller.photoDone = loadPhoto(opening, source, index);
      }
    }
  }
};
const retire = () => {
  const opening = current;
  const opener = openedFrom;
  current = null;
  openedFrom = null;
  if (opening?.snapshot)
    opening.call('dismiss_link_review', { reviewId: opening.snapshot.reviewId }).catch(() => {});
  if (dialog.open) dialog.close();
  dialog.hidden = true;
  fields.replaceChildren();
  previewRows.replaceChildren();
  if (opener instanceof HTMLElement && opener.isConnected && !opener.matches(':disabled'))
    opener.focus();
};
/** @param {Opening} opening */
const prepare = async (opening) => {
  opening.busy = true;
  message(opening, 'linkReview.loading');
  sync(opening);
  try {
    const snapshot = await opening.call('prepare_link_review');
    if (!active(opening)) return;
    opening.snapshot = snapshot;
    message(opening, snapshot.differences.length ? '' : 'linkReview.equal');
    render(opening);
  } catch (error) {
    if (active(opening)) message(opening, failureKey(error), true);
  } finally {
    if (active(opening)) {
      opening.busy = false;
      sync(opening);
    }
  }
};
/** @param {Invoke} [call] @returns {Promise<void>} */
const open = (call = invoke) => {
  const opener = current ? openedFrom : document.activeElement;
  retire();
  openedFrom = opener;
  /** @type {Opening} */
  const opening = {
    call,
    generation: identityState.generation,
    snapshot: null,
    choices: new Map(),
    preview: null,
    saved: false,
    completed: false,
    expired: false,
    busy: false,
    photos: new Map(),
    photoPending: new Set(),
  };
  current = opening;
  dialog.hidden = false;
  if (!dialog.open) dialog.showModal();
  refreshButton.focus();
  const done = prepare(opening);
  controller.prepareDone = done;
  return done;
};
const preview = async () => {
  const opening = current;
  if (!active(opening) || opening.busy || opening.saved || !opening.snapshot || !complete(opening))
    return;
  opening.busy = true;
  sync(opening);
  try {
    const result = await opening.call('preview_link_review', {
      reviewId: opening.snapshot.reviewId,
      selected: selected(opening),
    });
    if (active(opening)) {
      opening.preview = result;
      message(opening);
      render(opening);
    }
  } catch (error) {
    if (active(opening)) message(opening, failureKey(error), true);
  } finally {
    if (active(opening)) {
      opening.busy = false;
      sync(opening);
    }
  }
};
const confirm = async () => {
  const opening = current;
  if (
    !active(opening) ||
    opening.busy ||
    opening.expired ||
    opening.saved ||
    !opening.snapshot ||
    !complete(opening)
  )
    return;
  opening.busy = true;
  message(opening, 'linkReview.saving');
  sync(opening);
  try {
    await opening.call('confirm_link_review', {
      reviewId: opening.snapshot.reviewId,
      selected: selected(opening),
    });
    if (active(opening)) {
      opening.saved = true;
      message(opening, 'linkReview.saved');
      render(opening);
      resumeButton.focus();
    }
  } catch (error) {
    if (active(opening)) {
      opening.expired = true;
      message(opening, completionFailure(error), true);
      render(opening);
      refreshButton.focus();
    }
  } finally {
    if (active(opening)) {
      opening.busy = false;
      sync(opening);
    }
  }
};
const resume = async () => {
  const opening = current;
  if (
    !active(opening) ||
    opening.busy ||
    opening.expired ||
    !opening.saved ||
    !opening.snapshot ||
    opening.completed
  )
    return;
  opening.busy = true;
  message(opening, 'linkReview.resuming');
  sync(opening);
  try {
    await opening.call('resume_link_review', { reviewId: opening.snapshot.reviewId });
    if (active(opening)) {
      opening.completed = true;
      message(opening, 'linkReview.resumed');
      document.dispatchEvent(new Event('dyapp:link-review-resumed'));
    }
  } catch (error) {
    if (active(opening)) {
      opening.expired = true;
      message(opening, completionFailure(error), true);
      refreshButton.focus();
    }
  } finally {
    if (active(opening)) {
      opening.busy = false;
      sync(opening);
    }
  }
};

previewButton.addEventListener('click', () => {
  controller.previewDone = preview();
});
editButton.addEventListener('click', () => {
  if (!current || current.busy) return;
  current.preview = null;
  render(current);
});
saveButton.addEventListener('click', () => {
  controller.confirmDone = confirm();
});
refreshButton.addEventListener('click', () => open(current?.call ?? invoke));
resumeButton.addEventListener('click', () => {
  controller.resumeDone = resume();
});
domById('link-review-close', HTMLButtonElement).addEventListener('click', retire);
dialog.addEventListener('cancel', (event) => {
  event.preventDefault();
  retire();
});
dialog.addEventListener('close', () => {
  if (!dialog.open && current) retire();
});
document.addEventListener('dyapp:identity-change', retire);
const controller = {
  open,
  close: retire,
  prepareDone: /** @type {Promise<void> | undefined} */ (undefined),
  previewDone: /** @type {Promise<void> | undefined} */ (undefined),
  confirmDone: /** @type {Promise<void> | undefined} */ (undefined),
  resumeDone: /** @type {Promise<void> | undefined} */ (undefined),
  photoDone: /** @type {Promise<void> | undefined} */ (undefined),
};
export { controller };
