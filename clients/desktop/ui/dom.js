/**
 * @template {Element} T
 * @param {string} id
 * @param {new () => T} Kind
 * @returns {T}
 */
export const byId = (id, Kind) => {
  const node = document.getElementById(id);
  if (!(node instanceof Kind)) throw new Error(`Missing or unexpected element: #${id}`);
  return node;
};

/**
 * @template {Element} T
 * @param {ParentNode} root
 * @param {string} selector
 * @param {new () => T} Kind
 * @returns {T}
 */
export const query = (root, selector, Kind) => {
  const node = root.querySelector(selector);
  if (!(node instanceof Kind)) throw new Error(`Missing or unexpected element: ${selector}`);
  return node;
};

/**
 * @template {Element} T
 * @param {ParentNode} root
 * @param {string} selector
 * @param {new () => T} Kind
 * @returns {T | null}
 */
export const find = (root, selector, Kind) => {
  const node = root.querySelector(selector);
  if (node === null) return null;
  if (!(node instanceof Kind)) throw new Error(`Unexpected element: ${selector}`);
  return node;
};

/**
 * @template {readonly [new () => Element, ...(new () => Element)[]]} Kinds
 * @param {ParentNode} root
 * @param {string} selector
 * @param {Kinds} kinds
 * @returns {NodeListOf<InstanceType<Kinds[number]>>}
 */
export const all = (root, selector, ...kinds) => {
  const nodes = root.querySelectorAll(selector);
  for (const node of nodes) {
    if (!kinds.some((Kind) => node instanceof Kind))
      throw new Error(`Unexpected element: ${selector}`);
  }
  return /** @type {NodeListOf<InstanceType<Kinds[number]>>} */ (nodes);
};

/** @param {Event} event @returns {unknown} */
export const eventDetail = (event) => ('detail' in event ? event.detail : undefined);
