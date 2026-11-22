// Static presentation only: native Rust owns comparison, validation, merging and all writes.
const openings = new WeakMap();
const sources = ['local', 'remote'];
const element = (id) => {
  const node = document.getElementById(id);
  if (!(node instanceof HTMLElement)) throw new Error(`Comparison element missing: ${id}`);
  return node;
};
const button = (id) => {
  const node = element(id);
  if (!(node instanceof HTMLButtonElement)) throw new Error(`Comparison button missing: ${id}`);
  return node;
};

const valueNode = (strings, field, source, prefix) => {
  const box = document.createElement('div');
  box.className = 'review-value';
  const text = document.createElement('p');
  text.id = `${prefix}-${source}-value`;
  text.textContent = strings[`review.sample.${field}.${source}`];
  box.append(text);
  if (field === 'photos') photoValues(box, strings, source, prefix);
  return box;
};

const photoValues = (box, strings, source, prefix) => {
  const photos = document.createElement('div');
  photos.className = 'review-photos';
  const details = document.createElement('details');
  const summary = document.createElement('summary');
  summary.textContent = strings['review.photoDetails'];
  details.append(summary);
  const hashes = source === 'local' ? ['aa', 'bb'] : ['cc'];
  for (const [index, hash] of hashes.entries()) {
    const photo = document.createElement('div');
    photo.className = `photo review-photo ${source === 'local' ? 'p2' : 'p4'}`;
    photo.setAttribute('role', 'img');
    photo.setAttribute(
      'aria-label',
      `${strings[source === 'local' ? 'review.photoLocal' : 'review.photoRemote']} ${index + 1}`,
    );
    photos.append(photo);
    const reference = document.createElement('code');
    reference.id = `${prefix}-${source}-${index}-thumbnail`;
    reference.textContent = hash.repeat(32);
    details.append(reference);
    for (const [part, chunk] of [hash, 'dd'].entries()) {
      const chunks = document.createElement('code');
      chunks.id = `${prefix}-${source}-${index}-original-${part}`;
      chunks.textContent = chunk.repeat(32);
      details.append(chunks);
    }
  }
  box.append(photos, details);
};

const choiceCard = (strings, field, choose, radios) => {
  const card = document.createElement('fieldset');
  card.className = 'card review-card';
  card.dataset.reviewField = field;
  const title = document.createElement('legend');
  title.textContent = strings[`review.field.${field}`];
  card.append(title);
  for (const source of sources) {
    const label = document.createElement('label');
    label.className = 'choice';
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = `review-${field}`;
    input.id = `review-${field}-${source}`;
    input.value = source;
    const content = document.createElement('div');
    content.className = 'review-option';
    const heading = document.createElement('b');
    heading.textContent = strings[`review.${source}`];
    input.setAttribute('aria-describedby', `review-${field}-${source}-value`);
    label.append(input, heading);
    content.append(label, valueNode(strings, field, source, `review-${field}`));
    card.append(content);
    radios.set(`${field}:${source}`, input);
    input.onchange = () => choose(field, source);
  }
  return card;
};

const previewCard = (strings, field, values) => {
  const card = document.createElement('div');
  card.className = 'card';
  card.dataset.reviewPreviewField = field;
  const title = document.createElement('h2');
  title.textContent = strings[`review.field.${field}`];
  card.append(title);
  for (const source of sources) {
    const value = valueNode(strings, field, source, `review-preview-${field}`);
    value.dataset.reviewSource = source;
    value.hidden = true;
    values.set(`${field}:${source}`, value);
    card.append(value);
  }
  return card;
};

/** Seeded states represent earlier explicit choices; the initial comparison has no defaults. */
export const installReviewFixture = (
  strings,
  screen,
  navigate = (target) => {
    location.hash = target;
  },
) => {
  const root = element('review-fields');
  const preview = element('review-preview-values');
  const confirm = button('review-confirm');
  const show = button('review-preview');
  const edit = button('review-edit');
  const back = button('review-back');
  openings.get(root)?.dispose();
  const lifetime = new AbortController();
  const offered = Object.keys(strings)
    .filter((key) => key.startsWith('review.field.'))
    .map((key) => key.slice('review.field.'.length));
  if (!offered.length) throw new Error('Comparison fixture catalogue is missing');
  const fields = screen === 'link-review-equal' ? [] : offered;
  const selected = new Map();
  const radios = new Map();
  const values = new Map();
  let saving = screen === 'link-review-saving';
  let previewing = screen === 'link-review-preview';
  const editable = [
    'link-review-comparison',
    'link-review-ready',
    'link-review-preview',
    'link-review-validation',
    'link-review-equal',
  ].includes(screen);
  const current = () => openings.get(root) === owner && !lifetime.signal.aborted;
  const complete = () => fields.every((field) => selected.has(field));
  const render = () => {
    for (const [key, radio] of radios) {
      const [field, source] = key.split(':');
      radio.checked = selected.get(field) === source;
      radio.disabled = !editable || saving;
    }
    for (const [key, value] of values) {
      const [field, source] = key.split(':');
      value.hidden = selected.get(field) !== source;
    }
    root.hidden = (!editable && !saving) || previewing;
    document.getElementById('review-preview-panel').hidden = !previewing;
    show.hidden = !editable || previewing;
    edit.hidden = !previewing;
    show.disabled = confirm.disabled = !editable || saving || !complete();
    back.disabled = saving;
  };
  const choose = (field, source) => {
    if (!current() || !editable || saving || !fields.includes(field) || !sources.includes(source))
      return;
    selected.set(field, source);
    render();
  };
  const save = () => {
    if (!current() || !editable || saving || !complete()) return;
    saving = true;
    render();
    navigate('link-review-saving');
  };
  const owner = {
    choose,
    save,
    fields,
    selection: () => new Map(selected),
    dispose: () => {
      lifetime.abort();
      for (const radio of radios.values()) radio.onchange = null;
    },
  };
  openings.set(root, owner);
  root.replaceChildren(...fields.map((field) => choiceCard(strings, field, choose, radios)));
  preview.replaceChildren(...fields.map((field) => previewCard(strings, field, values)));
  if (!['link-review-comparison', 'link-review-loading'].includes(screen)) {
    for (const field of fields) selected.set(field, field === 'photos' ? 'remote' : 'local');
  }
  const listen = (node, action) =>
    node.addEventListener('click', action, { signal: lifetime.signal });
  listen(confirm, save);
  listen(show, () => {
    if (!current() || saving || !editable || !complete()) return;
    previewing = true;
    render();
    edit.focus();
  });
  listen(edit, () => {
    if (!current() || saving) return;
    previewing = false;
    render();
    show.focus();
  });
  render();
  return owner;
};

export const currentReviewFixture = () => openings.get(document.getElementById('review-fields'));
