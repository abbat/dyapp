import { checkControls, isVisible } from './forward-reachability.js';
import { currentReviewFixture, installReviewFixture } from './review-fixture.js';

const button = (id) => {
  const node = document.getElementById(id);
  if (!(node instanceof HTMLButtonElement)) throw new Error(`Comparison action missing: ${id}`);
  return node;
};
const sameChoices = (before, after) =>
  before.size === after.size && [...before].every(([field, source]) => after.get(field) === source);

/** Runs against the actual static controls, never native write or authorization policy. */
export const checkReviewFixture = async () => {
  const screen = document.documentElement.dataset.screen;
  const original = currentReviewFixture();
  const editable = [
    'link-review-comparison',
    'link-review-ready',
    'link-review-preview',
    'link-review-validation',
    'link-review-equal',
  ].includes(screen);
  if (!original) throw new Error('Comparison did not initialise');
  if (
    ['link-review-comparison', 'link-review-loading'].includes(screen) &&
    (original.selection().size !== 0 ||
      document.querySelector('#review-fields input[type=radio]:checked') !== null)
  )
    throw new Error('Comparison selected answers before an owner decision');
  let checked = original;
  if (!editable) {
    const routes = [];
    checked = installReviewFixture(STRINGS[document.documentElement.lang], screen, (target) =>
      routes.push(target),
    );
    checkBlocked(checked);
    if (routes.length) throw new Error('Unavailable comparison navigated into a save');
  }
  checkValues(checked);
  if (screen === 'link-review-preview')
    checkPreviewValues(checked, STRINGS[document.documentElement.lang]);
  await checkDisclosure(checked);
  await checkControls(
    [...document.querySelectorAll('input[type=radio], button, summary')].filter(
      (node) => isVisible(node) && !node.matches(':disabled'),
    ),
  );
  if (screen === 'link-review-comparison' || screen === 'link-review-equal')
    await checkChoices(screen);
};

const checkPreviewValues = (owner, strings) => {
  const chosen = owner.selection();
  for (const field of owner.fields) {
    const card = document.querySelector(`[data-review-preview-field="${field}"]`);
    const values = [...card.querySelectorAll(':scope > [data-review-source]')].filter(isVisible);
    const source = chosen.get(field);
    if (
      values.length !== 1 ||
      values[0].dataset.reviewSource !== source ||
      values[0].querySelector('p')?.textContent !== strings[`review.sample.${field}.${source}`]
    )
      throw new Error(`Preview changed or truncated the selected source: ${field}`);
  }
};

const checkValues = (owner) => {
  const strings = STRINGS[document.documentElement.lang];
  for (const field of owner.fields) {
    for (const source of ['local', 'remote']) {
      const value = document.getElementById(`review-${field}-${source}-value`);
      const expected = strings[`review.sample.${field}.${source}`];
      if (!expected?.trim() || value?.textContent !== expected)
        throw new Error(`Compared value is missing or truncated: ${field}/${source}`);
    }
  }
};

const checkRetirement = (strings, screen, routes) => {
  const retired = installReviewFixture(strings, screen, (target) => routes.push(target));
  for (const field of retired.fields) retired.choose(field, 'local');
  const prior = retired.selection();
  const next = installReviewFixture(strings, screen, (target) => routes.push(target));
  const fresh = next.selection();
  retired.choose('name', 'remote');
  retired.save();
  if (
    !sameChoices(prior, retired.selection()) ||
    !sameChoices(fresh, next.selection()) ||
    routes.length !== 1
  )
    throw new Error('Retired editable comparison changed an equal successor');
};

const checkBlocked = (owner) => {
  const before = owner.selection();
  owner.choose('name', 'remote');
  owner.save();
  if (!sameChoices(before, owner.selection()) || !button('review-confirm').disabled)
    throw new Error('Unavailable comparison admitted a choice or save');
};

const checkDisclosure = async (owner) => {
  for (const details of document.querySelectorAll('#review-fields details')) {
    if (!isVisible(details)) continue;
    const before = owner.selection();
    const summary = details.querySelector('summary');
    summary.click();
    if (!details.open) throw new Error('Photo references did not open');
    await checkControls([summary]);
    summary.click();
    if (details.open || !sameChoices(before, owner.selection()))
      throw new Error('Reading photo references changed a source choice');
  }
};

const checkChoices = async (screen) => {
  const strings = STRINGS[document.documentElement.lang];
  const routes = [];
  const owner = installReviewFixture(strings, screen, (target) => routes.push(target));
  for (const [index, field] of owner.fields.entries()) {
    if (!button('review-confirm').disabled) throw new Error('Partial field choice enabled save');
    const radio = document.getElementById(`review-${field}-local`);
    radio.click();
    if (owner.selection().size !== index + 1) throw new Error('Explicit source choice was lost');
  }
  if (button('review-confirm').disabled) throw new Error('Complete explicit choices cannot save');
  await checkControls([button('review-preview')]);
  button('review-preview').click();
  if (!isVisible(document.getElementById('review-preview-panel')) || routes.length)
    throw new Error('Read-only preview wrote or did not appear');
  const before = owner.selection();
  checkPreviewValues(owner, strings);
  button('review-edit').click();
  if (!sameChoices(before, owner.selection()))
    throw new Error('Preview discarded explicit choices');
  owner.save();
  owner.save();
  if (routes.length !== 1 || routes[0] !== 'link-review-saving')
    throw new Error('Confirmation admitted more than one transition');
  checkRetirement(strings, screen, routes);
};
