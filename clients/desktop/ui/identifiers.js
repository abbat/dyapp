// One identifier presentation for static markup and asynchronously replaced values.
// WBR nodes add visual chunk boundaries without changing textContent or copied text.
(() => {
  const createIdentifierFormatter = () => {
    const previous = new WeakMap();
    const segmenter =
      typeof Intl.Segmenter === 'function'
        ? new Intl.Segmenter(undefined, { granularity: 'grapheme' })
        : null;
    const charactersForIdentifier = (raw) => {
      // Older engines retain Unicode text intact rather than split an emoji sequence.
      if (segmenter) return [...segmenter.segment(raw)].map((part) => part.segment);
      return [...raw].every((character) => character.codePointAt(0) < 128)
        ? Array.from(raw)
        : [raw];
    };
    const labelIdentifier = (element) => {
      element.classList.add('identifier');
      if (!element.hasAttribute('aria-label') && !element.hasAttribute('aria-labelledby')) {
        element.setAttribute('role', 'group');
        element.setAttribute('aria-label', window.strings['identifier.label']);
      }
    };
    const format = (element) => {
      labelIdentifier(element);
      if (
        [...element.children].some(
          (child) => child.tagName !== 'WBR' && !child.classList.contains('identifier-chunk'),
        )
      )
        return;
      const raw = element.textContent;
      const cached = previous.get(element);
      if (cached?.text === raw && cached.nodes === element.childNodes.length) return;
      const characters = charactersForIdentifier(raw);
      const nodes = [];
      for (let start = 0; start < characters.length; start += 8) {
        if (start) nodes.push(document.createElement('wbr'));
        const chunk = document.createElement('span');
        chunk.className = 'identifier-chunk';
        chunk.textContent = characters.slice(start, start + 8).join('');
        nodes.push(chunk);
      }
      element.replaceChildren(...nodes);
      previous.set(element, { text: raw, nodes: element.childNodes.length });
    };
    return format;
  };
  Promise.resolve(window.stringsShown).then(() => {
    const format = createIdentifierFormatter();
    const codesUnderRoot = (root) => {
      const element = root.nodeType === Node.ELEMENT_NODE ? root : root.parentElement;
      if (!element) return [];
      const owner = element.closest('code');
      return [...(owner ? [owner] : []), ...element.querySelectorAll('code')];
    };
    const update = (roots) => {
      const elements = new Set(roots.flatMap(codesUnderRoot));
      for (const element of elements) format(element);
    };
    const observer = new MutationObserver((records) => {
      observer.disconnect();
      update(records.flatMap((record) => [record.target, ...record.addedNodes]));
      observer.observe(document.body, { subtree: true, childList: true, characterData: true });
    });
    update([document.body]);
    observer.observe(document.body, { subtree: true, childList: true, characterData: true });
    window.identifiersReady = true;
  });
})();
